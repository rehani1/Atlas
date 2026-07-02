use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GenerationContextItemType {
    SystemPrompt,
    Summary,
    Memory,
    PriorMessage,
    DocumentChunk,
    UserMessage,
    ModelOptions,
    TruncationNotice,
}

impl GenerationContextItemType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            GenerationContextItemType::SystemPrompt => "system_prompt",
            GenerationContextItemType::Summary => "summary",
            GenerationContextItemType::Memory => "memory",
            GenerationContextItemType::PriorMessage => "prior_message",
            GenerationContextItemType::DocumentChunk => "document_chunk",
            GenerationContextItemType::UserMessage => "user_message",
            GenerationContextItemType::ModelOptions => "model_options",
            GenerationContextItemType::TruncationNotice => "truncation_notice",
        }
    }

    pub(crate) fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "system_prompt" => Ok(GenerationContextItemType::SystemPrompt),
            "summary" => Ok(GenerationContextItemType::Summary),
            "memory" => Ok(GenerationContextItemType::Memory),
            "prior_message" => Ok(GenerationContextItemType::PriorMessage),
            "document_chunk" => Ok(GenerationContextItemType::DocumentChunk),
            "user_message" => Ok(GenerationContextItemType::UserMessage),
            "model_options" => Ok(GenerationContextItemType::ModelOptions),
            "truncation_notice" => Ok(GenerationContextItemType::TruncationNotice),
            _ => Err(format!("Unknown generation context item type: {value}")),
        }
    }
}

impl fmt::Display for GenerationContextItemType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct GenerationContextItem {
    pub(crate) id: String,
    pub(crate) generation_run_id: String,
    pub(crate) item_type: GenerationContextItemType,
    pub(crate) item_id: Option<String>,
    pub(crate) label: String,
    pub(crate) token_count_estimate: i64,
    pub(crate) order_index: i64,
    pub(crate) metadata_json: Option<String>,
    pub(crate) created_at: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct GenerationContextItemDraft {
    pub(crate) item_type: GenerationContextItemType,
    pub(crate) item_id: Option<String>,
    pub(crate) label: String,
    pub(crate) token_count_estimate: i64,
    pub(crate) order_index: i64,
    pub(crate) metadata_json: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ContextPromptMessage {
    pub(crate) role: String,
    pub(crate) content: String,
}

#[derive(Clone, Debug)]
pub(crate) struct AssembledContext {
    pub(crate) messages: Vec<ContextPromptMessage>,
    pub(crate) items: Vec<GenerationContextItemDraft>,
}
