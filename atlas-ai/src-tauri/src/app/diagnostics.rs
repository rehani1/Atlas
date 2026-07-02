use std::path::Path;

use rusqlite::{params, Connection, Row};

use crate::{
    domain::{
        diagnostics::{
            DiagnosticsCenter, DiagnosticsError, DiagnosticsJob, DiagnosticsJobs,
            DiagnosticsKnowledge, DiagnosticsModelSpeed, DiagnosticsQueue,
        },
        job::{Job, JobStatus, JobType},
        model::{OllamaStatus, OllamaStatusKind},
    },
    infra::{jobs, sqlite},
};

const MAX_RUNNING_JOBS: i64 = 20;
const MAX_FAILED_JOBS: i64 = 10;
const MAX_RECENT_ERRORS: i64 = 12;

pub(crate) struct DiagnosticsInput<'a> {
    pub(crate) app_version: &'a str,
    pub(crate) generated_at: i64,
    pub(crate) database_path: &'a Path,
    pub(crate) ollama: OllamaStatus,
}

pub(crate) fn collect(
    conn: &Connection,
    input: DiagnosticsInput<'_>,
) -> Result<DiagnosticsCenter, String> {
    let database =
        sqlite::diagnostics(conn, input.database_path).map_err(|error| error.to_string())?;
    let running_jobs = jobs::list_active(conn, MAX_RUNNING_JOBS)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(DiagnosticsJob::from)
        .collect::<Vec<_>>();
    let recent_failed_jobs = jobs::list_failed(conn, MAX_FAILED_JOBS)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(DiagnosticsJob::from)
        .collect::<Vec<_>>();
    let embedding_queue = embedding_queue(conn)?;
    let knowledge = knowledge(conn)?;
    let model_speeds = model_speeds(conn)?;
    let recent_errors = recent_errors(conn)?;

    let mut diagnostics = DiagnosticsCenter {
        app_version: input.app_version.to_string(),
        generated_at: input.generated_at,
        ollama: input.ollama,
        database,
        jobs: DiagnosticsJobs {
            running: running_jobs,
            recent_failed: recent_failed_jobs,
            embedding_queue,
        },
        knowledge,
        model_speeds,
        recent_errors,
        copy_summary: String::new(),
    };
    diagnostics.copy_summary = copy_summary(&diagnostics);

    Ok(diagnostics)
}

impl From<Job> for DiagnosticsJob {
    fn from(job: Job) -> Self {
        Self {
            id: job.id,
            job_type: job.job_type,
            status: job.status,
            label: truncate_inline(&job.label, 160),
            progress_current: job.progress_current,
            progress_total: job.progress_total,
            error_message: job
                .error_message
                .as_deref()
                .map(|message| truncate_inline(message, 300)),
            created_at: job.created_at,
            started_at: job.started_at,
            completed_at: job.completed_at,
        }
    }
}

fn embedding_queue(conn: &Connection) -> Result<DiagnosticsQueue, String> {
    Ok(DiagnosticsQueue {
        queued: count_jobs(conn, JobType::EmbeddingIndex, JobStatus::Queued)?,
        running: count_jobs(conn, JobType::EmbeddingIndex, JobStatus::Running)?,
        cancelling: count_jobs(conn, JobType::EmbeddingIndex, JobStatus::Cancelling)?,
        recent_failed: count_jobs(conn, JobType::EmbeddingIndex, JobStatus::Failed)?,
    })
}

