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
    generation_run: Option<GenerationRun>,
}

#[derive(Clone, Serialize)]
struct GenerationRun {
    id: String,
    conversation_id: String,
    message_id: Option<i64>,
    model_name: String,
    started_at: i64,
    first_token_at: Option<i64>,
    completed_at: Option<i64>,
    status: String,
    total_duration_ms: Option<i64>,
    load_duration_ms: Option<i64>,
    prompt_eval_count: Option<i64>,
    prompt_eval_duration_ms: Option<i64>,
    eval_count: Option<i64>,
    eval_duration_ms: Option<i64>,
    tokens_per_second: Option<f64>,
    error_message: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct GenerationMetadata {
    total_duration_ms: Option<i64>,
    load_duration_ms: Option<i64>,
    prompt_eval_count: Option<i64>,
    prompt_eval_duration_ms: Option<i64>,
    eval_count: Option<i64>,
    eval_duration_ms: Option<i64>,
    tokens_per_second: Option<f64>,
}

struct GenerationCompletion {
    first_token_at: Option<i64>,
    completed_at: i64,
    status: &'static str,
    metadata: GenerationMetadata,
    error_message: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct OllamaModel {
    name: String,
    size: i64,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum OllamaStatusKind {
    Unavailable,
    RunningWithModels,
    RunningWithoutModels,
    SelectedModelMissing,
}

#[derive(Serialize)]
struct OllamaStatus {
    status: OllamaStatusKind,
    models: Vec<OllamaModel>,
    selected_model: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
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
    total_duration: Option<i64>,
    load_duration: Option<i64>,
    prompt_eval_count: Option<i64>,
    prompt_eval_duration: Option<i64>,
    eval_count: Option<i64>,
    eval_duration: Option<i64>,
}

struct OllamaChatStreamResult {
    content: String,
    first_token_at: Option<i64>,
    metadata: GenerationMetadata,
}

struct OllamaChatStreamError {
    message: String,
    content: String,
    first_token_at: Option<i64>,
    metadata: GenerationMetadata,
    cancelled: bool,
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

fn nanos_to_millis(nanos: Option<i64>) -> Option<i64> {
    nanos.map(|nanos| ((nanos as f64) / 1_000_000.0).round() as i64)
}

fn calculate_tokens_per_second(
    eval_count: Option<i64>,
    eval_duration_ns: Option<i64>,
) -> Option<f64> {
    let eval_count = eval_count?;
    let eval_duration_ns = eval_duration_ns?;

    if eval_count <= 0 || eval_duration_ns <= 0 {
        return None;
    }

    Some(eval_count as f64 / (eval_duration_ns as f64 / 1_000_000_000.0))
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
        generation_run: None,
    })
}

fn read_generation_run(
    row: &Row<'_>,
    offset: usize,
) -> Result<Option<GenerationRun>, rusqlite::Error> {
    let id = row.get::<_, Option<String>>(offset)?;

    Ok(match id {
        Some(id) => Some(GenerationRun {
            id,
            conversation_id: row.get(offset + 1)?,
            message_id: row.get(offset + 2)?,
            model_name: row.get(offset + 3)?,
            started_at: row.get(offset + 4)?,
            first_token_at: row.get(offset + 5)?,
            completed_at: row.get(offset + 6)?,
            status: row.get(offset + 7)?,
            total_duration_ms: row.get(offset + 8)?,
            load_duration_ms: row.get(offset + 9)?,
            prompt_eval_count: row.get(offset + 10)?,
            prompt_eval_duration_ms: row.get(offset + 11)?,
            eval_count: row.get(offset + 12)?,
            eval_duration_ms: row.get(offset + 13)?,
            tokens_per_second: row.get(offset + 14)?,
            error_message: row.get(offset + 15)?,
        }),
        None => None,
    })
}

fn read_chat_message_with_generation_run(row: &Row<'_>) -> Result<ChatMessage, rusqlite::Error> {
    Ok(ChatMessage {
        id: row.get(0)?,
        chat_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        created_at: row.get(4)?,
        generation_run: read_generation_run(row, 5)?,
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

fn normalize_selected_model(selected_model: Option<String>) -> Option<String> {
    selected_model
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty())
}

fn ollama_request(
    method: &str,
    path: &str,
    body: Option<String>,
) -> Result<OllamaResponse, String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 11434));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))
        .map_err(|_| "Ollama is not running. Open Ollama and try again.".to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(1200)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|error| error.to_string())?;

    let body = body.unwrap_or_default();
    let request = format!(
    "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:11434\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
    body.len(),
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

    Ok(tags.models)
}

