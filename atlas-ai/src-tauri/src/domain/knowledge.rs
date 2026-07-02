use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct KnowledgeWorkspace {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) root_path: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) document_count: i64,
    pub(crate) chunk_count: i64,
    pub(crate) last_indexed_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct KnowledgeDocument {
    pub(crate) id: String,
    pub(crate) workspace_id: String,
    pub(crate) path: String,
    pub(crate) file_name: String,
    pub(crate) extension: String,
    pub(crate) content_hash: String,
    pub(crate) size_bytes: i64,
    pub(crate) modified_at: Option<i64>,
    pub(crate) indexed_at: i64,
    pub(crate) deleted_at: Option<i64>,
    pub(crate) chunk_count: i64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct KnowledgeChunk {
    pub(crate) id: String,
    pub(crate) document_id: String,
    pub(crate) chunk_index: i64,
    pub(crate) content: String,
    pub(crate) start_byte: i64,
    pub(crate) end_byte: i64,
    pub(crate) start_line: i64,
    pub(crate) end_line: i64,
    pub(crate) token_count_estimate: i64,
    pub(crate) created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct KnowledgePromptSetting {
    pub(crate) conversation_id: String,
    pub(crate) enabled_for_prompt: bool,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DocumentSearchResult {
    pub(crate) workspace_id: String,
    pub(crate) document_id: String,
    pub(crate) chunk_id: String,
    pub(crate) path: String,
    pub(crate) file_name: String,
    pub(crate) extension: String,
    pub(crate) chunk_index: i64,
    pub(crate) start_byte: i64,
    pub(crate) end_byte: i64,
    pub(crate) start_line: i64,
    pub(crate) end_line: i64,
    pub(crate) content: String,
    pub(crate) snippet: String,
    pub(crate) score: f64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct GenerationDocumentSourceUse {
    pub(crate) id: String,
    pub(crate) generation_run_id: String,
    pub(crate) retrieval_run_id: Option<String>,
    pub(crate) document_id: Option<String>,
    pub(crate) chunk_id: Option<String>,
    pub(crate) source_id: String,
    pub(crate) workspace_id: Option<String>,
    pub(crate) path: String,
    pub(crate) file_name: String,
    pub(crate) chunk_index: i64,
    pub(crate) start_byte: i64,
    pub(crate) end_byte: i64,
    pub(crate) start_line: i64,
    pub(crate) end_line: i64,
    pub(crate) content: String,
    pub(crate) score: f64,
    pub(crate) used_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DocumentIndexStats {
    pub(crate) workspace_id: String,
    pub(crate) discovered_files: i64,
    pub(crate) indexed_files: i64,
    pub(crate) unchanged_files: i64,
    pub(crate) skipped_files: i64,
    pub(crate) removed_files: i64,
    pub(crate) chunk_count: i64,
}
