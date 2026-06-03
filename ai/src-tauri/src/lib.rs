use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::{
  collections::HashMap,
  io::{BufRead, BufReader, Read, Write},
  net::{SocketAddr, TcpStream},
  path::PathBuf,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
  },
  time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, State};

struct ChatStore {
  conn: Mutex<Connection>,
}

#[derive(Default)]
struct GenerationTasks {
  tasks: Mutex<HashMap<String, Arc<AtomicBool>>>,
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

#[derive(Serialize)]
struct OllamaModel {
  name: String,
  size: i64,
  modified_at: String,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
  models: Vec<OllamaModelResponse>,
}

#[derive(Deserialize)]
struct OllamaModelResponse {
  name: String,
  size: i64,
  modified_at: String,
}

#[derive(Serialize)]
struct OllamaChatRequest {
  model: String,
  messages: Vec<OllamaChatMessage>,
  stream: bool,
}

#[derive(Serialize)]
struct OllamaChatMessage {
  role: String,
  content: String,
}

#[derive(Deserialize)]
struct OllamaChatResponseMessage {
  content: String,
}

#[derive(Deserialize)]
struct OllamaChatStreamResponse {
  message: Option<OllamaChatResponseMessage>,
  done: bool,
}

struct OllamaResponse {
  status_code: u16,
  body: String,
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

fn escape_like_pattern(value: &str) -> String {
  let mut escaped = String::with_capacity(value.len());

  for character in value.chars() {
    match character {
      '%' | '_' | '\\' => {
        escaped.push('\\');
        escaped.push(character);
      }
      _ => escaped.push(character),
    }
  }

  escaped
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

fn validate_ollama_model_name(model: &str) -> Result<String, String> {
  let model = model.trim();

  if model.is_empty() {
    return Err("Model name cannot be empty".to_string());
  }

  if model
    .chars()
    .any(|character| character.is_whitespace() || character == '"' || character == '\\')
  {
    return Err("Model name contains invalid characters".to_string());
  }

  Ok(model.to_string())
}

fn ollama_request(method: &str, path: &str, body: Option<String>) -> Result<OllamaResponse, String> {
  let addr = SocketAddr::from(([127, 0, 0, 1], 11434));
  let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).map_err(|_| {
    "Ollama is not running. Open Ollama and try again.".to_string()
  })?;
  stream
    .set_read_timeout(Some(Duration::from_secs(1200)))
    .map_err(|error| error.to_string())?;
  stream
    .set_write_timeout(Some(Duration::from_secs(10)))
    .map_err(|error| error.to_string())?;

  let body = body.unwrap_or_default();
  let request = format!(
    "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:11434\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
    body.as_bytes().len(),
    body
  );

  stream
    .write_all(request.as_bytes())
    .map_err(|error| error.to_string())?;

  let mut raw_response = String::new();
  stream
    .read_to_string(&mut raw_response)
    .map_err(|error| error.to_string())?;

  let (headers, body) = raw_response
    .split_once("\r\n\r\n")
    .ok_or_else(|| "Ollama returned an invalid HTTP response".to_string())?;
  let status_code = headers
    .lines()
    .next()
    .and_then(|status| status.split_whitespace().nth(1))
    .and_then(|status| status.parse::<u16>().ok())
    .ok_or_else(|| "Ollama returned an invalid HTTP status".to_string())?;

  Ok(OllamaResponse {
    status_code,
    body: body.to_string(),
  })
}

fn ollama_error(response: &OllamaResponse) -> String {
  if response.body.trim().is_empty() {
    return format!("Ollama request failed with status {}", response.status_code);
  }

  serde_json::from_str::<serde_json::Value>(&response.body)
    .ok()
    .and_then(|value| {
      value
        .get("error")
        .and_then(|error| error.as_str())
        .map(ToString::to_string)
    })
    .unwrap_or_else(|| response.body.clone())
}

fn read_ollama_models() -> Result<Vec<OllamaModel>, String> {
  let response = ollama_request("GET", "/api/tags", None)?;

  if !(200..300).contains(&response.status_code) {
    return Err(ollama_error(&response));
  }

  let tags = serde_json::from_str::<OllamaTagsResponse>(&response.body)
    .map_err(|error| error.to_string())?;

  Ok(
    tags
      .models
      .into_iter()
      .map(|model| OllamaModel {
        name: model.name,
        size: model.size,
        modified_at: model.modified_at,
      })
      .collect(),
  )
}

fn stream_ollama_chat(
  model: String,
  messages: Vec<OllamaChatMessage>,
  cancellation: Arc<AtomicBool>,
) -> Result<String, String> {
  let client = reqwest::blocking::Client::builder()
    .connect_timeout(Duration::from_secs(2))
    .timeout(None)
    .build()
    .map_err(|error| error.to_string())?;
  let request = OllamaChatRequest {
    model,
    messages,
    stream: true,
  };
  let response = client
    .post("http://127.0.0.1:11434/api/chat")
    .json(&request)
    .send()
    .map_err(|_| "Ollama is not running. Open Ollama and try again.".to_string())?;
  let status = response.status();

  if !status.is_success() {
    let body = response.text().unwrap_or_default();
    return Err(
      serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|value| {
          value
            .get("error")
            .and_then(|error| error.as_str())
            .map(ToString::to_string)
        })
        .unwrap_or_else(|| format!("Ollama request failed with status {status}")),
    );
  }

