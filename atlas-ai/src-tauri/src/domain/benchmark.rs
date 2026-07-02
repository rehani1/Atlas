use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ModelBenchmarkStatus {
    Queued,
    Running,
    Completed,
    Cancelled,
    Failed,
}

impl ModelBenchmarkStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ModelBenchmarkStatus::Queued => "queued",
            ModelBenchmarkStatus::Running => "running",
            ModelBenchmarkStatus::Completed => "completed",
            ModelBenchmarkStatus::Cancelled => "cancelled",
            ModelBenchmarkStatus::Failed => "failed",
        }
    }

    pub(crate) fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "queued" => Ok(ModelBenchmarkStatus::Queued),
            "running" => Ok(ModelBenchmarkStatus::Running),
            "completed" => Ok(ModelBenchmarkStatus::Completed),
            "cancelled" => Ok(ModelBenchmarkStatus::Cancelled),
            "failed" => Ok(ModelBenchmarkStatus::Failed),
            _ => Err(format!("Unknown model benchmark status: {value}")),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ModelBenchmark {
    pub(crate) id: String,
    pub(crate) job_id: String,
    pub(crate) model_name: String,
    pub(crate) prompt_type: String,
    pub(crate) prompt_label: String,
    pub(crate) prompt_text_hash: String,
    pub(crate) started_at: Option<i64>,
    pub(crate) completed_at: Option<i64>,
    pub(crate) status: ModelBenchmarkStatus,
    pub(crate) total_duration_ms: Option<i64>,
    pub(crate) first_token_ms: Option<i64>,
    pub(crate) prompt_eval_count: Option<i64>,
    pub(crate) prompt_eval_duration_ms: Option<i64>,
    pub(crate) eval_count: Option<i64>,
    pub(crate) eval_duration_ms: Option<i64>,
    pub(crate) tokens_per_second: Option<f64>,
    pub(crate) error_message: Option<String>,
    pub(crate) created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ModelUsage {
    pub(crate) model_name: String,
    pub(crate) last_used_at: Option<i64>,
    pub(crate) generation_count: i64,
}
