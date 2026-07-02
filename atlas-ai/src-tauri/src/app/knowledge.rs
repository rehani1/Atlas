use rusqlite::Connection;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::UNIX_EPOCH,
};

use crate::{
    domain::knowledge::{
        DocumentIndexStats, DocumentSearchResult, GenerationDocumentSourceUse, KnowledgeChunk,
        KnowledgeDocument, KnowledgePromptSetting, KnowledgeWorkspace,
    },
    infra::knowledge,
};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const TARGET_CHUNK_CHARS: usize = 1_400;
const MAX_CHUNK_CHARS: usize = 2_400;
const PROMPT_SEARCH_LIMIT: i64 = 32;
const PROMPT_CHUNK_LIMIT: usize = 8;
const PROMPT_CHUNKS_PER_DOCUMENT: usize = 2;

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "md", "txt", "json", "rs", "ts", "tsx", "js", "jsx", "py", "toml", "yaml", "yml",
];

const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "can", "do", "does", "for", "from",
    "how", "i", "in", "is", "it", "of", "on", "or", "that", "the", "this", "to", "what", "when",
    "where", "which", "who", "why", "with", "you",
];

#[derive(Clone, Debug)]
pub(crate) struct ValidatedKnowledgePath {
    pub(crate) root_path: PathBuf,
    pub(crate) name: String,
    pub(crate) is_file: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedChunk {
    pub(crate) chunk_index: i64,
    pub(crate) content: String,
    pub(crate) start_byte: i64,
    pub(crate) end_byte: i64,
    pub(crate) start_line: i64,
    pub(crate) end_line: i64,
    pub(crate) token_count_estimate: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedDocument {
    pub(crate) path: String,
    pub(crate) file_name: String,
    pub(crate) extension: String,
    pub(crate) content_hash: String,
    pub(crate) size_bytes: i64,
    pub(crate) modified_at: Option<i64>,
    pub(crate) chunks: Vec<PreparedChunk>,
}

#[derive(Clone, Debug)]
pub(crate) enum FileIndexOutcome {
    Indexed { path: String, chunk_count: i64 },
    Unchanged { path: String },
    Skipped,
}

pub(crate) fn supported_extensions() -> &'static [&'static str] {
    SUPPORTED_EXTENSIONS
}

pub(crate) fn validate_path(input: &str) -> Result<ValidatedKnowledgePath, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Enter a folder or file path to index.".to_string());
    }

    let root_path = fs::canonicalize(trimmed)
        .map_err(|error| format!("Could not access that path: {error}"))?;
    let metadata = fs::symlink_metadata(&root_path).map_err(|error| error.to_string())?;

    if metadata.file_type().is_symlink() {
        return Err("Symlink roots are not indexed.".to_string());
    }

    let is_file = metadata.is_file();
    if !is_file && !metadata.is_dir() {
        return Err("Only folders and files can be indexed.".to_string());
    }

    if is_file && extension_for_path(&root_path).is_none() {
        return Err(format!(
            "That file type is not supported. Supported extensions: {}",
            SUPPORTED_EXTENSIONS.join(", ")
        ));
    }

    let name = root_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("Knowledge workspace")
        .to_string();

    Ok(ValidatedKnowledgePath {
        root_path,
        name,
        is_file,
    })
}

pub(crate) fn discover_files(
    validated_path: &ValidatedKnowledgePath,
    cancellation: &AtomicBool,
) -> Result<Vec<PathBuf>, String> {
    if cancellation.load(Ordering::SeqCst) {
        return Err("Job cancelled".to_string());
    }

    if validated_path.is_file {
        return Ok(vec![validated_path.root_path.clone()]);
    }

    let mut files = Vec::new();
    collect_supported_files(
        &validated_path.root_path,
        &validated_path.root_path,
        &mut files,
        cancellation,
    )?;
    files.sort();

    Ok(files)
}

fn collect_supported_files(
    root_path: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
    cancellation: &AtomicBool,
) -> Result<(), String> {
    if cancellation.load(Ordering::SeqCst) {
        return Err("Job cancelled".to_string());
    }

    let entries = fs::read_dir(directory)
        .map_err(|error| format!("Could not read {}: {error}", directory.display()))?;

    for entry in entries {
        if cancellation.load(Ordering::SeqCst) {
            return Err("Job cancelled".to_string());
        }

        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;

        if metadata.file_type().is_symlink() {
            continue;
        }

        if metadata.is_dir() {
            if file_name.starts_with('.') {
                continue;
            }
            collect_supported_files(root_path, &path, files, cancellation)?;
            continue;
        }

        if !metadata.is_file() || extension_for_path(&path).is_none() {
            continue;
        }

        let canonical_path = fs::canonicalize(&path).map_err(|error| error.to_string())?;
        if canonical_path.starts_with(root_path) {
            files.push(canonical_path);
        }
    }

    Ok(())
}

