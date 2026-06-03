use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use std::{
  path::PathBuf,
  sync::Mutex,
  time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, State};

struct ChatStore {
  conn: Mutex<Connection>,
}

#[derive(Serialize)]
struct ChatSummary {
  id: String,
  title: String,
  created_at: i64,
  updated_at: i64,
  message_count: i64,
}

#[derive(Serialize)]
struct ChatMessage {
  id: i64,
  chat_id: String,
  role: String,
  content: String,
  created_at: i64,
}

impl ChatStore {
  fn new(db_path: PathBuf) -> Result<Self, rusqlite::Error> {
    let conn = Connection::open(db_path)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
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

      CREATE INDEX IF NOT EXISTS idx_chats_updated_at
        ON chats(updated_at DESC);

      CREATE INDEX IF NOT EXISTS idx_messages_chat_id_created_at
        ON messages(chat_id, created_at, id);

      CREATE TRIGGER IF NOT EXISTS messages_after_insert_update_chat
      AFTER INSERT ON messages
      BEGIN
        UPDATE chats
        SET updated_at = NEW.created_at
        WHERE id = NEW.chat_id;
      END;
      ",
    )?;

    Ok(Self {
      conn: Mutex::new(conn),
    })
  }
}

fn now_millis() -> Result<i64, String> {
  let duration = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_err(|error| error.to_string())?;

  Ok(duration.as_millis() as i64)
}

fn normalize_title(title: Option<String>) -> String {
  let trimmed = title.as_deref().unwrap_or("New chat").trim();

  if trimmed.is_empty() {
    return "New chat".to_string();
  }

  let mut title = trimmed.chars().take(64).collect::<String>();
  if trimmed.chars().count() > 64 {
    title.push_str("...");
  }

  title
}

fn create_id(conn: &Connection) -> Result<String, rusqlite::Error> {
  conn.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
}

fn read_chat_summary(row: &Row<'_>) -> Result<ChatSummary, rusqlite::Error> {
  Ok(ChatSummary {
    id: row.get(0)?,
    title: row.get(1)?,
    created_at: row.get(2)?,
    updated_at: row.get(3)?,
    message_count: row.get(4)?,
  })
}

fn read_chat_message(row: &Row<'_>) -> Result<ChatMessage, rusqlite::Error> {
  Ok(ChatMessage {
    id: row.get(0)?,
    chat_id: row.get(1)?,
    role: row.get(2)?,
    content: row.get(3)?,
    created_at: row.get(4)?,
  })
}

fn get_chat_summary(conn: &Connection, chat_id: &str) -> Result<Option<ChatSummary>, rusqlite::Error> {
  conn
    .query_row(
      "
      SELECT c.id, c.title, c.created_at, c.updated_at, COUNT(m.id) AS message_count
      FROM chats c
      LEFT JOIN messages m ON m.chat_id = c.id
      WHERE c.id = ?1
      GROUP BY c.id
      ",
      params![chat_id],
      read_chat_summary,
    )
    .optional()
}

#[tauri::command]
fn list_chats(store: State<'_, ChatStore>) -> Result<Vec<ChatSummary>, String> {
  let conn = store
    .conn
    .lock()
    .map_err(|_| "Database lock was poisoned".to_string())?;
  let mut statement = conn
    .prepare(
      "
      SELECT c.id, c.title, c.created_at, c.updated_at, COUNT(m.id) AS message_count
      FROM chats c
      LEFT JOIN messages m ON m.chat_id = c.id
      GROUP BY c.id
      ORDER BY c.updated_at DESC
      ",
    )
    .map_err(|error| error.to_string())?;

  let chats = statement
    .query_map([], read_chat_summary)
    .map_err(|error| error.to_string())?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|error| error.to_string())?;

  Ok(chats)
}

#[tauri::command]
fn create_chat(store: State<'_, ChatStore>, title: Option<String>) -> Result<ChatSummary, String> {
  let conn = store
    .conn
    .lock()
    .map_err(|_| "Database lock was poisoned".to_string())?;
  let id = create_id(&conn).map_err(|error| error.to_string())?;
  let now = now_millis()?;
  let title = normalize_title(title);

  conn
    .execute(
      "
      INSERT INTO chats (id, title, created_at, updated_at)
      VALUES (?1, ?2, ?3, ?3)
      ",
      params![id, title, now],
    )
    .map_err(|error| error.to_string())?;

  get_chat_summary(&conn, &id)
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "Created chat was not found".to_string())
}

#[tauri::command]
fn get_messages(store: State<'_, ChatStore>, chat_id: String) -> Result<Vec<ChatMessage>, String> {
  let conn = store
    .conn
    .lock()
    .map_err(|_| "Database lock was poisoned".to_string())?;
  let mut statement = conn
    .prepare(
      "
      SELECT id, chat_id, role, content, created_at
      FROM messages
      WHERE chat_id = ?1
      ORDER BY created_at ASC, id ASC
      ",
    )
    .map_err(|error| error.to_string())?;

  let messages = statement
    .query_map(params![chat_id], read_chat_message)
    .map_err(|error| error.to_string())?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|error| error.to_string())?;

  Ok(messages)
}

#[tauri::command]
fn add_message(
  store: State<'_, ChatStore>,
  chat_id: String,
  role: String,
  content: String,
) -> Result<ChatMessage, String> {
  let role = role.trim();
  if !matches!(role, "user" | "assistant" | "system") {
    return Err("Invalid message role".to_string());
  }

  let content = content.trim();
  if content.is_empty() {
    return Err("Message content cannot be empty".to_string());
  }

  let conn = store
    .conn
    .lock()
    .map_err(|_| "Database lock was poisoned".to_string())?;
  let now = now_millis()?;

  conn
    .execute(
      "
      INSERT INTO messages (chat_id, role, content, created_at)
      VALUES (?1, ?2, ?3, ?4)
      ",
      params![chat_id, role, content, now],
    )
    .map_err(|error| error.to_string())?;

  let message_id = conn.last_insert_rowid();

  conn
    .query_row(
      "
      SELECT id, chat_id, role, content, created_at
      FROM messages
      WHERE id = ?1
      ",
      params![message_id],
      read_chat_message,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_chat(store: State<'_, ChatStore>, chat_id: String) -> Result<bool, String> {
  let conn = store
    .conn
    .lock()
    .map_err(|_| "Database lock was poisoned".to_string())?;
  let deleted = conn
    .execute("DELETE FROM chats WHERE id = ?1", params![chat_id])
    .map_err(|error| error.to_string())?;

  Ok(deleted > 0)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      let app_data_dir = app.path().app_data_dir()?;
      std::fs::create_dir_all(&app_data_dir)?;
      app.manage(ChatStore::new(app_data_dir.join("atlas.sqlite3"))?);

      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      list_chats,
      create_chat,
      get_messages,
      add_message,
      delete_chat
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
