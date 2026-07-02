use rusqlite::Connection;

use crate::{
    domain::summary::{ConversationSummary, SummarySourceMessage},
    infra::summaries,
};

const MAX_MANUAL_SUMMARY_CHARS: usize = 20_000;

pub(crate) fn get_current(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Option<ConversationSummary>, String> {
    summaries::get_by_conversation(conn, conversation_id).map_err(|error| error.to_string())
}

pub(crate) fn get_enabled_for_prompt(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Option<ConversationSummary>, String> {
    summaries::get_enabled_for_prompt(conn, conversation_id).map_err(|error| error.to_string())
}

pub(crate) fn list_source_messages(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Vec<SummarySourceMessage>, String> {
    summaries::list_source_messages(conn, conversation_id).map_err(|error| error.to_string())
}

pub(crate) fn save_manual(
    conn: &Connection,
    conversation_id: &str,
    summary: &str,
    source_messages: &[SummarySourceMessage],
    enabled_for_prompt: bool,
    now: i64,
) -> Result<ConversationSummary, String> {
    let summary = normalize_summary(summary)?;
    let (source_message_start_id, source_message_end_id) = source_message_range(source_messages);

    summaries::upsert(
        conn,
        summaries::SummaryUpsert {
            conversation_id,
            summary: &summary,
            source_message_start_id,
            source_message_end_id,
            model_name: "manual",
            enabled_for_prompt: Some(enabled_for_prompt),
            now,
        },
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn save_generated(
    conn: &Connection,
    conversation_id: &str,
    summary: &str,
    source_messages: &[SummarySourceMessage],
    model_name: &str,
    enabled_for_prompt: Option<bool>,
    now: i64,
) -> Result<ConversationSummary, String> {
    let summary = normalize_summary(summary)?;
    let (source_message_start_id, source_message_end_id) = source_message_range(source_messages);

    summaries::upsert(
        conn,
        summaries::SummaryUpsert {
            conversation_id,
            summary: &summary,
            source_message_start_id,
            source_message_end_id,
            model_name,
            enabled_for_prompt,
            now,
        },
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn set_enabled(
    conn: &Connection,
    conversation_id: &str,
    enabled_for_prompt: bool,
    now: i64,
) -> Result<ConversationSummary, String> {
    summaries::set_enabled(conn, conversation_id, enabled_for_prompt, now)
        .map_err(|error| error.to_string())
}

pub(crate) fn delete(conn: &Connection, conversation_id: &str) -> Result<bool, String> {
    summaries::delete_for_conversation(conn, conversation_id).map_err(|error| error.to_string())
}

pub(crate) fn source_message_range(
    messages: &[SummarySourceMessage],
) -> (Option<i64>, Option<i64>) {
    (
        messages.first().map(|message| message.id),
        messages.last().map(|message| message.id),
    )
}

fn normalize_summary(summary: &str) -> Result<String, String> {
    let summary = summary.trim();

    if summary.is_empty() {
        return Err("Summary cannot be empty.".to_string());
    }

    if summary.chars().count() > MAX_MANUAL_SUMMARY_CHARS {
        return Err(format!(
            "Summary must be {MAX_MANUAL_SUMMARY_CHARS} characters or fewer."
        ));
    }

    Ok(summary.to_string())
}

#[cfg(test)]
mod tests {
    use super::{normalize_summary, source_message_range};
    use crate::domain::summary::SummarySourceMessage;

    #[test]
    fn source_message_range_uses_first_and_last_message_ids() {
        let messages = vec![
            SummarySourceMessage {
                id: 4,
                role: "user".to_string(),
                content: "First".to_string(),
            },
            SummarySourceMessage {
                id: 9,
                role: "assistant".to_string(),
                content: "Second".to_string(),
            },
        ];

        assert_eq!(source_message_range(&messages), (Some(4), Some(9)));
        assert_eq!(source_message_range(&[]), (None, None));
    }

    #[test]
    fn normalize_summary_rejects_blank_text() {
        assert_eq!(
            normalize_summary("   ").unwrap_err(),
            "Summary cannot be empty."
        );
    }
}
