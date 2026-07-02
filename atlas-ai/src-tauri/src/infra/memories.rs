use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::domain::memory::{Memory, MemoryPromptSetting, MemoryScopeType, PromptMemoryUse};

pub(crate) struct MemoryInsert<'a> {
    pub(crate) scope_type: MemoryScopeType,
    pub(crate) scope_id: Option<&'a str>,
    pub(crate) content: &'a str,
    pub(crate) source_conversation_id: Option<&'a str>,
    pub(crate) source_message_id: Option<i64>,
    pub(crate) confidence: Option<f64>,
    pub(crate) pinned: bool,
    pub(crate) now: i64,
}

pub(crate) struct MemoryUpdate<'a> {
    pub(crate) content: &'a str,
    pub(crate) pinned: bool,
    pub(crate) updated_at: i64,
}

pub(crate) fn create_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
      CREATE TABLE IF NOT EXISTS memories (
        id TEXT PRIMARY KEY,
        scope_type TEXT NOT NULL CHECK(scope_type IN ('global', 'conversation', 'project')),
        scope_id TEXT,
        content TEXT NOT NULL,
        source_conversation_id TEXT REFERENCES chats(id) ON DELETE SET NULL,
        source_message_id INTEGER REFERENCES messages(id) ON DELETE SET NULL,
        confidence REAL,
        pinned INTEGER NOT NULL DEFAULT 0 CHECK(pinned IN (0, 1)),
        archived_at INTEGER,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS memory_prompt_settings (
        conversation_id TEXT PRIMARY KEY REFERENCES chats(id) ON DELETE CASCADE,
        enabled_for_prompt INTEGER NOT NULL DEFAULT 0 CHECK(enabled_for_prompt IN (0, 1)),
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS generation_memory_uses (
        id TEXT PRIMARY KEY,
        generation_run_id TEXT NOT NULL REFERENCES generation_runs(id) ON DELETE CASCADE,
        memory_id TEXT REFERENCES memories(id) ON DELETE SET NULL,
        content_snapshot TEXT NOT NULL,
        scope_type TEXT NOT NULL CHECK(scope_type IN ('global', 'conversation', 'project')),
        scope_id TEXT,
        source_conversation_id TEXT,
        source_message_id INTEGER,
        used_at INTEGER NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_memories_scope
        ON memories(scope_type, scope_id, archived_at, pinned DESC, updated_at DESC);

      CREATE INDEX IF NOT EXISTS idx_memories_updated
        ON memories(archived_at, pinned DESC, updated_at DESC);

      CREATE INDEX IF NOT EXISTS idx_memories_source_message
        ON memories(source_message_id);

      CREATE INDEX IF NOT EXISTS idx_generation_memory_uses_run
        ON generation_memory_uses(generation_run_id, used_at ASC);
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

fn read_scope_type(row: &Row<'_>, index: usize) -> Result<MemoryScopeType, rusqlite::Error> {
    row.get::<_, String>(index)
        .and_then(|value| MemoryScopeType::from_str(&value).map_err(to_from_sql_error))
}

fn read_memory(row: &Row<'_>) -> Result<Memory, rusqlite::Error> {
    Ok(Memory {
        id: row.get(0)?,
        scope_type: read_scope_type(row, 1)?,
        scope_id: row.get(2)?,
        content: row.get(3)?,
        source_conversation_id: row.get(4)?,
        source_message_id: row.get(5)?,
        confidence: row.get(6)?,
        pinned: row.get::<_, i64>(7)? != 0,
        archived_at: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn memory_select() -> &'static str {
    "
      SELECT
        id,
        scope_type,
        scope_id,
        content,
        source_conversation_id,
        source_message_id,
        confidence,
        pinned,
        archived_at,
        created_at,
        updated_at
      FROM memories
    "
}

pub(crate) fn list(
    conn: &Connection,
    include_archived: bool,
) -> Result<Vec<Memory>, rusqlite::Error> {
    let sql = if include_archived {
        format!(
            "{} ORDER BY archived_at IS NOT NULL ASC, pinned DESC, updated_at DESC",
            memory_select()
        )
    } else {
        format!(
            "{} WHERE archived_at IS NULL ORDER BY pinned DESC, updated_at DESC",
            memory_select()
        )
    };
    let mut statement = conn.prepare(&sql)?;

    let memories = statement
        .query_map([], read_memory)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(memories)
}

pub(crate) fn get(conn: &Connection, memory_id: &str) -> Result<Option<Memory>, rusqlite::Error> {
    conn.query_row(
        &format!("{} WHERE id = ?1", memory_select()),
        params![memory_id],
        read_memory,
    )
    .optional()
}

pub(crate) fn insert(
    conn: &Connection,
    input: MemoryInsert<'_>,
) -> Result<Memory, rusqlite::Error> {
    let id = create_id(conn)?;
    conn.execute(
        "
      INSERT INTO memories (
        id,
        scope_type,
        scope_id,
        content,
        source_conversation_id,
        source_message_id,
        confidence,
        pinned,
        created_at,
        updated_at
      )
      VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
      ",
        params![
            id,
            input.scope_type.as_str(),
            input.scope_id,
            input.content,
            input.source_conversation_id,
            input.source_message_id,
            input.confidence,
            if input.pinned { 1 } else { 0 },
            input.now,
        ],
    )?;

    get(conn, &id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn update(
    conn: &Connection,
    memory_id: &str,
    input: MemoryUpdate<'_>,
) -> Result<Memory, rusqlite::Error> {
    conn.execute(
        "
      UPDATE memories
      SET content = ?2,
          pinned = ?3,
          updated_at = ?4
      WHERE id = ?1
      ",
        params![
            memory_id,
            input.content,
            if input.pinned { 1 } else { 0 },
            input.updated_at,
        ],
    )?;

    get(conn, memory_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn set_archived(
    conn: &Connection,
    memory_id: &str,
    archived_at: Option<i64>,
    updated_at: i64,
) -> Result<Memory, rusqlite::Error> {
    conn.execute(
        "
      UPDATE memories
      SET archived_at = ?2,
          updated_at = ?3
      WHERE id = ?1
      ",
        params![memory_id, archived_at, updated_at],
    )?;

    get(conn, memory_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn delete(conn: &Connection, memory_id: &str) -> Result<bool, rusqlite::Error> {
    let deleted = conn.execute("DELETE FROM memories WHERE id = ?1", params![memory_id])?;

    Ok(deleted > 0)
}

pub(crate) fn get_prompt_setting(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Option<MemoryPromptSetting>, rusqlite::Error> {
    conn.query_row(
        "
      SELECT conversation_id, enabled_for_prompt, created_at, updated_at
      FROM memory_prompt_settings
      WHERE conversation_id = ?1
      ",
        params![conversation_id],
        |row| {
            Ok(MemoryPromptSetting {
                conversation_id: row.get(0)?,
                enabled_for_prompt: row.get::<_, i64>(1)? != 0,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        },
    )
    .optional()
}

pub(crate) fn set_prompt_enabled(
    conn: &Connection,
    conversation_id: &str,
    enabled_for_prompt: bool,
    now: i64,
) -> Result<MemoryPromptSetting, rusqlite::Error> {
    conn.execute(
        "
      INSERT INTO memory_prompt_settings (
        conversation_id,
        enabled_for_prompt,
        created_at,
        updated_at
      )
      VALUES (?1, ?2, ?3, ?3)
      ON CONFLICT(conversation_id) DO UPDATE SET
        enabled_for_prompt = excluded.enabled_for_prompt,
        updated_at = excluded.updated_at
      ",
        params![conversation_id, if enabled_for_prompt { 1 } else { 0 }, now],
    )?;

    get_prompt_setting(conn, conversation_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn list_prompt_memories(
    conn: &Connection,
    conversation_id: &str,
    limit: i64,
) -> Result<Vec<Memory>, rusqlite::Error> {
    let mut statement = conn.prepare(&format!(
        "
      {}
      WHERE archived_at IS NULL
        AND (
          scope_type = 'global'
          OR (scope_type = 'conversation' AND scope_id = ?1)
        )
      ORDER BY pinned DESC, updated_at DESC
      LIMIT ?2
      ",
        memory_select()
    ))?;

    let memories = statement
        .query_map(params![conversation_id, limit.clamp(1, 24)], read_memory)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(memories)
}

pub(crate) fn insert_generation_uses(
    conn: &Connection,
    generation_run_id: &str,
    memories: &[Memory],
    used_at: i64,
) -> Result<Vec<PromptMemoryUse>, rusqlite::Error> {
    let mut use_ids = Vec::with_capacity(memories.len());

    for memory in memories {
        let id = create_id(conn)?;
        use_ids.push(id.clone());
        conn.execute(
            "
        INSERT INTO generation_memory_uses (
          id,
          generation_run_id,
          memory_id,
          content_snapshot,
          scope_type,
          scope_id,
          source_conversation_id,
          source_message_id,
          used_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ",
            params![
                id,
                generation_run_id,
                memory.id,
                memory.content,
                memory.scope_type.as_str(),
                memory.scope_id,
                memory.source_conversation_id,
                memory.source_message_id,
                used_at,
            ],
        )?;
    }

    use_ids
        .into_iter()
        .map(|use_id| get_generation_use(conn, &use_id))
        .collect()
}

fn read_generation_use(row: &Row<'_>) -> Result<PromptMemoryUse, rusqlite::Error> {
    Ok(PromptMemoryUse {
        id: row.get(0)?,
        generation_run_id: row.get(1)?,
        memory_id: row.get(2)?,
        content: row.get(3)?,
        scope_type: read_scope_type(row, 4)?,
        scope_id: row.get(5)?,
        source_conversation_id: row.get(6)?,
        source_message_id: row.get(7)?,
        used_at: row.get(8)?,
    })
}

fn generation_use_select() -> &'static str {
    "
      SELECT
        id,
        generation_run_id,
        memory_id,
        content_snapshot,
        scope_type,
        scope_id,
        source_conversation_id,
        source_message_id,
        used_at
      FROM generation_memory_uses
    "
}

pub(crate) fn get_generation_use(
    conn: &Connection,
    use_id: &str,
) -> Result<PromptMemoryUse, rusqlite::Error> {
    conn.query_row(
        &format!("{} WHERE id = ?1", generation_use_select()),
        params![use_id],
        read_generation_use,
    )
}

pub(crate) fn list_generation_uses(
    conn: &Connection,
    generation_run_id: &str,
) -> Result<Vec<PromptMemoryUse>, rusqlite::Error> {
    let mut statement = conn.prepare(&format!(
        "{} WHERE generation_run_id = ?1 ORDER BY used_at ASC, id ASC",
        generation_use_select()
    ))?;

    let uses = statement
        .query_map(params![generation_run_id], read_generation_use)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(uses)
}
