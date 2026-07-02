mod app;
mod domain;
mod infra;

use app::{benchmarks as benchmark_service, jobs as job_service, models as model_service};
use domain::benchmark::{ModelBenchmark, ModelUsage};
use domain::database::DatabaseDiagnostics;
use domain::job::{Job, JobEvent, JobStatus, JobType};
use domain::model::{validate_ollama_model_name, OllamaModel, OllamaStatus};
use domain::search::ChatSearchResult;
use infra::{
    benchmarks::CompletedBenchmarkMetrics, jobs as job_repository, ollama, search, sqlite,
};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone)]
struct ChatStore {
    conn: Arc<Mutex<Connection>>,
    db_path: PathBuf,
}

#[derive(Default)]
struct GenerationTasks {
    tasks: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

#[derive(Default)]
struct JobTasks {
    tasks: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

#[derive(Default)]
struct BenchmarkTasks {
    active_job_id: Mutex<Option<String>>,
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

#[derive(Copy, Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ChatExportFormat {
    Markdown,
    Json,
    PlainText,
}

#[derive(Serialize)]
struct ChatExport {
    file_name: String,
    mime_type: String,
    content: String,
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

impl ChatStore {
    fn new(db_path: PathBuf) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(&db_path)?;
        sqlite::setup_database(&conn)?;
        let recovered_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or_default();
        job_repository::mark_interrupted(&conn, recovered_at)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path,
        })
    }
}

fn now_millis() -> Result<i64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?;

    Ok(duration.as_millis() as i64)
}

fn emit_job_event(app: &AppHandle, job: &Job) -> Result<(), String> {
    app.emit(
        "job_updated",
        JobEvent {
            job_id: job.id.clone(),
            job_type: job.job_type,
            job: job.clone(),
        },
    )
    .map_err(|error| error.to_string())
}

fn model_pull_label(model: &str, status: &str) -> String {
    if status == "success" {
        return format!("Downloaded {model}");
    }

    format!("Downloading {model}: {status}")
}

fn update_model_pull_progress(
    store: &ChatStore,
    app: &AppHandle,
    job_id: &str,
    model: &str,
    progress: ollama::PullProgress,
) -> Result<(), String> {
    let label = model_pull_label(model, &progress.status);
    let job = {
        let conn = store
            .conn
            .lock()
            .map_err(|_| "Database lock was poisoned".to_string())?;
        job_service::update_progress(
            &conn,
            job_id,
            progress.completed,
            progress.total,
            Some(&label),
        )?
    };

    emit_job_event(app, &job)
}

fn run_model_pull_job(
    store: ChatStore,
    app: AppHandle,
    job_id: String,
    model: String,
    cancellation: Arc<AtomicBool>,
) -> Result<Job, String> {
    let running_job = {
        let conn = store
            .conn
            .lock()
            .map_err(|_| "Database lock was poisoned".to_string())?;
        job_service::start(&conn, &job_id, now_millis()?)?
    };
    emit_job_event(&app, &running_job)?;

    let pull_result = ollama::pull_model_stream(model.clone(), cancellation.clone(), |progress| {
        update_model_pull_progress(&store, &app, &job_id, &model, progress)
    });
    let completed_at = now_millis()?;
    let final_job = match pull_result {
        Ok(()) if cancellation.load(Ordering::SeqCst) => {
            let conn = store
                .conn
                .lock()
                .map_err(|_| "Database lock was poisoned".to_string())?;
            job_service::finish_cancelled(&conn, &job_id, completed_at)?
        }
        Ok(()) => {
            let models = model_service::list_models()?;
            let result_json = serde_json::json!({
              "model": model,
              "models": models
            })
            .to_string();
            let conn = store
                .conn
                .lock()
                .map_err(|_| "Database lock was poisoned".to_string())?;
            job_service::finish_succeeded(&conn, &job_id, Some(&result_json), completed_at)?
        }
        Err(error) if cancellation.load(Ordering::SeqCst) || error == "Job cancelled" => {
            let conn = store
                .conn
                .lock()
                .map_err(|_| "Database lock was poisoned".to_string())?;
            job_service::finish_cancelled(&conn, &job_id, completed_at)?
        }
        Err(error) => {
            let conn = store
                .conn
                .lock()
                .map_err(|_| "Database lock was poisoned".to_string())?;
            job_service::finish_failed(&conn, &job_id, &error, completed_at)?
        }
    };

    emit_job_event(&app, &final_job)?;
    Ok(final_job)
}

fn update_model_benchmark_progress(
    store: &ChatStore,
    app: &AppHandle,
    job_id: &str,
    model: &str,
    prompt_label: &str,
    progress_current: i64,
    progress_total: i64,
) -> Result<(), String> {
    let label = if progress_current >= progress_total {
        format!("Benchmarked {model}")
    } else {
        format!("Benchmarking {model}: {prompt_label}")
    };
    let job = {
        let conn = store
            .conn
            .lock()
            .map_err(|_| "Database lock was poisoned".to_string())?;
        job_service::update_progress(
            &conn,
            job_id,
            Some(progress_current),
            Some(progress_total),
            Some(&label),
        )?
    };

    emit_job_event(app, &job)
}

