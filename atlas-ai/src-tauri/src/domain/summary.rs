use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ConversationSummary {
    pub(crate) id: String,
    pub(crate) conversation_id: String,
    pub(crate) summary: String,
    pub(crate) source_message_start_id: Option<i64>,
    pub(crate) source_message_end_id: Option<i64>,
    pub(crate) model_name: String,
    pub(crate) version: i64,
    pub(crate) enabled_for_prompt: bool,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct SummarySourceMessage {
    pub(crate) id: i64,
    pub(crate) role: String,
    pub(crate) content: String,
}
