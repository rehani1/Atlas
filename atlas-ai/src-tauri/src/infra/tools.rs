use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::domain::tools::{
    ToolCall, ToolCallInsert, ToolCallStatus, ToolPermission, ToolPermissionScopeType,
};

pub(crate) fn create_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
      CREATE TABLE IF NOT EXISTS tool_calls (
        id TEXT PRIMARY KEY,
        conversation_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
        message_id INTEGER REFERENCES messages(id) ON DELETE SET NULL,
        tool_name TEXT NOT NULL,
        arguments_json TEXT NOT NULL,
        arguments_summary TEXT NOT NULL,
        status TEXT NOT NULL CHECK(status IN ('pending', 'denied', 'succeeded', 'failed')),
        result_summary TEXT,
        error_message TEXT,
        created_at INTEGER NOT NULL,
        completed_at INTEGER
      );

      CREATE TABLE IF NOT EXISTS tool_permissions (
        id TEXT PRIMARY KEY,
        scope_type TEXT NOT NULL CHECK(scope_type IN ('workspace')),
        scope_id TEXT NOT NULL,
        tool_name TEXT NOT NULL,
        permission TEXT NOT NULL CHECK(permission IN ('allow')),
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        UNIQUE(scope_type, scope_id, tool_name)
      );

      CREATE INDEX IF NOT EXISTS idx_tool_calls_conversation
        ON tool_calls(conversation_id, created_at DESC);

      CREATE INDEX IF NOT EXISTS idx_tool_calls_message
        ON tool_calls(message_id, created_at ASC);

      CREATE INDEX IF NOT EXISTS idx_tool_calls_status
        ON tool_calls(status, created_at DESC);

      CREATE INDEX IF NOT EXISTS idx_tool_permissions_scope
        ON tool_permissions(scope_type, scope_id, tool_name);
      ",
    )
}

fn create_id(conn: &Connection) -> Result<String, rusqlite::Error> {
    conn.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
}

fn to_from_sql_error(error: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
    )
}

fn read_tool_call(row: &Row<'_>) -> Result<ToolCall, rusqlite::Error> {
    let status = row
        .get::<_, String>(6)
        .and_then(|value| ToolCallStatus::from_str(&value).map_err(to_from_sql_error))?;

    Ok(ToolCall {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        message_id: row.get(2)?,
        tool_name: row.get(3)?,
        arguments_json: row.get(4)?,
        arguments_summary: row.get(5)?,
        status,
        result_summary: row.get(7)?,
        error_message: row.get(8)?,
        created_at: row.get(9)?,
        completed_at: row.get(10)?,
    })
}