fn finish_model_benchmark_cancelled(
    store: &ChatStore,
    job_id: &str,
    completed_at: i64,
) -> Result<Job, String> {
    let conn = store
        .conn
        .lock()
        .map_err(|_| "Database lock was poisoned".to_string())?;
    benchmark_service::mark_remaining_cancelled(&conn, job_id, completed_at)?;
    job_service::finish_cancelled(&conn, job_id, completed_at)
}

fn finish_model_benchmark_failed(
    store: &ChatStore,
    job_id: &str,
    completed_at: i64,
    error_message: &str,
) -> Result<Job, String> {
    let conn = store
        .conn
        .lock()
        .map_err(|_| "Database lock was poisoned".to_string())?;
    benchmark_service::mark_remaining_failed(&conn, job_id, completed_at, error_message)?;
    job_service::finish_failed(&conn, job_id, error_message, completed_at)
}

fn benchmark_metrics(
    started_at: i64,
    completed_at: i64,
    first_token_at: Option<i64>,
    metadata: &GenerationMetadata,
) -> CompletedBenchmarkMetrics {
    CompletedBenchmarkMetrics {
        total_duration_ms: metadata
            .total_duration_ms
            .or_else(|| Some(completed_at.saturating_sub(started_at))),
        first_token_ms: first_token_at
            .map(|first_token_at| first_token_at.saturating_sub(started_at)),
        prompt_eval_count: metadata.prompt_eval_count,
        prompt_eval_duration_ms: metadata.prompt_eval_duration_ms,
        eval_count: metadata.eval_count,
        eval_duration_ms: metadata.eval_duration_ms,
        tokens_per_second: metadata.tokens_per_second,
    }
}

fn run_model_benchmark_job(
    store: ChatStore,
    app: AppHandle,
    job_id: String,
    model: String,
    benchmarks: Vec<ModelBenchmark>,
    cancellation: Arc<AtomicBool>,
) -> Result<Job, String> {
    let running_job = {
        let conn = store
            .conn
            .lock()
            .map_err(|_| "Database lock was poisoned".to_string())?;
        job_service::start(&conn, &job_id, now_millis()?)?
    };
    emit_job_event(&app, &running_job)?;

    let installed_models = model_service::list_models()?;
    if !installed_models
        .iter()
        .any(|installed| installed.name == model)
    {
        let completed_at = now_millis()?;
        let final_job = finish_model_benchmark_failed(
            &store,
            &job_id,
            completed_at,
            "Model is not installed. Install it before benchmarking.",
        )?;
        emit_job_event(&app, &final_job)?;
        return Ok(final_job);
    }

    let progress_total = benchmarks.len() as i64;
    for (index, prompt) in benchmark_service::BENCHMARK_SUITE.iter().enumerate() {
        let benchmark = benchmarks
            .iter()
            .find(|benchmark| benchmark.prompt_type == prompt.prompt_type)
            .ok_or_else(|| format!("Benchmark row was not found for {}", prompt.prompt_type))?;

        if cancellation.load(Ordering::SeqCst) {
            let completed_at = now_millis()?;
            let final_job = finish_model_benchmark_cancelled(&store, &job_id, completed_at)?;
            emit_job_event(&app, &final_job)?;
            return Ok(final_job);
        }

        update_model_benchmark_progress(
            &store,
            &app,
            &job_id,
            &model,
            prompt.prompt_label,
            index as i64,
            progress_total,
        )?;

        let started_at = now_millis()?;
        {
            let conn = store
                .conn
                .lock()
                .map_err(|_| "Database lock was poisoned".to_string())?;
            benchmark_service::mark_running(&conn, &benchmark.id, started_at)?;
        }

        let stream_result = stream_ollama_chat(
            model.clone(),
            vec![OllamaChatMessage {
                role: "user".to_string(),
                content: prompt.prompt_text.to_string(),
            }],
            cancellation.clone(),
        );
        let completed_at = now_millis()?;

        match stream_result {
            Ok(result) => {
                let metrics = benchmark_metrics(
                    started_at,
                    completed_at,
                    result.first_token_at,
                    &result.metadata,
                );
                let conn = store
                    .conn
                    .lock()
                    .map_err(|_| "Database lock was poisoned".to_string())?;
                benchmark_service::mark_completed(&conn, &benchmark.id, completed_at, &metrics)?;
            }
            Err(error) if error.cancelled || cancellation.load(Ordering::SeqCst) => {
                {
                    let conn = store
                        .conn
                        .lock()
                        .map_err(|_| "Database lock was poisoned".to_string())?;
                    benchmark_service::mark_cancelled(&conn, &benchmark.id, completed_at)?;
                }
                let final_job = finish_model_benchmark_cancelled(&store, &job_id, completed_at)?;
                emit_job_event(&app, &final_job)?;
                return Ok(final_job);
            }
            Err(error) => {
                {
                    let conn = store
                        .conn
                        .lock()
                        .map_err(|_| "Database lock was poisoned".to_string())?;
                    benchmark_service::mark_failed(
                        &conn,
                        &benchmark.id,
                        completed_at,
                        &error.message,
                    )?;
                }
                let final_job =
                    finish_model_benchmark_failed(&store, &job_id, completed_at, &error.message)?;
                emit_job_event(&app, &final_job)?;
                return Ok(final_job);
            }
        }

        update_model_benchmark_progress(
            &store,
            &app,
            &job_id,
            &model,
            prompt.prompt_label,
            (index + 1) as i64,
            progress_total,
        )?;
    }

    let completed_at = now_millis()?;
    let result_json = serde_json::json!({
      "model": model,
      "benchmark_count": benchmarks.len()
    })
    .to_string();
    let final_job = {
        let conn = store
            .conn
            .lock()
            .map_err(|_| "Database lock was poisoned".to_string())?;
        job_service::finish_succeeded(&conn, &job_id, Some(&result_json), completed_at)?
    };
    emit_job_event(&app, &final_job)?;
    Ok(final_job)
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

fn sanitize_file_name(value: &str) -> String {
    let mut sanitized = String::new();
    let mut previous_was_separator = false;

    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            sanitized.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if (character.is_whitespace() || matches!(character, '-' | '_'))
            && !sanitized.is_empty()
            && !previous_was_separator
        {
            sanitized.push('-');
            previous_was_separator = true;
        }

        if sanitized.len() >= 80 {
            break;
        }
    }

    let sanitized = sanitized.trim_matches('-').to_string();
    if sanitized.is_empty() {
        "atlas-chat".to_string()
    } else {
        sanitized
    }
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let month_piece = (5 * doy + 2) / 153;
    let day = doy - (153 * month_piece + 2) / 5 + 1;
    let month = month_piece + if month_piece < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };

    (year, month, day)
}