  let mut reader = BufReader::new(response);
  let mut line = String::new();
  let mut content = String::new();

  loop {
    if cancellation.load(Ordering::SeqCst) {
      return Err("Generation cancelled".to_string());
    }

    line.clear();
    let bytes_read = reader.read_line(&mut line).map_err(|error| error.to_string())?;
    if bytes_read == 0 {
      break;
    }

    let line = line.trim();
    if line.is_empty() {
      continue;
    }

    let chunk = serde_json::from_str::<OllamaChatStreamResponse>(line)
      .map_err(|error| error.to_string())?;

    if let Some(message) = chunk.message {
      content.push_str(&message.content);
    }

    if chunk.done {
      break;
    }
  }

  let content = content.trim().to_string();
  if content.is_empty() {
    return Err("Ollama returned an empty response".to_string());
  }

  Ok(content)
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

fn list_messages_for_chat(conn: &Connection, chat_id: &str) -> Result<Vec<ChatMessage>, rusqlite::Error> {
  let mut statement = conn.prepare(
    "
    SELECT id, chat_id, role, content, created_at
    FROM messages
    WHERE chat_id = ?1
    ORDER BY created_at ASC, id ASC
    ",
  )?;

  let messages = statement
    .query_map(params![chat_id], read_chat_message)?
    .collect::<Result<Vec<_>, _>>()?;

  Ok(messages)
}

fn insert_message(
  conn: &Connection,
  chat_id: &str,
  role: &str,
  content: &str,
) -> Result<ChatMessage, String> {
  let role = role.trim();
  if !matches!(role, "user" | "assistant" | "system") {
    return Err("Invalid message role".to_string());
  }

  let content = content.trim();
  if content.is_empty() {
    return Err("Message content cannot be empty".to_string());
  }

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
fn search_chats(store: State<'_, ChatStore>, query: String) -> Result<Vec<ChatSummary>, String> {
  let query = query.trim();

  if query.is_empty() {
    return list_chats(store);
  }

  let conn = store
    .conn
    .lock()
    .map_err(|_| "Database lock was poisoned".to_string())?;
  let pattern = format!("%{}%", escape_like_pattern(&query.to_lowercase()));
  let mut statement = conn
    .prepare(
      "
      SELECT c.id, c.title, c.created_at, c.updated_at, COUNT(m.id) AS message_count
      FROM chats c
      LEFT JOIN messages m ON m.chat_id = c.id
      WHERE LOWER(c.title) LIKE ?1 ESCAPE '\\'
        OR EXISTS (
          SELECT 1
          FROM messages search_m
          WHERE search_m.chat_id = c.id
            AND LOWER(search_m.content) LIKE ?1 ESCAPE '\\'
        )
      GROUP BY c.id
      ORDER BY c.updated_at DESC
      ",
    )
    .map_err(|error| error.to_string())?;

  let chats = statement
    .query_map(params![pattern], read_chat_summary)
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
  list_messages_for_chat(&conn, &chat_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn add_message(
  store: State<'_, ChatStore>,
  chat_id: String,
  role: String,
  content: String,
) -> Result<ChatMessage, String> {
  let conn = store
    .conn
    .lock()
    .map_err(|_| "Database lock was poisoned".to_string())?;
  insert_message(&conn, &chat_id, &role, &content)
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

#[tauri::command]
async fn list_ollama_models() -> Result<Vec<OllamaModel>, String> {
  tauri::async_runtime::spawn_blocking(read_ollama_models)
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn download_ollama_model(model: String) -> Result<Vec<OllamaModel>, String> {
  tauri::async_runtime::spawn_blocking(move || {
    let model = validate_ollama_model_name(&model)?;
    let body = serde_json::json!({
      "name": model,
      "stream": false
    })
    .to_string();
    let response = ollama_request("POST", "/api/pull", Some(body))?;

    if !(200..300).contains(&response.status_code) {
      return Err(ollama_error(&response));
    }

    read_ollama_models()
  })
  .await
  .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn delete_ollama_model(model: String) -> Result<Vec<OllamaModel>, String> {
  tauri::async_runtime::spawn_blocking(move || {
    let model = validate_ollama_model_name(&model)?;
    let body = serde_json::json!({ "name": model }).to_string();
    let response = ollama_request("DELETE", "/api/delete", Some(body))?;

    if !(200..300).contains(&response.status_code) {
      return Err(ollama_error(&response));
    }

    read_ollama_models()
  })
  .await
  .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn generate_assistant_response(
  store: State<'_, ChatStore>,
  tasks: State<'_, GenerationTasks>,
  chat_id: String,
  model: String,
) -> Result<ChatMessage, String> {
  let model = validate_ollama_model_name(&model)?;
  let messages = {
    let conn = store
      .conn
      .lock()
      .map_err(|_| "Database lock was poisoned".to_string())?;
    let messages =
      list_messages_for_chat(&conn, &chat_id).map_err(|error| error.to_string())?;

    if messages.is_empty() {
      return Err("Chat has no messages to send to Ollama".to_string());
    }

    messages
  };
  let ollama_messages = messages
    .into_iter()
    .map(|message| OllamaChatMessage {
      role: message.role,
      content: message.content,
    })
    .collect::<Vec<_>>();
  let cancellation = Arc::new(AtomicBool::new(false));

  {
    let mut tasks = tasks
      .tasks
      .lock()
      .map_err(|_| "Generation task lock was poisoned".to_string())?;
    if let Some(existing_task) = tasks.insert(chat_id.clone(), cancellation.clone()) {
      existing_task.store(true, Ordering::SeqCst);
    }
  }

  let assistant_content = tauri::async_runtime::spawn_blocking(move || {
    stream_ollama_chat(model, ollama_messages, cancellation.clone())
  })
  .await
  .map_err(|error| error.to_string())?;

  {
    let mut tasks = tasks
      .tasks
      .lock()
      .map_err(|_| "Generation task lock was poisoned".to_string())?;
    tasks.remove(&chat_id);
  }

  let assistant_content = assistant_content?;
  let conn = store
    .conn
    .lock()
    .map_err(|_| "Database lock was poisoned".to_string())?;
  insert_message(&conn, &chat_id, "assistant", &assistant_content)
}

#[tauri::command]
fn cancel_ollama_generation(
  tasks: State<'_, GenerationTasks>,
  chat_id: String,
) -> Result<bool, String> {
  let cancellation = tasks
    .tasks
    .lock()
    .map_err(|_| "Generation task lock was poisoned".to_string())?
    .remove(&chat_id);

  if let Some(cancellation) = cancellation {
    cancellation.store(true, Ordering::SeqCst);
    return Ok(true);
  }

  Ok(false)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      let app_data_dir = app.path().app_data_dir()?;
      std::fs::create_dir_all(&app_data_dir)?;
      app.manage(ChatStore::new(app_data_dir.join("atlas.sqlite3"))?);
      app.manage(GenerationTasks::default());

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
      search_chats,
      create_chat,
      get_messages,
      add_message,
      delete_chat,
      list_ollama_models,
      download_ollama_model,
      delete_ollama_model,
      generate_assistant_response,
      cancel_ollama_generation
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