fn read_tool_permission(row: &Row<'_>) -> Result<ToolPermission, rusqlite::Error> {
    let scope_type = row
        .get::<_, String>(1)
        .and_then(|value| ToolPermissionScopeType::from_str(&value).map_err(to_from_sql_error))?;

    Ok(ToolPermission {
        id: row.get(0)?,
        scope_type,
        scope_id: row.get(2)?,
        tool_name: row.get(3)?,
        permission: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

pub(crate) fn insert_tool_call(
    conn: &Connection,
    input: ToolCallInsert<'_>,
) -> Result<ToolCall, rusqlite::Error> {
    let id = create_id(conn)?;
    conn.execute(
        "
      INSERT INTO tool_calls (
        id,
        conversation_id,
        message_id,
        tool_name,
        arguments_json,
        arguments_summary,
        status,
        created_at
      )
      VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7)
      ",
        params![
            id,
            input.conversation_id,
            input.message_id,
            input.tool_name,
            input.arguments_json,
            input.arguments_summary,
            input.created_at,
        ],
    )?;

    get_tool_call(conn, &id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn get_tool_call(
    conn: &Connection,
    tool_call_id: &str,
) -> Result<Option<ToolCall>, rusqlite::Error> {
    conn.query_row(
        "
      SELECT
        id,
        conversation_id,
        message_id,
        tool_name,
        arguments_json,
        arguments_summary,
        status,
        result_summary,
        error_message,
        created_at,
        completed_at
      FROM tool_calls
      WHERE id = ?1
      ",
        params![tool_call_id],
        read_tool_call,
    )
    .optional()
}

pub(crate) fn list_for_message(
    conn: &Connection,
    message_id: i64,
) -> Result<Vec<ToolCall>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "
      SELECT
        id,
        conversation_id,
        message_id,
        tool_name,
        arguments_json,
        arguments_summary,
        status,
        result_summary,
        error_message,
        created_at,
        completed_at
      FROM tool_calls
      WHERE message_id = ?1
      ORDER BY created_at ASC, id ASC
      ",
    )?;

    let calls = statement
        .query_map(params![message_id], read_tool_call)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(calls)
}

pub(crate) fn mark_denied(
    conn: &Connection,
    tool_call_id: &str,
    completed_at: i64,
) -> Result<ToolCall, rusqlite::Error> {
    conn.execute(
        "
      UPDATE tool_calls
      SET status = 'denied',
          result_summary = NULL,
          error_message = NULL,
          completed_at = ?2
      WHERE id = ?1
      ",
        params![tool_call_id, completed_at],
    )?;

    get_tool_call(conn, tool_call_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn mark_succeeded(
    conn: &Connection,
    tool_call_id: &str,
    result_summary: &str,
    completed_at: i64,
) -> Result<ToolCall, rusqlite::Error> {
    conn.execute(
        "
      UPDATE tool_calls
      SET status = 'succeeded',
          result_summary = ?2,
          error_message = NULL,
          completed_at = ?3
      WHERE id = ?1
      ",
        params![tool_call_id, result_summary, completed_at],
    )?;

    get_tool_call(conn, tool_call_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn mark_failed(
    conn: &Connection,
    tool_call_id: &str,
    error_message: &str,
    completed_at: i64,
) -> Result<ToolCall, rusqlite::Error> {
    conn.execute(
        "
      UPDATE tool_calls
      SET status = 'failed',
          result_summary = NULL,
          error_message = ?2,
          completed_at = ?3
      WHERE id = ?1
      ",
        params![tool_call_id, error_message, completed_at],
    )?;

    get_tool_call(conn, tool_call_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn upsert_permission(
    conn: &Connection,
    scope_type: ToolPermissionScopeType,
    scope_id: &str,
    tool_name: &str,
    now: i64,
) -> Result<ToolPermission, rusqlite::Error> {
    let existing_id = conn
        .query_row(
            "
      SELECT id
      FROM tool_permissions
      WHERE scope_type = ?1
        AND scope_id = ?2
        AND tool_name = ?3
      ",
            params![scope_type.as_str(), scope_id, tool_name],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let id = existing_id.unwrap_or(create_id(conn)?);

    conn.execute(
        "
      INSERT INTO tool_permissions (
        id,
        scope_type,
        scope_id,
        tool_name,
        permission,
        created_at,
        updated_at
      )
      VALUES (?1, ?2, ?3, ?4, 'allow', ?5, ?5)
      ON CONFLICT(scope_type, scope_id, tool_name) DO UPDATE SET
        permission = excluded.permission,
        updated_at = excluded.updated_at
      ",
        params![id, scope_type.as_str(), scope_id, tool_name, now],
    )?;

    get_permission(conn, scope_type, scope_id, tool_name)?
        .ok_or(rusqlite::Error::QueryReturnedNoRows)
}

fn get_permission(
    conn: &Connection,
    scope_type: ToolPermissionScopeType,
    scope_id: &str,
    tool_name: &str,
) -> Result<Option<ToolPermission>, rusqlite::Error> {
    conn.query_row(
        "
      SELECT
        id,
        scope_type,
        scope_id,
        tool_name,
        permission,
        created_at,
        updated_at
      FROM tool_permissions
      WHERE scope_type = ?1
        AND scope_id = ?2
        AND tool_name = ?3
      ",
        params![scope_type.as_str(), scope_id, tool_name],
        read_tool_permission,
    )
    .optional()
}
