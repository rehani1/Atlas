use serde::Serialize;

use crate::domain::{
    database::DatabaseDiagnostics,
    job::{JobStatus, JobType},
    model::OllamaStatus,
};

#[derive(Serialize)]
pub(crate) struct DiagnosticsCenter {
    pub(crate) app_version: String,
    pub(crate) generated_at: i64,
    pub(crate) ollama: OllamaStatus,
    pub(crate) database: DatabaseDiagnostics,
    pub(crate) jobs: DiagnosticsJobs,
    pub(crate) knowledge: DiagnosticsKnowledge,
    pub(crate) model_speeds: Vec<DiagnosticsModelSpeed>,
    pub(crate) recent_errors: Vec<DiagnosticsError>,
    pub(crate) copy_summary: String,
}

#[derive(Serialize)]
pub(crate) struct DiagnosticsJobs {
    pub(crate) running: Vec<DiagnosticsJob>,
    pub(crate) recent_failed: Vec<DiagnosticsJob>,
    pub(crate) embedding_queue: DiagnosticsQueue,
}

#[derive(Serialize)]
pub(crate) struct DiagnosticsJob {
    pub(crate) id: String,
    pub(crate) job_type: JobType,
    pub(crate) status: JobStatus,
    pub(crate) label: String,
    pub(crate) progress_current: Option<i64>,
    pub(crate) progress_total: Option<i64>,
    pub(crate) error_message: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) started_at: Option<i64>,
    pub(crate) completed_at: Option<i64>,
}

#[derive(Serialize)]
pub(crate) struct DiagnosticsQueue {
    pub(crate) queued: i64,
    pub(crate) running: i64,
    pub(crate) cancelling: i64,
    pub(crate) recent_failed: i64,
}

#[derive(Serialize)]
pub(crate) struct DiagnosticsKnowledge {
    pub(crate) workspace_count: i64,
    pub(crate) document_count: i64,
    pub(crate) indexed_document_count: i64,
    pub(crate) deleted_document_count: i64,
    pub(crate) chunk_count: i64,
    pub(crate) last_indexed_at: Option<i64>,
}

#[derive(Serialize)]
pub(crate) struct DiagnosticsModelSpeed {
    pub(crate) model_name: String,
    pub(crate) generation_count: i64,
    pub(crate) average_tokens_per_second: Option<f64>,
    pub(crate) last_used_at: Option<i64>,
    pub(crate) benchmark_count: i64,
    pub(crate) benchmark_average_tokens_per_second: Option<f64>,
}

#[derive(Serialize)]
pub(crate) struct DiagnosticsError {
    pub(crate) source: String,
    pub(crate) label: String,
    pub(crate) message: String,
    pub(crate) occurred_at: i64,
}