pub(crate) fn create_or_update_workspace(
    conn: &Connection,
    validated_path: &ValidatedKnowledgePath,
    now: i64,
) -> Result<KnowledgeWorkspace, String> {
    knowledge::upsert_workspace(
        conn,
        &validated_path.root_path.display().to_string(),
        &validated_path.name,
        now,
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn index_file(
    conn: &Connection,
    workspace_id: &str,
    file_path: &Path,
    now: i64,
) -> Result<FileIndexOutcome, String> {
    let prepared = match prepare_document(file_path)? {
        Some(prepared) => prepared,
        None => return Ok(FileIndexOutcome::Skipped),
    };

    let existing = knowledge::find_document(conn, workspace_id, &prepared.path)
        .map_err(|error| error.to_string())?;

    if existing.as_ref().is_some_and(|document| {
        document.deleted_at.is_none()
            && document.content_hash == prepared.content_hash
            && document.chunk_count > 0
    }) {
        let _ = knowledge::upsert_document(
            conn,
            knowledge::UpsertDocumentInput {
                workspace_id,
                path: &prepared.path,
                file_name: &prepared.file_name,
                extension: &prepared.extension,
                content_hash: &prepared.content_hash,
                size_bytes: prepared.size_bytes,
                modified_at: prepared.modified_at,
                indexed_at: now,
            },
        )
        .map_err(|error| error.to_string())?;

        return Ok(FileIndexOutcome::Unchanged {
            path: prepared.path,
        });
    }

    let document = knowledge::upsert_document(
        conn,
        knowledge::UpsertDocumentInput {
            workspace_id,
            path: &prepared.path,
            file_name: &prepared.file_name,
            extension: &prepared.extension,
            content_hash: &prepared.content_hash,
            size_bytes: prepared.size_bytes,
            modified_at: prepared.modified_at,
            indexed_at: now,
        },
    )
    .map_err(|error| error.to_string())?;
    let chunks = prepared
        .chunks
        .iter()
        .map(|chunk| knowledge::ChunkInsert {
            id: chunk_id(&document.id, chunk.chunk_index),
            document_id: &document.id,
            workspace_id,
            path: &prepared.path,
            file_name: &prepared.file_name,
            chunk_index: chunk.chunk_index,
            content: &chunk.content,
            start_byte: chunk.start_byte,
            end_byte: chunk.end_byte,
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            token_count_estimate: chunk.token_count_estimate,
            created_at: now,
        })
        .collect::<Vec<_>>();
    let chunk_count = chunks.len() as i64;
    knowledge::replace_document_chunks(conn, &document.id, &chunks)
        .map_err(|error| error.to_string())?;

    Ok(FileIndexOutcome::Indexed {
        path: prepared.path,
        chunk_count,
    })
}

pub(crate) fn mark_missing_documents_deleted(
    conn: &Connection,
    workspace_id: &str,
    active_paths: &[String],
    now: i64,
) -> Result<i64, String> {
    knowledge::mark_documents_deleted(conn, workspace_id, active_paths, now)
        .map_err(|error| error.to_string())
}

pub(crate) fn list_workspaces(conn: &Connection) -> Result<Vec<KnowledgeWorkspace>, String> {
    knowledge::list_workspaces(conn).map_err(|error| error.to_string())
}

pub(crate) fn remove_workspace(conn: &Connection, workspace_id: &str) -> Result<bool, String> {
    knowledge::delete_workspace(conn, workspace_id).map_err(|error| error.to_string())
}

pub(crate) fn list_documents(
    conn: &Connection,
    limit: Option<i64>,
) -> Result<Vec<KnowledgeDocument>, String> {
    knowledge::list_documents(conn, limit.unwrap_or(100)).map_err(|error| error.to_string())
}

pub(crate) fn get_chunk(conn: &Connection, chunk_id: &str) -> Result<KnowledgeChunk, String> {
    knowledge::get_chunk(conn, chunk_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Document chunk was not found.".to_string())
}

pub(crate) fn search_documents(
    conn: &Connection,
    query: &str,
    limit: Option<i64>,
) -> Result<Vec<DocumentSearchResult>, String> {
    let fts_query = build_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    knowledge::search_documents(conn, &fts_query, limit.unwrap_or(20)).map_err(|error| {
        format!(
            "Could not search indexed documents. Query terms: {}. {error}",
            fts_query
        )
    })
}

pub(crate) fn prompt_setting(
    conn: &Connection,
    conversation_id: &str,
) -> Result<KnowledgePromptSetting, String> {
    knowledge::get_prompt_setting(conn, conversation_id)
        .map_err(|error| error.to_string())?
        .map(Ok)
        .unwrap_or_else(|| {
            Ok(KnowledgePromptSetting {
                conversation_id: conversation_id.to_string(),
                enabled_for_prompt: false,
                created_at: 0,
                updated_at: 0,
            })
        })
}

pub(crate) fn set_prompt_enabled(
    conn: &Connection,
    conversation_id: &str,
    enabled_for_prompt: bool,
    now: i64,
) -> Result<KnowledgePromptSetting, String> {
    knowledge::set_prompt_enabled(conn, conversation_id, enabled_for_prompt, now)
        .map_err(|error| error.to_string())
}

pub(crate) fn retrieve_prompt_chunks(
    conn: &Connection,
    query: &str,
) -> Result<Vec<DocumentSearchResult>, String> {
    let mut results = search_documents(conn, query, Some(PROMPT_SEARCH_LIMIT))?;
    let mut counts_by_document: HashMap<String, usize> = HashMap::new();
    let mut selected = Vec::new();

    for result in results.drain(..) {
        let count = counts_by_document
            .entry(result.document_id.clone())
            .or_insert(0);
        if *count >= PROMPT_CHUNKS_PER_DOCUMENT {
            continue;
        }

        *count += 1;
        selected.push(result);

        if selected.len() >= PROMPT_CHUNK_LIMIT {
            break;
        }
    }

    Ok(selected)
}

pub(crate) fn record_generation_sources(
    conn: &Connection,
    conversation_id: &str,
    generation_run_id: &str,
    query: &str,
    chunks: &[DocumentSearchResult],
    used_at: i64,
) -> Result<Vec<GenerationDocumentSourceUse>, String> {
    if chunks.is_empty() {
        return Ok(Vec::new());
    }

    let selected_chunk_ids_json = serde_json::to_string(
        &chunks
            .iter()
            .map(|chunk| chunk.chunk_id.as_str())
            .collect::<Vec<_>>(),
    )
    .map_err(|error| error.to_string())?;
    let retrieval_run_id = knowledge::insert_retrieval_run(
        conn,
        conversation_id,
        generation_run_id,
        query,
        &selected_chunk_ids_json,
        used_at,
    )
    .map_err(|error| error.to_string())?;

    knowledge::insert_generation_source_uses(
        conn,
        generation_run_id,
        Some(&retrieval_run_id),
        chunks,
        used_at,
    )
    .map_err(|error| error.to_string())
}

fn prepare_document(file_path: &Path) -> Result<Option<PreparedDocument>, String> {
    let canonical_path = fs::canonicalize(file_path).map_err(|error| error.to_string())?;
    let extension = match extension_for_path(&canonical_path) {
        Some(extension) => extension,
        None => return Ok(None),
    };
    let metadata = fs::metadata(&canonical_path).map_err(|error| error.to_string())?;

    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Ok(None);
    }

    let bytes = fs::read(&canonical_path).map_err(|error| error.to_string())?;
    if bytes.contains(&0) {
        return Ok(None);
    }

    let content = match String::from_utf8(bytes.clone()) {
        Ok(content) => content,
        Err(_) => return Ok(None),
    };
    let content = content.replace("\r\n", "\n");
    let chunks = chunk_text(&content);

    if chunks.is_empty() {
        return Ok(None);
    }

    let file_name = canonical_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document")
        .to_string();

    Ok(Some(PreparedDocument {
        path: canonical_path.display().to_string(),
        file_name,
        extension,
        content_hash: hash_bytes(&bytes),
        size_bytes: metadata.len() as i64,
        modified_at: modified_at_ms(&metadata),
        chunks,
    }))
}

fn extension_for_path(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    SUPPORTED_EXTENSIONS
        .iter()
        .any(|supported| *supported == extension)
        .then_some(extension)
}

fn modified_at_ms(metadata: &fs::Metadata) -> Option<i64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;

    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }

    format!("{hash:016x}")
}