fn build_ollama_status(
    models: Vec<OllamaModel>,
    selected_model: Option<String>,
    error: Option<String>,
) -> OllamaStatus {
    let selected_model = normalize_selected_model(selected_model);
    let status = if error.is_some() {
        OllamaStatusKind::Unavailable
    } else if models.is_empty() {
        OllamaStatusKind::RunningWithoutModels
    } else if selected_model.as_ref().is_some_and(|selected_model| {
        !models
            .iter()
            .any(|model| model.name == selected_model.as_str())
    }) {
        OllamaStatusKind::SelectedModelMissing
    } else {
        OllamaStatusKind::RunningWithModels
    };

    OllamaStatus {
        status,
        models,
        selected_model,
        error,
    }
}

fn metadata_from_ollama_chunk(chunk: &OllamaChatStreamResponse) -> GenerationMetadata {
    GenerationMetadata {
        total_duration_ms: nanos_to_millis(chunk.total_duration),
        load_duration_ms: nanos_to_millis(chunk.load_duration),
        prompt_eval_count: chunk.prompt_eval_count,
        prompt_eval_duration_ms: nanos_to_millis(chunk.prompt_eval_duration),
        eval_count: chunk.eval_count,
        eval_duration_ms: nanos_to_millis(chunk.eval_duration),
        tokens_per_second: calculate_tokens_per_second(chunk.eval_count, chunk.eval_duration),
    }
}

fn stream_error(
    message: impl Into<String>,
    content: &str,
    first_token_at: Option<i64>,
    metadata: &GenerationMetadata,
    cancelled: bool,
) -> Box<OllamaChatStreamError> {
    Box::new(OllamaChatStreamError {
        message: message.into(),
        content: content.trim().to_string(),
        first_token_at,
        metadata: metadata.clone(),
        cancelled,
    })
}

fn stream_ollama_chat(
    model: String,
    messages: Vec<OllamaChatMessage>,
    cancellation: Arc<AtomicBool>,
) -> Result<OllamaChatStreamResult, Box<OllamaChatStreamError>> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(None)
        .build()
        .map_err(|error| {
            stream_error(
                error.to_string(),
                "",
                None,
                &GenerationMetadata::default(),
                false,
            )
        })?;
    let request = OllamaChatRequest {
        model,
        messages,
        stream: true,
    };
    let response = client
        .post("http://127.0.0.1:11434/api/chat")
        .json(&request)
        .send()
        .map_err(|_| {
            stream_error(
                "Ollama is not running. Open Ollama and try again.",
                "",
                None,
                &GenerationMetadata::default(),
                false,
            )
        })?;
    let status = response.status();

    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        let message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(|error| error.as_str())
                    .map(ToString::to_string)
            })
            .unwrap_or_else(|| format!("Ollama request failed with status {status}"));
        return Err(stream_error(
            message,
            "",
            None,
            &GenerationMetadata::default(),
            false,
        ));
    }

    let mut reader = BufReader::new(response);
    let mut line = String::new();
    let mut content = String::new();
    let mut first_token_at = None;
    let mut metadata = GenerationMetadata::default();

    loop {
        if cancellation.load(Ordering::SeqCst) {
            return Err(stream_error(
                "Generation cancelled",
                &content,
                first_token_at,
                &metadata,
                true,
            ));
        }

        line.clear();
        let bytes_read = reader.read_line(&mut line).map_err(|error| {
            stream_error(
                error.to_string(),
                &content,
                first_token_at,
                &metadata,
                false,
            )
        })?;
        if bytes_read == 0 {
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let chunk = serde_json::from_str::<OllamaChatStreamResponse>(line).map_err(|error| {
            stream_error(
                error.to_string(),
                &content,
                first_token_at,
                &metadata,
                false,
            )
        })?;

        if let Some(message) = &chunk.message {
            if first_token_at.is_none() && !message.content.is_empty() {
                first_token_at = now_millis().ok();
            }
            content.push_str(&message.content);
        }

        if chunk.done {
            metadata = metadata_from_ollama_chunk(&chunk);
            break;
        }
    }

    let content = content.trim().to_string();
    if content.is_empty() {
        return Err(stream_error(
            "Ollama returned an empty response",
            "",
            first_token_at,
            &metadata,
            false,
        ));
    }

    Ok(OllamaChatStreamResult {
        content,
        first_token_at,
        metadata,
    })
}