fn count_jobs(conn: &Connection, job_type: JobType, status: JobStatus) -> Result<i64, String> {
    conn.query_row(
        "
      SELECT COUNT(*)
      FROM jobs
      WHERE job_type = ?1
        AND status = ?2
      ",
        params![job_type.as_str(), status.as_str()],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

fn knowledge(conn: &Connection) -> Result<DiagnosticsKnowledge, String> {
    conn.query_row(
        "
      SELECT
        (SELECT COUNT(*) FROM workspaces),
        (SELECT COUNT(*) FROM documents WHERE deleted_at IS NULL),
        (SELECT COUNT(*) FROM documents WHERE deleted_at IS NULL AND indexed_at IS NOT NULL),
        (SELECT COUNT(*) FROM documents WHERE deleted_at IS NOT NULL),
        (SELECT COUNT(*) FROM document_chunks),
        (SELECT MAX(indexed_at) FROM documents WHERE deleted_at IS NULL)
      ",
        [],
        |row| {
            Ok(DiagnosticsKnowledge {
                workspace_count: row.get(0)?,
                document_count: row.get(1)?,
                indexed_document_count: row.get(2)?,
                deleted_document_count: row.get(3)?,
                chunk_count: row.get(4)?,
                last_indexed_at: row.get(5)?,
            })
        },
    )
    .map_err(|error| error.to_string())
}

fn model_speeds(conn: &Connection) -> Result<Vec<DiagnosticsModelSpeed>, String> {
    let mut statement = conn
        .prepare(
            "
      WITH generation_speed AS (
        SELECT
          model_name,
          COUNT(*) AS generation_count,
          AVG(tokens_per_second) AS average_tokens_per_second,
          MAX(started_at) AS last_used_at
        FROM generation_runs
        WHERE status = 'completed'
        GROUP BY model_name
      ),
      benchmark_speed AS (
        SELECT
          model_name,
          COUNT(*) AS benchmark_count,
          AVG(tokens_per_second) AS benchmark_average_tokens_per_second
        FROM model_benchmarks
        WHERE status = 'completed'
        GROUP BY model_name
      ),
      combined_models AS (
        SELECT model_name FROM generation_speed
        UNION
        SELECT model_name FROM benchmark_speed
      )
      SELECT
        combined_models.model_name,
        COALESCE(generation_count, 0),
        average_tokens_per_second,
        last_used_at,
        COALESCE(benchmark_count, 0),
        benchmark_average_tokens_per_second
      FROM combined_models
      LEFT JOIN generation_speed
        ON generation_speed.model_name = combined_models.model_name
      LEFT JOIN benchmark_speed
        ON benchmark_speed.model_name = combined_models.model_name
      ORDER BY COALESCE(last_used_at, 0) DESC, combined_models.model_name ASC
      LIMIT 20
      ",
        )
        .map_err(|error| error.to_string())?;

    let speeds = statement
        .query_map([], |row| {
            Ok(DiagnosticsModelSpeed {
                model_name: row.get(0)?,
                generation_count: row.get(1)?,
                average_tokens_per_second: row.get(2)?,
                last_used_at: row.get(3)?,
                benchmark_count: row.get(4)?,
                benchmark_average_tokens_per_second: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(speeds)
}

fn recent_errors(conn: &Connection) -> Result<Vec<DiagnosticsError>, String> {
    let mut statement = conn
        .prepare(
            "
      SELECT source, label, message, occurred_at
      FROM (
        SELECT
          'job' AS source,
          job_type || ': ' || label AS label,
          COALESCE(error_message, 'Job failed without an error message.') AS message,
          COALESCE(completed_at, created_at) AS occurred_at
        FROM jobs
        WHERE status = 'failed'

        UNION ALL

        SELECT
          'generation' AS source,
          model_name AS label,
          COALESCE(error_message, 'Generation failed without an error message.') AS message,
          COALESCE(completed_at, started_at) AS occurred_at
        FROM generation_runs
        WHERE status = 'failed'

        UNION ALL

        SELECT
          'benchmark' AS source,
          model_name || ': ' || prompt_label AS label,
          COALESCE(error_message, 'Benchmark failed without an error message.') AS message,
          COALESCE(completed_at, created_at) AS occurred_at
        FROM model_benchmarks
        WHERE status = 'failed'

        UNION ALL

        SELECT
          'tool_call' AS source,
          tool_name AS label,
          COALESCE(error_message, 'Tool call failed without an error message.') AS message,
          COALESCE(completed_at, created_at) AS occurred_at
        FROM tool_calls
        WHERE status = 'failed'
      )
      ORDER BY occurred_at DESC
      LIMIT ?1
      ",
        )
        .map_err(|error| error.to_string())?;

    let errors = statement
        .query_map(params![MAX_RECENT_ERRORS], read_error)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(errors)
}

fn read_error(row: &Row<'_>) -> Result<DiagnosticsError, rusqlite::Error> {
    Ok(DiagnosticsError {
        source: row.get(0)?,
        label: truncate_inline(&row.get::<_, String>(1)?, 160),
        message: truncate_inline(&row.get::<_, String>(2)?, 300),
        occurred_at: row.get(3)?,
    })
}

fn copy_summary(diagnostics: &DiagnosticsCenter) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Atlas diagnostics ({})", diagnostics.app_version));
    lines.push(format!("Generated at: {}", diagnostics.generated_at));
    lines.push(format!(
        "Ollama: {}",
        ollama_status_label(&diagnostics.ollama.status)
    ));
    lines.push(format!(
        "Installed models: {}",
        diagnostics.ollama.models.len()
    ));
    lines.push(format!(
        "Active model: {}",
        diagnostics
            .ollama
            .selected_model
            .as_deref()
            .unwrap_or("none")
    ));
    lines.push("Database path: redacted".to_string());
    lines.push(format!(
        "Database: {} bytes, WAL {} bytes, schema v{}, integrity {}",
        diagnostics.database.database_size_bytes,
        diagnostics.database.wal_size_bytes,
        diagnostics.database.user_version,
        diagnostics.database.integrity_check
    ));
    lines.push(format!(
        "Jobs: {} running, {} recent failed",
        diagnostics.jobs.running.len(),
        diagnostics.jobs.recent_failed.len()
    ));
    lines.push(format!(
        "Embedding queue: queued {}, running {}, cancelling {}, failed {}",
        diagnostics.jobs.embedding_queue.queued,
        diagnostics.jobs.embedding_queue.running,
        diagnostics.jobs.embedding_queue.cancelling,
        diagnostics.jobs.embedding_queue.recent_failed
    ));
    lines.push(format!(
        "Indexed documents: {} active docs, {} chunks, {} workspaces",
        diagnostics.knowledge.indexed_document_count,
        diagnostics.knowledge.chunk_count,
        diagnostics.knowledge.workspace_count
    ));

    if diagnostics.model_speeds.is_empty() {
        lines.push("Model speed: no completed generation or benchmark metrics".to_string());
    } else {
        lines.push("Model speed:".to_string());
        for speed in &diagnostics.model_speeds {
            let generation_speed = speed
                .average_tokens_per_second
                .map(|value| format!("{value:.1} tok/s"))
                .unwrap_or_else(|| "no chat speed".to_string());
            let benchmark_speed = speed
                .benchmark_average_tokens_per_second
                .map(|value| format!("{value:.1} tok/s benchmark"))
                .unwrap_or_else(|| "no benchmark speed".to_string());
            lines.push(format!(
                "- {}: {}, {}",
                speed.model_name, generation_speed, benchmark_speed
            ));
        }
    }

    lines.push(format!(
        "Recent backend errors: {} present; messages omitted from copied summary",
        diagnostics.recent_errors.len()
    ));
    lines.join("\n")
}

fn ollama_status_label(status: &OllamaStatusKind) -> &'static str {
    match status {
        OllamaStatusKind::Unavailable => "unavailable",
        OllamaStatusKind::RunningWithModels => "running_with_models",
        OllamaStatusKind::RunningWithoutModels => "running_without_models",
        OllamaStatusKind::SelectedModelMissing => "selected_model_missing",
    }
}

fn truncate_inline(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }

    let mut truncated = normalized
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}
