use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::domain::knowledge::{
    DocumentSearchResult, GenerationDocumentSourceUse, KnowledgeChunk, KnowledgeDocument,
    KnowledgePromptSetting, KnowledgeWorkspace,
};

pub(crate) struct UpsertDocumentInput<'a> {
    pub(crate) workspace_id: &'a str,
    pub(crate) path: &'a str,
    pub(crate) file_name: &'a str,
    pub(crate) extension: &'a str,
    pub(crate) content_hash: &'a str,
    pub(crate) size_bytes: i64,
    pub(crate) modified_at: Option<i64>,
    pub(crate) indexed_at: i64,
}

pub(crate) struct ChunkInsert<'a> {
    pub(crate) id: String,
    pub(crate) document_id: &'a str,
    pub(crate) workspace_id: &'a str,
    pub(crate) path: &'a str,
    pub(crate) file_name: &'a str,
    pub(crate) chunk_index: i64,
    pub(crate) content: &'a str,
    pub(crate) start_byte: i64,
    pub(crate) end_byte: i64,
    pub(crate) start_line: i64,
    pub(crate) end_line: i64,
    pub(crate) token_count_estimate: i64,
    pub(crate) created_at: i64,
}

pub(crate) fn create_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
      CREATE TABLE IF NOT EXISTS workspaces (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        root_path TEXT NOT NULL UNIQUE,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS documents (
        id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
        path TEXT NOT NULL,
        file_name TEXT NOT NULL,
        extension TEXT NOT NULL,
        content_hash TEXT NOT NULL,
        size_bytes INTEGER NOT NULL,
        modified_at INTEGER,
        indexed_at INTEGER NOT NULL,
        deleted_at INTEGER,
        UNIQUE(workspace_id, path)
      );

      CREATE TABLE IF NOT EXISTS document_chunks (
        id TEXT PRIMARY KEY,
        document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        chunk_index INTEGER NOT NULL,
        content TEXT NOT NULL,
        start_byte INTEGER NOT NULL,
        end_byte INTEGER NOT NULL,
        start_line INTEGER NOT NULL,
        end_line INTEGER NOT NULL,
        token_count_estimate INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        UNIQUE(document_id, chunk_index)
      );

      CREATE VIRTUAL TABLE IF NOT EXISTS document_chunk_search
      USING fts5(
        content,
        workspace_id UNINDEXED,
        document_id UNINDEXED,
        chunk_id UNINDEXED,
        path UNINDEXED,
        file_name UNINDEXED,
        chunk_index UNINDEXED,
        start_line UNINDEXED,
        end_line UNINDEXED,
        tokenize = 'unicode61'
      );

      CREATE TABLE IF NOT EXISTS knowledge_prompt_settings (
        conversation_id TEXT PRIMARY KEY REFERENCES chats(id) ON DELETE CASCADE,
        enabled_for_prompt INTEGER NOT NULL DEFAULT 0 CHECK(enabled_for_prompt IN (0, 1)),
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS retrieval_runs (
        id TEXT PRIMARY KEY,
        conversation_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
        generation_run_id TEXT REFERENCES generation_runs(id) ON DELETE SET NULL,
        query TEXT NOT NULL,
        strategy TEXT NOT NULL,
        selected_chunk_ids_json TEXT NOT NULL,
        created_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS generation_document_sources (
        id TEXT PRIMARY KEY,
        generation_run_id TEXT NOT NULL REFERENCES generation_runs(id) ON DELETE CASCADE,
        retrieval_run_id TEXT REFERENCES retrieval_runs(id) ON DELETE SET NULL,
        document_id TEXT,
        chunk_id TEXT,
        source_id TEXT NOT NULL,
        workspace_id TEXT,
        path TEXT NOT NULL,
        file_name TEXT NOT NULL,
        chunk_index INTEGER NOT NULL,
        start_byte INTEGER NOT NULL,
        end_byte INTEGER NOT NULL,
        start_line INTEGER NOT NULL,
        end_line INTEGER NOT NULL,
        content_snapshot TEXT NOT NULL,
        score REAL NOT NULL,
        used_at INTEGER NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_workspaces_updated
        ON workspaces(updated_at DESC);

      CREATE INDEX IF NOT EXISTS idx_documents_workspace_path
        ON documents(workspace_id, path);

      CREATE INDEX IF NOT EXISTS idx_documents_workspace_deleted
        ON documents(workspace_id, deleted_at, indexed_at DESC);

      CREATE INDEX IF NOT EXISTS idx_document_chunks_document
        ON document_chunks(document_id, chunk_index);

      CREATE INDEX IF NOT EXISTS idx_retrieval_runs_conversation
        ON retrieval_runs(conversation_id, created_at DESC);

      CREATE INDEX IF NOT EXISTS idx_generation_document_sources_run
        ON generation_document_sources(generation_run_id, source_id ASC);
      ",
    )
}

fn create_id(conn: &Connection) -> Result<String, rusqlite::Error> {
    conn.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
}

fn read_workspace(row: &Row<'_>) -> Result<KnowledgeWorkspace, rusqlite::Error> {
    Ok(KnowledgeWorkspace {
        id: row.get(0)?,
        name: row.get(1)?,
        root_path: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        document_count: row.get(5)?,
        chunk_count: row.get(6)?,
        last_indexed_at: row.get(7)?,
    })
}

fn read_document(row: &Row<'_>) -> Result<KnowledgeDocument, rusqlite::Error> {
    Ok(KnowledgeDocument {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        path: row.get(2)?,
        file_name: row.get(3)?,
        extension: row.get(4)?,
        content_hash: row.get(5)?,
        size_bytes: row.get(6)?,
        modified_at: row.get(7)?,
        indexed_at: row.get(8)?,
        deleted_at: row.get(9)?,
        chunk_count: row.get(10)?,
    })
}

fn read_chunk(row: &Row<'_>) -> Result<KnowledgeChunk, rusqlite::Error> {
    Ok(KnowledgeChunk {
        id: row.get(0)?,
        document_id: row.get(1)?,
        chunk_index: row.get(2)?,
        content: row.get(3)?,
        start_byte: row.get(4)?,
        end_byte: row.get(5)?,
        start_line: row.get(6)?,
        end_line: row.get(7)?,
        token_count_estimate: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn workspace_select() -> &'static str {
    "
      SELECT
        w.id,
        w.name,
        w.root_path,
        w.created_at,
        w.updated_at,
        COUNT(DISTINCT CASE WHEN d.deleted_at IS NULL THEN d.id END) AS document_count,
        COUNT(dc.id) AS chunk_count,
        MAX(CASE WHEN d.deleted_at IS NULL THEN d.indexed_at END) AS last_indexed_at
      FROM workspaces w
      LEFT JOIN documents d ON d.workspace_id = w.id
      LEFT JOIN document_chunks dc ON dc.document_id = d.id AND d.deleted_at IS NULL
    "
}

fn document_select() -> &'static str {
    "
      SELECT
        d.id,
        d.workspace_id,
        d.path,
        d.file_name,
        d.extension,
        d.content_hash,
        d.size_bytes,
        d.modified_at,
        d.indexed_at,
        d.deleted_at,
        COUNT(dc.id) AS chunk_count
      FROM documents d
      LEFT JOIN document_chunks dc ON dc.document_id = d.id
    "
}

pub(crate) fn upsert_workspace(
    conn: &Connection,
    root_path: &str,
    name: &str,
    now: i64,
) -> Result<KnowledgeWorkspace, rusqlite::Error> {
    let existing_id = conn
        .query_row(
            "SELECT id FROM workspaces WHERE root_path = ?1",
            params![root_path],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let id = existing_id.unwrap_or(create_id(conn)?);

    conn.execute(
        "
      INSERT INTO workspaces (
        id,
        name,
        root_path,
        created_at,
        updated_at
      )
      VALUES (?1, ?2, ?3, ?4, ?4)
      ON CONFLICT(root_path) DO UPDATE SET
        name = excluded.name,
        updated_at = excluded.updated_at
      ",
        params![id, name, root_path, now],
    )?;

    get_workspace(conn, root_path)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn get_workspace(
    conn: &Connection,
    root_path: &str,
) -> Result<Option<KnowledgeWorkspace>, rusqlite::Error> {
    conn.query_row(
        &format!(
            "{} WHERE w.root_path = ?1 GROUP BY w.id",
            workspace_select()
        ),
        params![root_path],
        read_workspace,
    )
    .optional()
}

pub(crate) fn list_workspaces(
    conn: &Connection,
) -> Result<Vec<KnowledgeWorkspace>, rusqlite::Error> {
    let mut statement = conn.prepare(&format!(
        "{} GROUP BY w.id ORDER BY w.updated_at DESC",
        workspace_select()
    ))?;

    let workspaces = statement
        .query_map([], read_workspace)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(workspaces)
}

pub(crate) fn delete_workspace(
    conn: &Connection,
    workspace_id: &str,
) -> Result<bool, rusqlite::Error> {
    conn.execute(
        "
      DELETE FROM document_chunk_search
      WHERE workspace_id = ?1
      ",
        params![workspace_id],
    )?;
    let deleted = conn.execute(
        "DELETE FROM workspaces WHERE id = ?1",
        params![workspace_id],
    )?;

    Ok(deleted > 0)
}

pub(crate) fn list_documents(
    conn: &Connection,
    limit: i64,
) -> Result<Vec<KnowledgeDocument>, rusqlite::Error> {
    let mut statement = conn.prepare(&format!(
        "
      {}
      WHERE d.deleted_at IS NULL
      GROUP BY d.id
      ORDER BY d.indexed_at DESC, d.path ASC
      LIMIT ?1
      ",
        document_select()
    ))?;

    let documents = statement
        .query_map(params![limit.clamp(1, 500)], read_document)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(documents)
}

pub(crate) fn list_workspace_document_paths(
    conn: &Connection,
    workspace_id: &str,
) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "
      SELECT path
      FROM documents
      WHERE workspace_id = ?1
        AND deleted_at IS NULL
      ",
    )?;

    let paths = statement
        .query_map(params![workspace_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(paths)
}

pub(crate) fn find_document(
    conn: &Connection,
    workspace_id: &str,
    path: &str,
) -> Result<Option<KnowledgeDocument>, rusqlite::Error> {
    conn.query_row(
        &format!(
            "
      {}
      WHERE d.workspace_id = ?1
        AND d.path = ?2
      GROUP BY d.id
      ",
            document_select()
        ),
        params![workspace_id, path],
        read_document,
    )
    .optional()
}

pub(crate) fn upsert_document(
    conn: &Connection,
    input: UpsertDocumentInput<'_>,
) -> Result<KnowledgeDocument, rusqlite::Error> {
    let existing_id = conn
        .query_row(
            "
      SELECT id
      FROM documents
      WHERE workspace_id = ?1
        AND path = ?2
      ",
            params![input.workspace_id, input.path],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let id = existing_id.unwrap_or(create_id(conn)?);

    conn.execute(
        "
      INSERT INTO documents (
        id,
        workspace_id,
        path,
        file_name,
        extension,
        content_hash,
        size_bytes,
        modified_at,
        indexed_at,
        deleted_at
      )
      VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)
      ON CONFLICT(workspace_id, path) DO UPDATE SET
        file_name = excluded.file_name,
        extension = excluded.extension,
        content_hash = excluded.content_hash,
        size_bytes = excluded.size_bytes,
        modified_at = excluded.modified_at,
        indexed_at = excluded.indexed_at,
        deleted_at = NULL
      ",
        params![
            id,
            input.workspace_id,
            input.path,
            input.file_name,
            input.extension,
            input.content_hash,
            input.size_bytes,
            input.modified_at,
            input.indexed_at,
        ],
    )?;

    find_document(conn, input.workspace_id, input.path)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn replace_document_chunks(
    conn: &Connection,
    document_id: &str,
    chunks: &[ChunkInsert<'_>],
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM document_chunk_search WHERE document_id = ?1",
        params![document_id],
    )?;
    conn.execute(
        "DELETE FROM document_chunks WHERE document_id = ?1",
        params![document_id],
    )?;

    for chunk in chunks {
        conn.execute(
            "
        INSERT INTO document_chunks (
          id,
          document_id,
          chunk_index,
          content,
          start_byte,
          end_byte,
          start_line,
          end_line,
          token_count_estimate,
          created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ",
            params![
                chunk.id,
                chunk.document_id,
                chunk.chunk_index,
                chunk.content,
                chunk.start_byte,
                chunk.end_byte,
                chunk.start_line,
                chunk.end_line,
                chunk.token_count_estimate,
                chunk.created_at,
            ],
        )?;
        conn.execute(
            "
        INSERT INTO document_chunk_search (
          content,
          workspace_id,
          document_id,
          chunk_id,
          path,
          file_name,
          chunk_index,
          start_line,
          end_line
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ",
            params![
                chunk.content,
                chunk.workspace_id,
                chunk.document_id,
                chunk.id,
                chunk.path,
                chunk.file_name,
                chunk.chunk_index,
                chunk.start_line,
                chunk.end_line,
            ],
        )?;
    }

    Ok(())
}

pub(crate) fn mark_documents_deleted(
    conn: &Connection,
    workspace_id: &str,
    active_paths: &[String],
    deleted_at: i64,
) -> Result<i64, rusqlite::Error> {
    let current_paths = list_workspace_document_paths(conn, workspace_id)?;
    let mut removed = 0;

    for path in current_paths {
        if active_paths.iter().any(|active_path| active_path == &path) {
            continue;
        }

        let document_id = conn.query_row(
            "
        SELECT id
        FROM documents
        WHERE workspace_id = ?1
          AND path = ?2
        ",
            params![workspace_id, path],
            |row| row.get::<_, String>(0),
        )?;
        conn.execute(
            "DELETE FROM document_chunk_search WHERE document_id = ?1",
            params![document_id],
        )?;
        conn.execute(
            "DELETE FROM document_chunks WHERE document_id = ?1",
            params![document_id],
        )?;
        conn.execute(
            "
        UPDATE documents
        SET deleted_at = ?3,
            indexed_at = ?3
        WHERE workspace_id = ?1
          AND path = ?2
        ",
            params![workspace_id, path, deleted_at],
        )?;
        removed += 1;
    }

    Ok(removed)
}

pub(crate) fn get_chunk(
    conn: &Connection,
    chunk_id: &str,
) -> Result<Option<KnowledgeChunk>, rusqlite::Error> {
    conn.query_row(
        "
      SELECT
        id,
        document_id,
        chunk_index,
        content,
        start_byte,
        end_byte,
        start_line,
        end_line,
        token_count_estimate,
        created_at
      FROM document_chunks
      WHERE id = ?1
      ",
        params![chunk_id],
        read_chunk,
    )
    .optional()
}

pub(crate) fn search_documents(
    conn: &Connection,
    fts_query: &str,
    limit: i64,
) -> Result<Vec<DocumentSearchResult>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "
      SELECT
        ds.workspace_id,
        ds.document_id,
        ds.chunk_id,
        ds.path,
        ds.file_name,
        d.extension,
        ds.chunk_index,
        dc.start_byte,
        dc.end_byte,
        ds.start_line,
        ds.end_line,
        dc.content,
        snippet(document_chunk_search, 0, '[', ']', ' ... ', 18) AS snippet,
        bm25(document_chunk_search) AS score
      FROM document_chunk_search ds
      JOIN documents d ON d.id = ds.document_id
      JOIN document_chunks dc ON dc.id = ds.chunk_id
      WHERE document_chunk_search MATCH ?1
        AND d.deleted_at IS NULL
      ORDER BY score ASC
      LIMIT ?2
      ",
    )?;

    let results = statement
        .query_map(params![fts_query, limit.clamp(1, 100)], |row| {
            Ok(DocumentSearchResult {
                workspace_id: row.get(0)?,
                document_id: row.get(1)?,
                chunk_id: row.get(2)?,
                path: row.get(3)?,
                file_name: row.get(4)?,
                extension: row.get(5)?,
                chunk_index: row.get(6)?,
                start_byte: row.get(7)?,
                end_byte: row.get(8)?,
                start_line: row.get(9)?,
                end_line: row.get(10)?,
                content: row.get(11)?,
                snippet: row.get(12)?,
                score: row.get(13)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

pub(crate) fn get_prompt_setting(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Option<KnowledgePromptSetting>, rusqlite::Error> {
    conn.query_row(
        "
      SELECT conversation_id, enabled_for_prompt, created_at, updated_at
      FROM knowledge_prompt_settings
      WHERE conversation_id = ?1
      ",
        params![conversation_id],
        |row| {
            Ok(KnowledgePromptSetting {
                conversation_id: row.get(0)?,
                enabled_for_prompt: row.get::<_, i64>(1)? != 0,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        },
    )
    .optional()
}

pub(crate) fn set_prompt_enabled(
    conn: &Connection,
    conversation_id: &str,
    enabled_for_prompt: bool,
    now: i64,
) -> Result<KnowledgePromptSetting, rusqlite::Error> {
    conn.execute(
        "
      INSERT INTO knowledge_prompt_settings (
        conversation_id,
        enabled_for_prompt,
        created_at,
        updated_at
      )
      VALUES (?1, ?2, ?3, ?3)
      ON CONFLICT(conversation_id) DO UPDATE SET
        enabled_for_prompt = excluded.enabled_for_prompt,
        updated_at = excluded.updated_at
      ",
        params![conversation_id, if enabled_for_prompt { 1 } else { 0 }, now],
    )?;

    get_prompt_setting(conn, conversation_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn insert_retrieval_run(
    conn: &Connection,
    conversation_id: &str,
    generation_run_id: &str,
    query: &str,
    selected_chunk_ids_json: &str,
    created_at: i64,
) -> Result<String, rusqlite::Error> {
    let id = create_id(conn)?;
    conn.execute(
        "
      INSERT INTO retrieval_runs (
        id,
        conversation_id,
        generation_run_id,
        query,
        strategy,
        selected_chunk_ids_json,
        created_at
      )
      VALUES (?1, ?2, ?3, ?4, 'fts5_keyword_v1', ?5, ?6)
      ",
        params![
            id,
            conversation_id,
            generation_run_id,
            query,
            selected_chunk_ids_json,
            created_at,
        ],
    )?;

    Ok(id)
}

pub(crate) fn insert_generation_source_uses(
    conn: &Connection,
    generation_run_id: &str,
    retrieval_run_id: Option<&str>,
    chunks: &[DocumentSearchResult],
    used_at: i64,
) -> Result<Vec<GenerationDocumentSourceUse>, rusqlite::Error> {
    let mut source_use_ids = Vec::with_capacity(chunks.len());

    for (index, chunk) in chunks.iter().enumerate() {
        let id = create_id(conn)?;
        source_use_ids.push(id.clone());
        let source_id = format!("S{}", index + 1);
        conn.execute(
            "
        INSERT INTO generation_document_sources (
          id,
          generation_run_id,
          retrieval_run_id,
          document_id,
          chunk_id,
          source_id,
          workspace_id,
          path,
          file_name,
          chunk_index,
          start_byte,
          end_byte,
          start_line,
          end_line,
          content_snapshot,
          score,
          used_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        ",
            params![
                id,
                generation_run_id,
                retrieval_run_id,
                chunk.document_id,
                chunk.chunk_id,
                source_id,
                chunk.workspace_id,
                chunk.path,
                chunk.file_name,
                chunk.chunk_index,
                chunk.start_byte,
                chunk.end_byte,
                chunk.start_line,
                chunk.end_line,
                chunk.content,
                chunk.score,
                used_at,
            ],
        )?;
    }

    source_use_ids
        .into_iter()
        .map(|source_use_id| get_generation_source_use(conn, &source_use_id))
        .collect()
}

fn get_generation_source_use(
    conn: &Connection,
    source_use_id: &str,
) -> Result<GenerationDocumentSourceUse, rusqlite::Error> {
    conn.query_row(
        "
      SELECT
        id,
        generation_run_id,
        retrieval_run_id,
        document_id,
        chunk_id,
        source_id,
        workspace_id,
        path,
        file_name,
        chunk_index,
        start_byte,
        end_byte,
        start_line,
        end_line,
        content_snapshot,
        score,
        used_at
      FROM generation_document_sources
      WHERE id = ?1
      ",
        params![source_use_id],
        read_generation_source_use,
    )
}

fn read_generation_source_use(
    row: &Row<'_>,
) -> Result<GenerationDocumentSourceUse, rusqlite::Error> {
    Ok(GenerationDocumentSourceUse {
        id: row.get(0)?,
        generation_run_id: row.get(1)?,
        retrieval_run_id: row.get(2)?,
        document_id: row.get(3)?,
        chunk_id: row.get(4)?,
        source_id: row.get(5)?,
        workspace_id: row.get(6)?,
        path: row.get(7)?,
        file_name: row.get(8)?,
        chunk_index: row.get(9)?,
        start_byte: row.get(10)?,
        end_byte: row.get(11)?,
        start_line: row.get(12)?,
        end_line: row.get(13)?,
        content: row.get(14)?,
        score: row.get(15)?,
        used_at: row.get(16)?,
    })
}

pub(crate) fn list_generation_source_uses(
    conn: &Connection,
    generation_run_id: &str,
) -> Result<Vec<GenerationDocumentSourceUse>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "
      SELECT
        id,
        generation_run_id,
        retrieval_run_id,
        document_id,
        chunk_id,
        source_id,
        workspace_id,
        path,
        file_name,
        chunk_index,
        start_byte,
        end_byte,
        start_line,
        end_line,
        content_snapshot,
        score,
        used_at
      FROM generation_document_sources
      WHERE generation_run_id = ?1
      ORDER BY source_id ASC
      ",
    )?;

    let source_uses = statement
        .query_map(params![generation_run_id], read_generation_source_use)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(source_uses)
}