fn get_chat_summary(
    conn: &Connection,
    chat_id: &str,
) -> Result<Option<ChatSummary>, rusqlite::Error> {
    conn.query_row(
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

fn list_messages_for_chat(
    conn: &Connection,
    chat_id: &str,
) -> Result<Vec<ChatMessage>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "
    SELECT
      m.id,
      m.chat_id,
      m.role,
      m.content,
      m.created_at,
      g.id,
      g.conversation_id,
      g.message_id,
      g.model_name,
      g.started_at,
      g.first_token_at,
      g.completed_at,
      g.status,
      g.total_duration_ms,
      g.load_duration_ms,
      g.prompt_eval_count,
      g.prompt_eval_duration_ms,
      g.eval_count,
      g.eval_duration_ms,
      g.tokens_per_second,
      g.error_message
    FROM messages m
    LEFT JOIN generation_runs g ON g.message_id = m.id
    WHERE m.chat_id = ?1
    ORDER BY m.created_at ASC, m.id ASC
    ",
    )?;

    let messages = statement
        .query_map(params![chat_id], read_chat_message_with_generation_run)?
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

    conn.execute(
        "
      INSERT INTO messages (chat_id, role, content, created_at)
      VALUES (?1, ?2, ?3, ?4)
      ",
        params![chat_id, role, content, now],
    )
    .map_err(|error| error.to_string())?;

    let message_id = conn.last_insert_rowid();

    conn.query_row(
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

fn read_generation_run_required(row: &Row<'_>) -> Result<GenerationRun, rusqlite::Error> {
    Ok(GenerationRun {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        message_id: row.get(2)?,
        model_name: row.get(3)?,
        started_at: row.get(4)?,
        first_token_at: row.get(5)?,
        completed_at: row.get(6)?,
        status: row.get(7)?,
        total_duration_ms: row.get(8)?,
        load_duration_ms: row.get(9)?,
        prompt_eval_count: row.get(10)?,
        prompt_eval_duration_ms: row.get(11)?,
        eval_count: row.get(12)?,
        eval_duration_ms: row.get(13)?,
        tokens_per_second: row.get(14)?,
        error_message: row.get(15)?,
    })
}

fn create_generation_run(
    conn: &Connection,
    conversation_id: &str,
    model_name: &str,
    started_at: i64,
) -> Result<String, String> {
    let id = create_id(conn).map_err(|error| error.to_string())?;

    conn.execute(
        "
      INSERT INTO generation_runs (
        id,
        conversation_id,
        model_name,
        started_at,
        status
      )
      VALUES (?1, ?2, ?3, ?4, 'running')
      ",
        params![id, conversation_id, model_name, started_at],
    )
    .map_err(|error| error.to_string())?;

    Ok(id)
}

fn update_generation_run(
    conn: &Connection,
    run_id: &str,
    message_id: Option<i64>,
    completion: &GenerationCompletion,
) -> Result<GenerationRun, String> {
    conn.execute(
        "
      UPDATE generation_runs
      SET message_id = ?2,
          first_token_at = ?3,
          completed_at = ?4,
          status = ?5,
          total_duration_ms = ?6,
          load_duration_ms = ?7,
          prompt_eval_count = ?8,
          prompt_eval_duration_ms = ?9,
          eval_count = ?10,
          eval_duration_ms = ?11,
          tokens_per_second = ?12,
          error_message = ?13
      WHERE id = ?1
      ",
        params![
            run_id,
            message_id,
            completion.first_token_at,
            completion.completed_at,
            completion.status,
            completion.metadata.total_duration_ms,
            completion.metadata.load_duration_ms,
            completion.metadata.prompt_eval_count,
            completion.metadata.prompt_eval_duration_ms,
            completion.metadata.eval_count,
            completion.metadata.eval_duration_ms,
            completion.metadata.tokens_per_second,
            completion.error_message.as_deref(),
        ],
    )
    .map_err(|error| error.to_string())?;

    conn.query_row(
        "
      SELECT
        id,
        conversation_id,
        message_id,
        model_name,
        started_at,
        first_token_at,
        completed_at,
        status,
        total_duration_ms,
        load_duration_ms,
        prompt_eval_count,
        prompt_eval_duration_ms,
        eval_count,
        eval_duration_ms,
        tokens_per_second,
        error_message
      FROM generation_runs
      WHERE id = ?1
      ",
        params![run_id],
        read_generation_run_required,
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

    conn.execute(
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
async fn get_ollama_status(selected_model: Option<String>) -> Result<OllamaStatus, String> {
    tauri::async_runtime::spawn_blocking(move || match read_ollama_models() {
        Ok(models) => build_ollama_status(models, selected_model, None),
        Err(error) => build_ollama_status(Vec::new(), selected_model, Some(error)),
    })
    .await
    .map_err(|error| error.to_string())
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
    let started_at = now_millis()?;
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
    let run_id = {
        let conn = store
            .conn
            .lock()
            .map_err(|_| "Database lock was poisoned".to_string())?;
        create_generation_run(&conn, &chat_id, &model, started_at)?
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

    let cancellation_for_stream = cancellation.clone();
    let stream_result = tauri::async_runtime::spawn_blocking(move || {
        stream_ollama_chat(model, ollama_messages, cancellation_for_stream)
    })
    .await;

    {
        let mut tasks = tasks
            .tasks
            .lock()
            .map_err(|_| "Generation task lock was poisoned".to_string())?;
        if tasks
            .get(&chat_id)
            .is_some_and(|current_task| Arc::ptr_eq(current_task, &cancellation))
        {
            tasks.remove(&chat_id);
        }
    }

    let stream_result = match stream_result {
        Ok(stream_result) => stream_result,
        Err(error) => {
            let completion = GenerationCompletion {
                first_token_at: None,
                completed_at: now_millis()?,
                status: "failed",
                metadata: GenerationMetadata::default(),
                error_message: Some(error.to_string()),
            };
            let conn = store
                .conn
                .lock()
                .map_err(|_| "Database lock was poisoned".to_string())?;
            update_generation_run(&conn, &run_id, None, &completion)?;
            return Err(error.to_string());
        }
    };

    match stream_result {
        Ok(result) => {
            let completion = GenerationCompletion {
                first_token_at: result.first_token_at,
                completed_at: now_millis()?,
                status: "completed",
                metadata: result.metadata,
                error_message: None,
            };
            let conn = store
                .conn
                .lock()
                .map_err(|_| "Database lock was poisoned".to_string())?;
            let mut message = insert_message(&conn, &chat_id, "assistant", &result.content)?;
            let run = update_generation_run(&conn, &run_id, Some(message.id), &completion)?;
            message.generation_run = Some(run);
            Ok(message)
        }
        Err(error) => {
            let error = *error;
            let status = if error.cancelled {
                "cancelled"
            } else {
                "failed"
            };
            let completion = GenerationCompletion {
                first_token_at: error.first_token_at,
                completed_at: now_millis()?,
                status,
                metadata: error.metadata,
                error_message: Some(error.message.clone()),
            };
            let conn = store
                .conn
                .lock()
                .map_err(|_| "Database lock was poisoned".to_string())?;

            if error.content.is_empty() {
                if let Err(update_error) = update_generation_run(&conn, &run_id, None, &completion)
                {
                    if error.cancelled {
                        return Err("Generation cancelled".to_string());
                    }

                    return Err(update_error);
                }

                return Err(error.message);
            }

            let mut message = match insert_message(&conn, &chat_id, "assistant", &error.content) {
                Ok(message) => message,
                Err(insert_error) => {
                    if error.cancelled {
                        return Err("Generation cancelled".to_string());
                    }

                    return Err(insert_error);
                }
            };
            let run = match update_generation_run(&conn, &run_id, Some(message.id), &completion) {
                Ok(run) => run,
                Err(update_error) => {
                    if error.cancelled {
                        return Err("Generation cancelled".to_string());
                    }

                    return Err(update_error);
                }
            };
            message.generation_run = Some(run);
            Ok(message)
        }
    }
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

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_chats,
            search_chats,
            create_chat,
            get_messages,
            add_message,
            delete_chat,
            get_ollama_status,
            list_ollama_models,
            download_ollama_model,
            delete_ollama_model,
            generate_assistant_response,
            cancel_ollama_generation
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{
        build_ollama_status, calculate_tokens_per_second, escape_like_pattern, nanos_to_millis,
        normalize_title, validate_ollama_model_name, OllamaModel, OllamaStatusKind,
    };

    fn test_model(name: &str) -> OllamaModel {
        OllamaModel {
            name: name.to_string(),
            size: 1024,
        }
    }

    #[test]
    fn normalize_title_defaults_for_missing_or_blank_titles() {
        assert_eq!(normalize_title(None), "New chat");
        assert_eq!(normalize_title(Some("   ".to_string())), "New chat");
    }

    #[test]
    fn normalize_title_trims_and_truncates_long_titles() {
        assert_eq!(
            normalize_title(Some("  Focused local workspace  ".to_string())),
            "Focused local workspace"
        );

        let long_title = "a".repeat(65);
        assert_eq!(
            normalize_title(Some(long_title)),
            format!("{}...", "a".repeat(64))
        );
    }

    #[test]
    fn escape_like_pattern_escapes_sql_like_wildcards() {
        assert_eq!(
            escape_like_pattern(r"100%_local\chat"),
            r"100\%\_local\\chat"
        );
    }

    #[test]
    fn validate_ollama_model_name_accepts_common_local_model_names() {
        assert_eq!(
            validate_ollama_model_name(" llama3.2:3b ").unwrap(),
            "llama3.2:3b"
        );
    }

    #[test]
    fn validate_ollama_model_name_rejects_empty_or_unsafe_names() {
        assert!(validate_ollama_model_name("   ").is_err());
        assert!(validate_ollama_model_name("llama 3").is_err());
        assert!(validate_ollama_model_name("bad\"name").is_err());
        assert!(validate_ollama_model_name(r"bad\name").is_err());
    }

    #[test]
    fn nanos_to_millis_rounds_ollama_nanosecond_durations() {
        assert_eq!(nanos_to_millis(Some(1_500_000)), Some(2));
        assert_eq!(nanos_to_millis(Some(999_999)), Some(1));
        assert_eq!(nanos_to_millis(None), None);
    }

    #[test]
    fn calculate_tokens_per_second_uses_eval_duration_seconds() {
        assert_eq!(
            calculate_tokens_per_second(Some(50), Some(2_000_000_000)),
            Some(25.0)
        );
        assert_eq!(calculate_tokens_per_second(Some(50), Some(0)), None);
        assert_eq!(
            calculate_tokens_per_second(Some(0), Some(2_000_000_000)),
            None
        );
        assert_eq!(calculate_tokens_per_second(None, Some(2_000_000_000)), None);
    }

    #[test]
    fn build_ollama_status_reports_unavailable_with_error_details() {
        let status = build_ollama_status(
            Vec::new(),
            Some("llama3.2:3b".to_string()),
            Some("connection refused".to_string()),
        );

        assert_eq!(status.status, OllamaStatusKind::Unavailable);
        assert_eq!(status.selected_model.as_deref(), Some("llama3.2:3b"));
        assert_eq!(status.error.as_deref(), Some("connection refused"));
    }

    #[test]
    fn build_ollama_status_distinguishes_empty_and_ready_model_lists() {
        let empty_status = build_ollama_status(Vec::new(), None, None);
        assert_eq!(empty_status.status, OllamaStatusKind::RunningWithoutModels);

        let ready_status = build_ollama_status(vec![test_model("llama3.2:3b")], None, None);
        assert_eq!(ready_status.status, OllamaStatusKind::RunningWithModels);
    }

    #[test]
    fn build_ollama_status_reports_missing_selected_model() {
        let status = build_ollama_status(
            vec![test_model("llama3.2:1b")],
            Some("llama3.2:3b".to_string()),
            None,
        );

        assert_eq!(status.status, OllamaStatusKind::SelectedModelMissing);
    }
}
