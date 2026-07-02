use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JobType {
    ChatGeneration,
    ModelPull,
    ModelDelete,
    ExportConversation,
    DocumentImport,
    EmbeddingIndex,
    ModelBenchmark,
    ConversationSummary,
}

impl JobType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            JobType::ChatGeneration => "chat_generation",
            JobType::ModelPull => "model_pull",
            JobType::ModelDelete => "model_delete",
            JobType::ExportConversation => "export_conversation",
            JobType::DocumentImport => "document_import",
            JobType::EmbeddingIndex => "embedding_index",
            JobType::ModelBenchmark => "model_benchmark",
            JobType::ConversationSummary => "conversation_summary",
        }
    }

    pub(crate) fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "chat_generation" => Ok(JobType::ChatGeneration),
            "model_pull" => Ok(JobType::ModelPull),
            "model_delete" => Ok(JobType::ModelDelete),
            "export_conversation" => Ok(JobType::ExportConversation),
            "document_import" => Ok(JobType::DocumentImport),
            "embedding_index" => Ok(JobType::EmbeddingIndex),
            "model_benchmark" => Ok(JobType::ModelBenchmark),
            "conversation_summary" => Ok(JobType::ConversationSummary),
            _ => Err(format!("Unknown job type: {value}")),
        }
    }
}

impl fmt::Display for JobType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JobStatus {
    Queued,
    Running,
    Cancelling,
    Cancelled,
    Succeeded,
    Failed,
}

impl JobStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Cancelling => "cancelling",
            JobStatus::Cancelled => "cancelled",
            JobStatus::Succeeded => "succeeded",
            JobStatus::Failed => "failed",
        }
    }

    pub(crate) fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "queued" => Ok(JobStatus::Queued),
            "running" => Ok(JobStatus::Running),
            "cancelling" => Ok(JobStatus::Cancelling),
            "cancelled" => Ok(JobStatus::Cancelled),
            "succeeded" => Ok(JobStatus::Succeeded),
            "failed" => Ok(JobStatus::Failed),
            _ => Err(format!("Unknown job status: {value}")),
        }
    }
}

impl fmt::Display for JobStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Job {
    pub(crate) id: String,
    pub(crate) job_type: JobType,
    pub(crate) status: JobStatus,
    pub(crate) progress_current: Option<i64>,
    pub(crate) progress_total: Option<i64>,
    pub(crate) label: String,
    pub(crate) payload_json: Option<String>,
    pub(crate) result_json: Option<String>,
    pub(crate) error_message: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) started_at: Option<i64>,
    pub(crate) completed_at: Option<i64>,
    pub(crate) cancelled_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct JobEvent {
    pub(crate) job_id: String,
    pub(crate) job_type: JobType,
    pub(crate) job: Job,
}
