use rusqlite::{params, Connection};
use std::{fs, path::Path, time::Duration};

use crate::{
    domain::database::{DatabaseDiagnostics, DatabaseTableCount},
    infra::{benchmarks, jobs, knowledge, memories, search, summaries},
};

const SCHEMA_VERSION: i64 = 6;

pub(crate) fn setup_database(conn: &Connection) -> Result<(), rusqlite::Error> {
    configure_connection(conn)?;
    create_current_schema(conn)?;
    jobs::create_schema(conn)?;
    search::create_schema(conn)?;
    benchmarks::create_schema(conn)?;
    summaries::create_schema(conn)?;
    memories::create_schema(conn)?;
    knowledge::create_schema(conn)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;

    Ok(())
}

fn configure_connection(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;

    Ok(())
}

fn create_current_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
      CREATE TABLE IF NOT EXISTS chats (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
        role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
        content TEXT NOT NULL,
        created_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS generation_runs (
        id TEXT PRIMARY KEY,
        conversation_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
        message_id INTEGER REFERENCES messages(id) ON DELETE SET NULL,
        model_name TEXT NOT NULL,
        started_at INTEGER NOT NULL,
        first_token_at INTEGER,
        completed_at INTEGER,
        status TEXT NOT NULL CHECK(status IN ('running', 'completed', 'cancelled', 'failed')),
        total_duration_ms INTEGER,
        load_duration_ms INTEGER,
        prompt_eval_count INTEGER,
        prompt_eval_duration_ms INTEGER,
        eval_count INTEGER,
        eval_duration_ms INTEGER,
        tokens_per_second REAL,
        error_message TEXT
      );

      CREATE INDEX IF NOT EXISTS idx_chats_updated_at
        ON chats(updated_at DESC);

      CREATE INDEX IF NOT EXISTS idx_messages_chat_id_created_at
        ON messages(chat_id, created_at, id);

      CREATE INDEX IF NOT EXISTS idx_generation_runs_conversation_started
        ON generation_runs(conversation_id, started_at DESC);

      CREATE UNIQUE INDEX IF NOT EXISTS idx_generation_runs_message_id
        ON generation_runs(message_id)
        WHERE message_id IS NOT NULL;

      CREATE TRIGGER IF NOT EXISTS messages_after_insert_update_chat
      AFTER INSERT ON messages
      BEGIN
        UPDATE chats
        SET updated_at = NEW.created_at
        WHERE id = NEW.chat_id;
      END;
      ",
    )
}

pub(crate) fn diagnostics(
    conn: &Connection,
    db_path: &Path,
) -> Result<DatabaseDiagnostics, rusqlite::Error> {
    let table_counts = [
        "chats",
        "messages",
        "generation_runs",
        "jobs",
        "chat_search",
        "message_search",
        "model_benchmarks",
        "conversation_summaries",
        "memories",
        "memory_prompt_settings",
        "generation_memory_uses",
        "workspaces",
        "documents",
        "document_chunks",
        "document_chunk_search",
        "knowledge_prompt_settings",
        "retrieval_runs",
        "generation_document_sources",
    ]
    .into_iter()
    .map(|table_name| table_count(conn, table_name))
    .collect::<Result<Vec<_>, _>>()?;
    let journal_mode = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    let user_version = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let page_count = conn.pragma_query_value(None, "page_count", |row| row.get(0))?;
    let page_size = conn.pragma_query_value(None, "page_size", |row| row.get(0))?;
    let freelist_count = conn.pragma_query_value(None, "freelist_count", |row| row.get(0))?;
    let integrity_check =
        conn.pragma_query_value(None, "integrity_check", |row| row.get::<_, String>(0))?;

    Ok(DatabaseDiagnostics {
        path: db_path.display().to_string(),
        database_size_bytes: file_size(db_path),
        wal_size_bytes: file_size(&wal_path(db_path)),
        shm_size_bytes: file_size(&shm_path(db_path)),
        journal_mode,
        user_version,
        page_count,
        page_size,
        freelist_count,
        integrity_check,
        table_counts,
    })
}

fn table_count(conn: &Connection, table_name: &str) -> Result<DatabaseTableCount, rusqlite::Error> {
    let row_count = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = ?1",
        params![table_name],
        |row| row.get::<_, i64>(0),
    )?;

    if row_count == 0 {
        return Ok(DatabaseTableCount {
            table_name: table_name.to_string(),
            row_count: 0,
        });
    }

    let row_count = conn.query_row(&format!("SELECT COUNT(*) FROM {table_name}"), [], |row| {
        row.get(0)
    })?;

    Ok(DatabaseTableCount {
        table_name: table_name.to_string(),
        row_count,
    })
}

fn file_size(path: &Path) -> i64 {
    fs::metadata(path)
        .map(|metadata| metadata.len() as i64)
        .unwrap_or_default()
}

fn wal_path(path: &Path) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{}-wal", path.display()))
}

fn shm_path(path: &Path) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{}-shm", path.display()))
}