fn chunk_id(document_id: &str, chunk_index: i64) -> String {
    format!("{document_id}:{chunk_index:04}")
}

fn chunk_text(content: &str) -> Vec<PreparedChunk> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_start_byte = 0_i64;
    let mut current_start_line = 1_i64;
    let mut byte_offset = 0_i64;
    let mut line_number = 1_i64;

    for segment in content.split_inclusive('\n') {
        let segment_len = segment.len() as i64;
        let trimmed = segment.trim();
        let starts_boundary = is_code_boundary(trimmed);
        let should_flush_before = !current.trim().is_empty()
            && (current.chars().count() + segment.chars().count() > MAX_CHUNK_CHARS
                || (starts_boundary && current.chars().count() >= TARGET_CHUNK_CHARS));

        if should_flush_before {
            push_chunk(
                &mut chunks,
                &mut current,
                current_start_byte,
                byte_offset,
                current_start_line,
                line_number.saturating_sub(1),
            );
            current_start_byte = byte_offset;
            current_start_line = line_number;
        }

        current.push_str(segment);
        byte_offset += segment_len;

        if segment.ends_with('\n') {
            let is_paragraph_boundary = trimmed.is_empty();
            if is_paragraph_boundary && current.chars().count() >= TARGET_CHUNK_CHARS {
                push_chunk(
                    &mut chunks,
                    &mut current,
                    current_start_byte,
                    byte_offset,
                    current_start_line,
                    line_number,
                );
                current_start_byte = byte_offset;
                current_start_line = line_number + 1;
            }
            line_number += 1;
        }
    }

    if !current.trim().is_empty() {
        push_chunk(
            &mut chunks,
            &mut current,
            current_start_byte,
            content.len() as i64,
            current_start_line,
            line_number,
        );
    }

    chunks
}