fn format_timestamp_ms(timestamp_ms: i64) -> String {
    let seconds = timestamp_ms.div_euclid(1_000);
    let milliseconds = timestamp_ms.rem_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hours = day_seconds / 3_600;
    let minutes = (day_seconds % 3_600) / 60;
    let seconds = day_seconds % 60;

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{milliseconds:03}Z")
}

fn format_duration_for_export(duration_ms: i64) -> String {
    if duration_ms < 1_000 {
        return format!("{duration_ms} ms");
    }

    format!("{:.2} s", duration_ms as f64 / 1_000.0)
}

fn chat_export_format_parts(format: ChatExportFormat) -> (&'static str, &'static str) {
    match format {
        ChatExportFormat::Markdown => ("md", "text/markdown;charset=utf-8"),
        ChatExportFormat::Json => ("json", "application/json;charset=utf-8"),
        ChatExportFormat::PlainText => ("txt", "text/plain;charset=utf-8"),
    }
}

fn role_label(role: &str) -> &str {
    match role {
        "assistant" => "Assistant",
        "system" => "System",
        "user" => "User",
        _ => "Message",
    }
}

fn escape_markdown_inline(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for character in value.chars() {
        if matches!(
            character,
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }

    escaped
}

fn append_generation_markdown(output: &mut String, run: &GenerationRun) {
    output.push_str("\n\nGeneration details:\n\n");
    output.push_str(&format!("- Model: `{}`\n", run.model_name));
    output.push_str(&format!("- Status: {}\n", run.status));

    if let Some(total_duration_ms) = run.total_duration_ms {
        output.push_str(&format!(
            "- Total duration: {}\n",
            format_duration_for_export(total_duration_ms)
        ));
    }

    if let Some(load_duration_ms) = run.load_duration_ms {
        output.push_str(&format!(
            "- Load duration: {}\n",
            format_duration_for_export(load_duration_ms)
        ));
    }

    if let Some(prompt_eval_count) = run.prompt_eval_count {
        output.push_str(&format!("- Prompt tokens: {prompt_eval_count}\n"));
    }

    if let Some(eval_count) = run.eval_count {
        output.push_str(&format!("- Completion tokens: {eval_count}\n"));
    }

    if let Some(tokens_per_second) = run.tokens_per_second {
        output.push_str(&format!("- Speed: {:.1} tokens/s\n", tokens_per_second));
    }

    if let Some(error_message) = &run.error_message {
        output.push_str(&format!("- Error: {error_message}\n"));
    }
}

fn append_generation_text(output: &mut String, run: &GenerationRun) {
    let mut details = vec![
        format!("model: {}", run.model_name),
        format!("status: {}", run.status),
    ];

    if let Some(total_duration_ms) = run.total_duration_ms {
        details.push(format!(
            "total: {}",
            format_duration_for_export(total_duration_ms)
        ));
    }

    if let Some(prompt_eval_count) = run.prompt_eval_count {
        details.push(format!("prompt tokens: {prompt_eval_count}"));
    }

    if let Some(eval_count) = run.eval_count {
        details.push(format!("completion tokens: {eval_count}"));
    }

    if let Some(tokens_per_second) = run.tokens_per_second {
        details.push(format!("speed: {:.1} tokens/s", tokens_per_second));
    }

    if let Some(error_message) = &run.error_message {
        details.push(format!("error: {error_message}"));
    }

    output.push_str(&format!("Generation: {}\n", details.join("; ")));
}

fn render_chat_export_markdown(
    chat: &ChatSummary,
    messages: &[ChatMessage],
    exported_at: i64,
) -> String {
    let mut output = String::new();
    output.push_str(&format!("# {}\n\n", escape_markdown_inline(&chat.title)));
    output.push_str(&format!("- Chat ID: `{}`\n", chat.id));
    output.push_str(&format!(
        "- Created: {}\n",
        format_timestamp_ms(chat.created_at)
    ));
    output.push_str(&format!(
        "- Updated: {}\n",
        format_timestamp_ms(chat.updated_at)
    ));
    output.push_str(&format!(
        "- Exported: {}\n",
        format_timestamp_ms(exported_at)
    ));
    output.push_str(&format!("- Messages: {}\n\n", chat.message_count));

    if messages.is_empty() {
        output.push_str("_No messages._\n");
        return output;
    }

    for message in messages {
        output.push_str(&format!(
            "## {} - {}\n\n",
            role_label(&message.role),
            format_timestamp_ms(message.created_at)
        ));
        output.push_str(&message.content);

        if let Some(run) = &message.generation_run {
            append_generation_markdown(&mut output, run);
        }

        output.push_str("\n\n");
    }

    output
}

fn render_chat_export_text(
    chat: &ChatSummary,
    messages: &[ChatMessage],
    exported_at: i64,
) -> String {
    let mut output = String::new();
    output.push_str(&chat.title);
    output.push('\n');
    output.push_str(&"=".repeat(chat.title.chars().count().max(1)));
    output.push_str("\n\n");
    output.push_str(&format!("Chat ID: {}\n", chat.id));
    output.push_str(&format!(
        "Created: {}\n",
        format_timestamp_ms(chat.created_at)
    ));
    output.push_str(&format!(
        "Updated: {}\n",
        format_timestamp_ms(chat.updated_at)
    ));
    output.push_str(&format!("Exported: {}\n", format_timestamp_ms(exported_at)));
    output.push_str(&format!("Messages: {}\n\n", chat.message_count));

    if messages.is_empty() {
        output.push_str("No messages.\n");
        return output;
    }

    for message in messages {
        output.push_str(&format!(
            "[{} | {}]\n",
            role_label(&message.role),
            format_timestamp_ms(message.created_at)
        ));
        output.push_str(&message.content);
        output.push('\n');

        if let Some(run) = &message.generation_run {
            append_generation_text(&mut output, run);
        }

        output.push('\n');
    }

    output
}

fn render_chat_export_json(
    chat: &ChatSummary,
    messages: &[ChatMessage],
    exported_at: i64,
) -> Result<String, String> {
    let messages = messages
        .iter()
        .map(|message| {
            serde_json::json!({
              "id": message.id,
              "chat_id": &message.chat_id,
              "role": &message.role,
              "content": &message.content,
              "created_at": message.created_at,
              "created_at_iso": format_timestamp_ms(message.created_at),
              "generation_run": &message.generation_run
            })
        })
        .collect::<Vec<_>>();
    let document = serde_json::json!({
      "format_version": 1,
      "exported_at": exported_at,
      "exported_at_iso": format_timestamp_ms(exported_at),
      "chat": {
        "id": &chat.id,
        "title": &chat.title,
        "created_at": chat.created_at,
        "created_at_iso": format_timestamp_ms(chat.created_at),
        "updated_at": chat.updated_at,
        "updated_at_iso": format_timestamp_ms(chat.updated_at),
        "message_count": chat.message_count
      },
      "messages": messages
    });

    serde_json::to_string_pretty(&document)
        .map(|content| format!("{content}\n"))
        .map_err(|error| error.to_string())
}

fn render_chat_export(
    chat: &ChatSummary,
    messages: &[ChatMessage],
    format: ChatExportFormat,
    exported_at: i64,
) -> Result<String, String> {
    match format {
        ChatExportFormat::Markdown => Ok(render_chat_export_markdown(chat, messages, exported_at)),
        ChatExportFormat::Json => render_chat_export_json(chat, messages, exported_at),
        ChatExportFormat::PlainText => Ok(render_chat_export_text(chat, messages, exported_at)),
    }
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
fn search_conversations(
    store: State<'_, ChatStore>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<ChatSearchResult>, String> {
    let conn = store
        .conn
        .lock()
        .map_err(|_| "Database lock was poisoned".to_string())?;
    search::search_conversations(&conn, &query, limit)
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
fn export_chat(
    store: State<'_, ChatStore>,
    chat_id: String,
    format: ChatExportFormat,
) -> Result<ChatExport, String> {
    let exported_at = now_millis()?;
    let conn = store
        .conn
        .lock()
        .map_err(|_| "Database lock was poisoned".to_string())?;
    let chat = get_chat_summary(&conn, &chat_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Chat was not found".to_string())?;
    let messages = list_messages_for_chat(&conn, &chat_id).map_err(|error| error.to_string())?;
    let content = render_chat_export(&chat, &messages, format, exported_at)?;
    let (extension, mime_type) = chat_export_format_parts(format);
    let short_id = chat.id.chars().take(8).collect::<String>();
    let file_name = format!(
        "{}-{}.{}",
        sanitize_file_name(&chat.title),
        short_id,
        extension
    );

    Ok(ChatExport {
        file_name,
        mime_type: mime_type.to_string(),
        content,
    })
}

#[tauri::command]
async fn get_ollama_status(selected_model: Option<String>) -> Result<OllamaStatus, String> {
    tauri::async_runtime::spawn_blocking(move || model_service::get_status(selected_model))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_ollama_models() -> Result<Vec<OllamaModel>, String> {
    tauri::async_runtime::spawn_blocking(model_service::list_models)
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
fn list_jobs(store: State<'_, ChatStore>, limit: Option<i64>) -> Result<Vec<Job>, String> {
    let conn = store
        .conn
        .lock()
        .map_err(|_| "Database lock was poisoned".to_string())?;
    job_service::list_recent(&conn, limit.unwrap_or(10))
}

#[tauri::command]
fn get_database_diagnostics(store: State<'_, ChatStore>) -> Result<DatabaseDiagnostics, String> {
    let conn = store
        .conn
        .lock()
        .map_err(|_| "Database lock was poisoned".to_string())?;
    sqlite::diagnostics(&conn, &store.db_path).map_err(|error| error.to_string())
}

#[tauri::command]
fn list_model_benchmarks(
    store: State<'_, ChatStore>,
    limit: Option<i64>,
) -> Result<Vec<ModelBenchmark>, String> {
    let conn = store
        .conn
        .lock()
        .map_err(|_| "Database lock was poisoned".to_string())?;
    benchmark_service::list_recent(&conn, limit.unwrap_or(50))
}

#[tauri::command]
fn list_model_usage(store: State<'_, ChatStore>) -> Result<Vec<ModelUsage>, String> {
    let conn = store
        .conn
        .lock()
        .map_err(|_| "Database lock was poisoned".to_string())?;
    let mut statement = conn
        .prepare(
            "
      SELECT
        model_name,
        MAX(started_at) AS last_used_at,
        COUNT(*) AS generation_count
      FROM generation_runs
      GROUP BY model_name
      ORDER BY last_used_at DESC
      ",
        )
        .map_err(|error| error.to_string())?;

    let usage = statement
        .query_map([], |row| {
            Ok(ModelUsage {
                model_name: row.get(0)?,
                last_used_at: row.get(1)?,
                generation_count: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(usage)
}

#[tauri::command]
fn cancel_job(
    app: AppHandle,
    store: State<'_, ChatStore>,
    tasks: State<'_, JobTasks>,
    job_id: String,
) -> Result<Job, String> {
    let job = {
        let conn = store
            .conn
            .lock()
            .map_err(|_| "Database lock was poisoned".to_string())?;
        let existing_job =
            job_service::get(&conn, &job_id)?.ok_or_else(|| "Job was not found".to_string())?;

        if job_service::is_terminal(existing_job.status) {
            existing_job
        } else {
            job_service::request_cancel(&conn, &job_id, now_millis()?)?
        }
    };

    if !job_service::is_terminal(job.status) {
        let cancellation = tasks
            .tasks
            .lock()
            .map_err(|_| "Job task lock was poisoned".to_string())?
            .remove(&job_id);

        if let Some(cancellation) = cancellation {
            cancellation.store(true, Ordering::SeqCst);
        }
    }

    emit_job_event(&app, &job)?;
    Ok(job)
}

#[tauri::command]
async fn start_model_benchmark(
    app: AppHandle,
    store: State<'_, ChatStore>,
    tasks: State<'_, JobTasks>,
    benchmark_tasks: State<'_, BenchmarkTasks>,
    model: String,
) -> Result<Job, String> {
    let model = validate_ollama_model_name(&model)?;
    let payload_json = serde_json::json!({
      "model": &model,
      "suite": benchmark_service::BENCHMARK_SUITE
        .iter()
        .map(|prompt| prompt.prompt_type)
        .collect::<Vec<_>>()
    })
    .to_string();
    let now = now_millis()?;
    let cancellation = Arc::new(AtomicBool::new(false));
    let (job, benchmarks) = {
        let mut active_job_id = benchmark_tasks
            .active_job_id
            .lock()
            .map_err(|_| "Benchmark task lock was poisoned".to_string())?;

        if active_job_id.is_some() {
            return Err("A model benchmark is already running.".to_string());
        }

        let conn = store
            .conn
            .lock()
            .map_err(|_| "Database lock was poisoned".to_string())?;
        let job = job_service::create(
            &conn,
            JobType::ModelBenchmark,
            &format!("Queued benchmark for {model}"),
            Some(&payload_json),
            now,
        )?;
        let benchmarks = benchmark_service::create_suite(&conn, &job.id, &model, now)?;
        tasks
            .tasks
            .lock()
            .map_err(|_| "Job task lock was poisoned".to_string())?
            .insert(job.id.clone(), cancellation.clone());
        *active_job_id = Some(job.id.clone());
        (job, benchmarks)
    };

    if let Err(error) = emit_job_event(&app, &job) {
        tasks
            .tasks
            .lock()
            .map_err(|_| "Job task lock was poisoned".to_string())?
            .remove(&job.id);
        let mut active_job_id = benchmark_tasks
            .active_job_id
            .lock()
            .map_err(|_| "Benchmark task lock was poisoned".to_string())?;
        if active_job_id.as_deref() == Some(job.id.as_str()) {
            *active_job_id = None;
        }
        return Err(error);
    }

    let store_for_job = store.inner().clone();
    let app_for_job = app.clone();
    let job_id = job.id.clone();
    let job_id_for_cleanup = job.id.clone();
    let cancellation_for_job = cancellation.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        run_model_benchmark_job(
            store_for_job,
            app_for_job,
            job_id,
            model,
            benchmarks,
            cancellation_for_job,
        )
    })
    .await
    .map_err(|error| error.to_string());

    {
        let mut tasks = tasks
            .tasks
            .lock()
            .map_err(|_| "Job task lock was poisoned".to_string())?;
        if tasks
            .get(&job_id_for_cleanup)
            .is_some_and(|current_task| Arc::ptr_eq(current_task, &cancellation))
        {
            tasks.remove(&job_id_for_cleanup);
        }
    }
    {
        let mut active_job_id = benchmark_tasks
            .active_job_id
            .lock()
            .map_err(|_| "Benchmark task lock was poisoned".to_string())?;
        if active_job_id.as_deref() == Some(job_id_for_cleanup.as_str()) {
            *active_job_id = None;
        }
    }

    let final_job = result??;
    match final_job.status {
        JobStatus::Succeeded => Ok(final_job),
        JobStatus::Cancelled => Err("Job cancelled".to_string()),
        JobStatus::Failed => Err(final_job
            .error_message
            .clone()
            .unwrap_or_else(|| "Job failed".to_string())),
        _ => Ok(final_job),
    }
}

#[tauri::command]
async fn download_ollama_model(
    app: AppHandle,
    store: State<'_, ChatStore>,
    tasks: State<'_, JobTasks>,
    model: String,
) -> Result<Job, String> {
    let model = validate_ollama_model_name(&model)?;
    let payload_json = serde_json::json!({ "model": &model }).to_string();
    let job = {
        let conn = store
            .conn
            .lock()
            .map_err(|_| "Database lock was poisoned".to_string())?;
        job_service::create(
            &conn,
            JobType::ModelPull,
            &format!("Queued download for {model}"),
            Some(&payload_json),
            now_millis()?,
        )?
    };
    let cancellation = Arc::new(AtomicBool::new(false));
    {
        tasks
            .tasks
            .lock()
            .map_err(|_| "Job task lock was poisoned".to_string())?
            .insert(job.id.clone(), cancellation.clone());
    }
    emit_job_event(&app, &job)?;

    let store_for_job = store.inner().clone();
    let app_for_job = app.clone();
    let job_id = job.id.clone();
    let job_id_for_cleanup = job.id.clone();
    let cancellation_for_job = cancellation.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        run_model_pull_job(
            store_for_job,
            app_for_job,
            job_id,
            model,
            cancellation_for_job,
        )
    })
    .await
    .map_err(|error| error.to_string())?;

    {
        let mut tasks = tasks
            .tasks
            .lock()
            .map_err(|_| "Job task lock was poisoned".to_string())?;
        if tasks
            .get(&job_id_for_cleanup)
            .is_some_and(|current_task| Arc::ptr_eq(current_task, &cancellation))
        {
            tasks.remove(&job_id_for_cleanup);
        }
    }

    let final_job = result?;
    match final_job.status {
        JobStatus::Succeeded => Ok(final_job),
        JobStatus::Cancelled => Err("Job cancelled".to_string()),
        JobStatus::Failed => Err(final_job
            .error_message
            .clone()
            .unwrap_or_else(|| "Job failed".to_string())),
        _ => Ok(final_job),
    }
}

#[tauri::command]
async fn delete_ollama_model(model: String) -> Result<Vec<OllamaModel>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let model = validate_ollama_model_name(&model)?;
        let body = serde_json::json!({ "name": model }).to_string();
        let response = ollama::request("DELETE", "/api/delete", Some(body))?;

        if !(200..300).contains(&response.status_code) {
            return Err(ollama::error(&response));
        }

        model_service::list_models()
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
            app.manage(JobTasks::default());
            app.manage(BenchmarkTasks::default());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_chats,
            search_chats,
            search_conversations,
            create_chat,
            get_messages,
            add_message,
            delete_chat,
            export_chat,
            get_ollama_status,
            list_ollama_models,
            list_jobs,
            get_database_diagnostics,
            list_model_benchmarks,
            list_model_usage,
            cancel_job,
            start_model_benchmark,
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
    use rusqlite::{params, Connection};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::app::{benchmarks as benchmark_service, jobs as job_service};
    use super::domain::benchmark::ModelBenchmarkStatus;
    use super::domain::job::{JobStatus, JobType};
    use super::domain::model::{
        build_ollama_status, validate_ollama_model_name, OllamaModel, OllamaStatusKind,
    };
    use super::domain::search::SearchResultSource;
    use super::infra::{
        benchmarks::{self as benchmark_repository, CompletedBenchmarkMetrics},
        jobs as job_repository, search, sqlite,
    };
    use super::{
        calculate_tokens_per_second, escape_like_pattern, format_timestamp_ms, nanos_to_millis,
        normalize_title, render_chat_export, sanitize_file_name, ChatExportFormat, ChatMessage,
        ChatSummary, GenerationRun,
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
    fn format_timestamp_ms_outputs_utc_iso_8601() {
        assert_eq!(
            format_timestamp_ms(0),
            "1970-01-01T00:00:00.000Z".to_string()
        );
        assert_eq!(
            format_timestamp_ms(1_700_000_000_123),
            "2023-11-14T22:13:20.123Z".to_string()
        );
    }

    #[test]
    fn sanitize_file_name_keeps_exports_safe_and_readable() {
        assert_eq!(
            sanitize_file_name("  Quarterly Chat: Plan / Draft?  "),
            "quarterly-chat-plan-draft"
        );
        assert_eq!(sanitize_file_name("////"), "atlas-chat");
    }

    #[test]
    fn render_chat_export_includes_metrics_and_failed_states() {
        let chat = ChatSummary {
            id: "abcdef123456".to_string(),
            title: "Export Check".to_string(),
            created_at: 0,
            updated_at: 1_700_000_000_123,
            message_count: 2,
        };
        let messages = vec![
            ChatMessage {
                id: 1,
                chat_id: chat.id.clone(),
                role: "user".to_string(),
                content: "Summarize this.".to_string(),
                created_at: 0,
                generation_run: None,
            },
            ChatMessage {
                id: 2,
                chat_id: chat.id.clone(),
                role: "assistant".to_string(),
                content: "Partial answer".to_string(),
                created_at: 1_000,
                generation_run: Some(GenerationRun {
                    id: "run-1".to_string(),
                    conversation_id: chat.id.clone(),
                    message_id: Some(2),
                    model_name: "llama3.2:3b".to_string(),
                    started_at: 900,
                    first_token_at: Some(950),
                    completed_at: Some(1_100),
                    status: "failed".to_string(),
                    total_duration_ms: Some(1_500),
                    load_duration_ms: Some(25),
                    prompt_eval_count: Some(12),
                    prompt_eval_duration_ms: Some(20),
                    eval_count: Some(6),
                    eval_duration_ms: Some(100),
                    tokens_per_second: Some(60.0),
                    error_message: Some("Generation failed".to_string()),
                }),
            },
        ];

        let markdown =
            render_chat_export(&chat, &messages, ChatExportFormat::Markdown, 2_000).unwrap();
        assert!(markdown.contains("# Export Check"));
        assert!(markdown.contains("- Model: `llama3.2:3b`"));
        assert!(markdown.contains("- Status: failed"));
        assert!(markdown.contains("- Speed: 60.0 tokens/s"));
        assert!(markdown.contains("- Error: Generation failed"));

        let json = render_chat_export(&chat, &messages, ChatExportFormat::Json, 2_000).unwrap();
        let value = serde_json::from_str::<serde_json::Value>(&json).unwrap();
        assert_eq!(value["format_version"], 1);
        assert_eq!(value["chat"]["title"], "Export Check");
        assert_eq!(value["messages"][1]["generation_run"]["status"], "failed");

        let text =
            render_chat_export(&chat, &messages, ChatExportFormat::PlainText, 2_000).unwrap();
        assert!(text.contains("Generation: model: llama3.2:3b; status: failed"));
    }

    #[test]
    fn job_service_persists_progress_and_cancellation() {
        let conn = Connection::open_in_memory().unwrap();
        job_repository::create_schema(&conn).unwrap();

        let job = job_service::create(
            &conn,
            JobType::ModelPull,
            "Queued download",
            Some(r#"{"model":"llama3.2:3b"}"#),
            100,
        )
        .unwrap();
        assert_eq!(job.job_type, JobType::ModelPull);
        assert_eq!(job.status, JobStatus::Queued);

        let running = job_service::start(&conn, &job.id, 125).unwrap();
        assert_eq!(running.status, JobStatus::Running);
        assert_eq!(running.started_at, Some(125));

        let progress =
            job_service::update_progress(&conn, &job.id, Some(50), Some(100), Some("Halfway"))
                .unwrap();
        assert_eq!(progress.progress_current, Some(50));
        assert_eq!(progress.progress_total, Some(100));
        assert_eq!(progress.label, "Halfway");

        let cancelling = job_service::request_cancel(&conn, &job.id, 150).unwrap();
        assert_eq!(cancelling.status, JobStatus::Cancelling);
        assert_eq!(cancelling.cancelled_at, Some(150));

        let cancelled = job_service::finish_cancelled(&conn, &job.id, 175).unwrap();
        assert_eq!(cancelled.status, JobStatus::Cancelled);
        assert_eq!(cancelled.completed_at, Some(175));

        let recent_jobs = job_service::list_recent(&conn, 10).unwrap();
        assert_eq!(recent_jobs.len(), 1);
        assert_eq!(recent_jobs[0].id, job.id);

        let interrupted =
            job_service::create(&conn, JobType::ModelPull, "Interrupted download", None, 200)
                .unwrap();
        job_service::start(&conn, &interrupted.id, 225).unwrap();
        let recovered_count = job_repository::mark_interrupted(&conn, 250).unwrap();
        assert_eq!(recovered_count, 1);

        let recovered = job_service::get(&conn, &interrupted.id).unwrap().unwrap();
        assert_eq!(recovered.status, JobStatus::Failed);
        assert_eq!(
            recovered.error_message.as_deref(),
            Some("Job interrupted because Atlas was closed.")
        );
    }

    #[test]
    fn benchmark_service_persists_suite_metrics() {
        let conn = Connection::open_in_memory().unwrap();
        job_repository::create_schema(&conn).unwrap();
        benchmark_repository::create_schema(&conn).unwrap();

        let job = job_service::create(
            &conn,
            JobType::ModelBenchmark,
            "Queued benchmark",
            Some(r#"{"model":"llama3.2:3b"}"#),
            100,
        )
        .unwrap();
        let benchmarks =
            benchmark_service::create_suite(&conn, &job.id, "llama3.2:3b", 100).unwrap();

        assert_eq!(benchmarks.len(), benchmark_service::BENCHMARK_SUITE.len());
        assert!(benchmarks
            .iter()
            .all(|benchmark| benchmark.status == ModelBenchmarkStatus::Queued));

        let running = benchmark_service::mark_running(&conn, &benchmarks[0].id, 125).unwrap();
        assert_eq!(running.status, ModelBenchmarkStatus::Running);
        assert_eq!(running.started_at, Some(125));

        let metrics = CompletedBenchmarkMetrics {
            total_duration_ms: Some(500),
            first_token_ms: Some(120),
            prompt_eval_count: Some(40),
            prompt_eval_duration_ms: Some(80),
            eval_count: Some(100),
            eval_duration_ms: Some(250),
            tokens_per_second: Some(400.0),
        };
        let completed =
            benchmark_service::mark_completed(&conn, &benchmarks[0].id, 700, &metrics).unwrap();

        assert_eq!(completed.status, ModelBenchmarkStatus::Completed);
        assert_eq!(completed.total_duration_ms, Some(500));
        assert_eq!(completed.first_token_ms, Some(120));
        assert_eq!(completed.tokens_per_second, Some(400.0));

        let recent = benchmark_service::list_recent(&conn, 10).unwrap();
        assert_eq!(recent.len(), benchmark_service::BENCHMARK_SUITE.len());
        assert!(recent.iter().any(|benchmark| {
            benchmark.id == completed.id && benchmark.status == ModelBenchmarkStatus::Completed
        }));
    }

    #[test]
    fn sqlite_setup_enables_wal_and_reports_safe_diagnostics() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("atlas-sqlite-test-{unique}.sqlite3"));
        let conn = Connection::open(&db_path).unwrap();

        sqlite::setup_database(&conn).unwrap();
        let diagnostics = sqlite::diagnostics(&conn, &db_path).unwrap();

        assert_eq!(diagnostics.user_version, 3);
        assert_eq!(diagnostics.journal_mode, "wal");
        assert_eq!(diagnostics.integrity_check, "ok");
        assert!(diagnostics
            .table_counts
            .iter()
            .any(|table| table.table_name == "jobs" && table.row_count == 0));
        assert!(diagnostics
            .table_counts
            .iter()
            .any(|table| table.table_name == "message_search" && table.row_count == 0));
        assert!(diagnostics
            .table_counts
            .iter()
            .any(|table| table.table_name == "model_benchmarks" && table.row_count == 0));

        drop(conn);
        let _ = fs::remove_file(&db_path);
        let _ = fs::remove_file(format!("{}-wal", db_path.display()));
        let _ = fs::remove_file(format!("{}-shm", db_path.display()));
    }

    #[test]
    fn fts_search_backfills_highlights_and_cleans_deleted_chats() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;

            CREATE TABLE chats (
              id TEXT PRIMARY KEY,
              title TEXT NOT NULL,
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL
            );

            CREATE TABLE messages (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
              role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
              content TEXT NOT NULL,
              created_at INTEGER NOT NULL
            );

            INSERT INTO chats (id, title, created_at, updated_at)
            VALUES ('chat-1', 'Rust Search Notes', 1704067200000, 1704067200000);

            INSERT INTO messages (chat_id, role, content, created_at)
            VALUES ('chat-1', 'user', 'This migration plan includes FTS snippets.', 1704067200000);
            ",
        )
        .unwrap();

        sqlite::setup_database(&conn).unwrap();

        let message_results = search::search_conversations(&conn, "migration", Some(10)).unwrap();
        let message_result = message_results
            .iter()
            .find(|result| matches!(result.source, SearchResultSource::Message))
            .expect("expected message search result");

        assert_eq!(message_result.chat_id, "chat-1");
        assert_eq!(message_result.message_id, Some(1));
        assert!(message_result
            .snippet
            .iter()
            .any(|part| part.is_match && part.text.eq_ignore_ascii_case("migration")));

        let title_results = search::search_conversations(&conn, "rust", Some(10)).unwrap();
        assert!(title_results.iter().any(|result| matches!(
            result.source,
            SearchResultSource::Title
        ) && result.chat_id == "chat-1"
            && result.message_id.is_none()));

        conn.execute("DELETE FROM chats WHERE id = ?1", params!["chat-1"])
            .unwrap();

        assert!(search::search_conversations(&conn, "migration", Some(10))
            .unwrap()
            .is_empty());
        assert!(search::search_conversations(&conn, "rust", Some(10))
            .unwrap()
            .is_empty());
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
