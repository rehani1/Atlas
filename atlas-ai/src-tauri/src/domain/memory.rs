use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MemoryScopeType {
    Global,
    Conversation,
    Project,
}

impl MemoryScopeType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            MemoryScopeType::Global => "global",
            MemoryScopeType::Conversation => "conversation",
            MemoryScopeType::Project => "project",
        }
    }

    pub(crate) fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "global" => Ok(MemoryScopeType::Global),
            "conversation" => Ok(MemoryScopeType::Conversation),
            "project" => Ok(MemoryScopeType::Project),
            _ => Err(format!("Unknown memory scope: {value}")),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Memory {
    pub(crate) id: String,
    pub(crate) scope_type: MemoryScopeType,
    pub(crate) scope_id: Option<String>,
    pub(crate) content: String,
    pub(crate) source_conversation_id: Option<String>,
    pub(crate) source_message_id: Option<i64>,
    pub(crate) confidence: Option<f64>,
    pub(crate) pinned: bool,
    pub(crate) archived_at: Option<i64>,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct MemoryPromptSetting {
    pub(crate) conversation_id: String,
    pub(crate) enabled_for_prompt: bool,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct PromptMemoryUse {
    pub(crate) id: String,
    pub(crate) generation_run_id: String,
    pub(crate) memory_id: Option<String>,
    pub(crate) content: String,
    pub(crate) scope_type: MemoryScopeType,
    pub(crate) scope_id: Option<String>,
    pub(crate) source_conversation_id: Option<String>,
    pub(crate) source_message_id: Option<i64>,
    pub(crate) used_at: i64,
}