fn push_chunk(
    chunks: &mut Vec<PreparedChunk>,
    current: &mut String,
    start_byte: i64,
    end_byte: i64,
    start_line: i64,
    end_line: i64,
) {
    let content = current.trim().to_string();
    current.clear();

    if content.is_empty() {
        return;
    }

    chunks.push(PreparedChunk {
        chunk_index: chunks.len() as i64,
        token_count_estimate: ((content.chars().count() as f64) / 4.0).ceil() as i64,
        content,
        start_byte,
        end_byte,
        start_line,
        end_line: end_line.max(start_line),
    });
}

fn is_code_boundary(line: &str) -> bool {
    [
        "fn ",
        "pub fn ",
        "async fn ",
        "pub async fn ",
        "def ",
        "class ",
        "function ",
        "export function ",
        "export default function ",
        "const ",
        "export const ",
        "impl ",
        "interface ",
        "type ",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn build_fts_query(query: &str) -> String {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for character in query.chars() {
        if character.is_alphanumeric() || character == '_' {
            current.push(character.to_ascii_lowercase());
            continue;
        }

        push_query_token(&mut tokens, &mut current);
    }

    push_query_token(&mut tokens, &mut current);

    tokens
        .into_iter()
        .take(12)
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn push_query_token(tokens: &mut Vec<String>, current: &mut String) {
    if current.chars().count() < 3 {
        current.clear();
        return;
    }

    if !STOP_WORDS.iter().any(|stop_word| *stop_word == current) && !tokens.contains(current) {
        tokens.push(current.clone());
    }

    current.clear();
}

pub(crate) fn empty_index_stats(workspace_id: &str) -> DocumentIndexStats {
    DocumentIndexStats {
        workspace_id: workspace_id.to_string(),
        discovered_files: 0,
        indexed_files: 0,
        unchanged_files: 0,
        skipped_files: 0,
        removed_files: 0,
        chunk_count: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_fts_query, chunk_text, hash_bytes, validate_path};

    #[test]
    fn chunk_text_keeps_source_ranges() {
        let content = "Title\n\n".to_string() + &"paragraph line\n".repeat(260);
        let chunks = chunk_text(&content);

        assert!(chunks.len() > 1);
        assert_eq!(chunks[0].start_line, 1);
        assert!(chunks[0].end_byte > chunks[0].start_byte);
        assert!(chunks.iter().all(|chunk| !chunk.content.trim().is_empty()));
    }

    #[test]
    fn fts_query_uses_non_stopword_terms() {
        assert_eq!(
            build_fts_query("What does the model service do?"),
            "\"model\" OR \"service\""
        );
    }

    #[test]
    fn hash_bytes_is_deterministic() {
        assert_eq!(hash_bytes(b"atlas"), hash_bytes(b"atlas"));
        assert_ne!(hash_bytes(b"atlas"), hash_bytes(b"Atlas"));
    }

    #[test]
    fn validate_path_rejects_empty_paths() {
        assert!(validate_path("   ").is_err());
    }
}
