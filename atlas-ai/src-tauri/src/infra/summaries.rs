use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::domain::summary::{ConversationSummary, SummarySourceMessage};

pub(crate) struct SummaryUpsert<'a> {
    pub(crate) conversation_id: &'a str,
    pub(crate) summary: &'a str,
    pub(crate) source_message_start_id: Option<i64>,
    pub(crate) source_message_end_id: Option<i64>,
    pub(crate) model_name: &'a str,
    pub(crate) enabled_for_prompt: Option<bool>,
    pub(crate) now: i64,
}

pub(crate) fn create_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
      CREATE TABLE IF NOT EXISTS conversation_summaries (
        id TEXT PRIMARY KEY,
        conversation_id TEXT NOT NULL UNIQUE REFERENCES chats(id) ON DELETE CASCADE,
        summary TEXT NOT NULL,
        source_message_start_id INTEGER REFERENCES messages(id) ON DELETE SET NULL,
        source_message_end_id INTEGER REFERENCES messages(id) ON DELETE SET NULL,
        model_name TEXT NOT NULL,
        version INTEGER NOT NULL,
        enabled_for_prompt INTEGER NOT NULL DEFAULT 0 CHECK(enabled_for_prompt IN (0, 1)),
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_conversation_summaries_updated
        ON conversation_summaries(updated_at DESC);

      CREATE INDEX IF NOT EXISTS idx_conversation_summaries_enabled
        ON conversation_summaries(conversation_id, enabled_for_prompt);
      ",
    )
}

fn create_id(conn: &Connection) -> Result<String, rusqlite::Error> {
    conn.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
}

fn read_summary(row: &Row<'_>) -> Result<ConversationSummary, rusqlite::Error> {
    Ok(ConversationSummary {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        summary: row.get(2)?,
        source_message_start_id: row.get(3)?,
        source_message_end_id: row.get(4)?,
        model_name: row.get(5)?,
        version: row.get(6)?,
        enabled_for_prompt: row.get::<_, i64>(7)? != 0,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn summary_select() -> &'static str {
    "
      SELECT
        id,
        conversation_id,
        summary,
        source_message_start_id,
        source_message_end_id,
        model_name,
        version,
        enabled_for_prompt,
        created_at,
        updated_at
      FROM conversation_summaries
    "
}

pub(crate) fn get_by_conversation(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Option<ConversationSummary>, rusqlite::Error> {
    conn.query_row(
        &format!("{} WHERE conversation_id = ?1", summary_select()),
        params![conversation_id],
        read_summary,
    )
    .optional()
}

pub(crate) fn get_enabled_for_prompt(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Option<ConversationSummary>, rusqlite::Error> {
    conn.query_row(
        &format!(
            "{} WHERE conversation_id = ?1 AND enabled_for_prompt = 1",
            summary_select()
        ),
        params![conversation_id],
        read_summary,
    )
    .optional()
}

pub(crate) fn upsert(
    conn: &Connection,
    input: SummaryUpsert<'_>,
) -> Result<ConversationSummary, rusqlite::Error> {
    let id = create_id(conn)?;
    let enabled_for_prompt = input
        .enabled_for_prompt
        .map(|enabled| if enabled { 1 } else { 0 });

    conn.execute(
        "
      INSERT INTO conversation_summaries (
        id,
        conversation_id,
        summary,
        source_message_start_id,
        source_message_end_id,
        model_name,
        version,
        enabled_for_prompt,
        created_at,
        updated_at
      )
      VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, COALESCE(?7, 0), ?8, ?8)
      ON CONFLICT(conversation_id) DO UPDATE SET
        summary = excluded.summary,
        source_message_start_id = excluded.source_message_start_id,
        source_message_end_id = excluded.source_message_end_id,
        model_name = excluded.model_name,
        version = conversation_summaries.version + 1,
        enabled_for_prompt = COALESCE(?7, conversation_summaries.enabled_for_prompt),
        updated_at = excluded.updated_at
      ",
        params![
            id,
            input.conversation_id,
            input.summary,
            input.source_message_start_id,
            input.source_message_end_id,
            input.model_name,
            enabled_for_prompt,
            input.now,
        ],
    )?;

    get_by_conversation(conn, input.conversation_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn set_enabled(
    conn: &Connection,
    conversation_id: &str,
    enabled_for_prompt: bool,
    updated_at: i64,
) -> Result<ConversationSummary, rusqlite::Error> {
    conn.execute(
        "
      UPDATE conversation_summaries
      SET enabled_for_prompt = ?2,
          updated_at = ?3
      WHERE conversation_id = ?1
      ",
        params![conversation_id, i64::from(enabled_for_prompt), updated_at],
    )?;

    get_by_conversation(conn, conversation_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn delete_for_conversation(
    conn: &Connection,
    conversation_id: &str,
) -> Result<bool, rusqlite::Error> {
    let deleted = conn.execute(
        "DELETE FROM conversation_summaries WHERE conversation_id = ?1",
        params![conversation_id],
    )?;

    Ok(deleted > 0)
}

pub(crate) fn list_source_messages(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Vec<SummarySourceMessage>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "
      SELECT id, role, content
      FROM messages
      WHERE chat_id = ?1
      ORDER BY created_at ASC, id ASC
      ",
    )?;

    let messages = statement
        .query_map(params![conversation_id], |row| {
            Ok(SummarySourceMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(messages)
}
