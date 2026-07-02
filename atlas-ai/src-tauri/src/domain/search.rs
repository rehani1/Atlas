use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SearchResultSource {
    Title,
    Message,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SearchSnippetPart {
    pub(crate) text: String,
    pub(crate) is_match: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ChatSearchResult {
    pub(crate) chat_id: String,
    pub(crate) chat_title: String,
    pub(crate) message_id: Option<i64>,
    pub(crate) role: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) message_count: i64,
    pub(crate) source: SearchResultSource,
    pub(crate) score: f64,
    pub(crate) snippet: Vec<SearchSnippetPart>,
}
