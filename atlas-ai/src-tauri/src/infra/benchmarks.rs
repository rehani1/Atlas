use rusqlite::{params, Connection, Row};

use crate::domain::benchmark::{ModelBenchmark, ModelBenchmarkStatus};

pub(crate) struct CompletedBenchmarkMetrics {
    pub(crate) total_duration_ms: Option<i64>,
    pub(crate) first_token_ms: Option<i64>,
    pub(crate) prompt_eval_count: Option<i64>,
    pub(crate) prompt_eval_duration_ms: Option<i64>,
    pub(crate) eval_count: Option<i64>,
    pub(crate) eval_duration_ms: Option<i64>,
    pub(crate) tokens_per_second: Option<f64>,
}

pub(crate) fn create_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
      CREATE TABLE IF NOT EXISTS model_benchmarks (
        id TEXT PRIMARY KEY,
        job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
        model_name TEXT NOT NULL,
        prompt_type TEXT NOT NULL,
        prompt_label TEXT NOT NULL,
        prompt_text_hash TEXT NOT NULL,
        started_at INTEGER,
        completed_at INTEGER,
        status TEXT NOT NULL CHECK(status IN (
          'queued',
          'running',
          'completed',
          'cancelled',
          'failed'
        )),
        total_duration_ms INTEGER,
        first_token_ms INTEGER,
        prompt_eval_count INTEGER,
        prompt_eval_duration_ms INTEGER,
        eval_count INTEGER,
        eval_duration_ms INTEGER,
        tokens_per_second REAL,
        error_message TEXT,
        created_at INTEGER NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_model_benchmarks_model_completed
        ON model_benchmarks(model_name, completed_at DESC);

      CREATE INDEX IF NOT EXISTS idx_model_benchmarks_job
        ON model_benchmarks(job_id);

      CREATE INDEX IF NOT EXISTS idx_model_benchmarks_created
        ON model_benchmarks(created_at DESC);
      ",
    )
}

fn create_id(conn: &Connection) -> Result<String, rusqlite::Error> {
    conn.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
}

fn read_benchmark(row: &Row<'_>) -> Result<ModelBenchmark, rusqlite::Error> {
    let status = row
        .get::<_, String>(7)
        .and_then(|value| ModelBenchmarkStatus::from_str(&value).map_err(to_from_sql_error))?;

    Ok(ModelBenchmark {
        id: row.get(0)?,
        job_id: row.get(1)?,
        model_name: row.get(2)?,
        prompt_type: row.get(3)?,
        prompt_label: row.get(4)?,
        prompt_text_hash: row.get(5)?,
        started_at: row.get(6)?,
        completed_at: row.get(8)?,
        status,
        total_duration_ms: row.get(9)?,
        first_token_ms: row.get(10)?,
        prompt_eval_count: row.get(11)?,
        prompt_eval_duration_ms: row.get(12)?,
        eval_count: row.get(13)?,
        eval_duration_ms: row.get(14)?,
        tokens_per_second: row.get(15)?,
        error_message: row.get(16)?,
        created_at: row.get(17)?,
    })
}

fn to_from_sql_error(error: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
    )
}

pub(crate) fn insert_suite(
    conn: &Connection,
    job_id: &str,
    model_name: &str,
    prompts: &[(&str, &str, String)],
    created_at: i64,
) -> Result<Vec<ModelBenchmark>, rusqlite::Error> {
    let mut benchmark_ids = Vec::with_capacity(prompts.len());

    for (prompt_type, prompt_label, prompt_text_hash) in prompts {
        let id = create_id(conn)?;
        benchmark_ids.push(id.clone());
        conn.execute(
            "
        INSERT INTO model_benchmarks (
          id,
          job_id,
          model_name,
          prompt_type,
          prompt_label,
          prompt_text_hash,
          status,
          created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ",
            params![
                id,
                job_id,
                model_name,
                prompt_type,
                prompt_label,
                prompt_text_hash,
                ModelBenchmarkStatus::Queued.as_str(),
                created_at
            ],
        )?;
    }

    benchmark_ids
        .into_iter()
        .map(|benchmark_id| get(conn, &benchmark_id))
        .collect()
}

pub(crate) fn list_recent(
    conn: &Connection,
    limit: i64,
) -> Result<Vec<ModelBenchmark>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "
      SELECT
        id,
        job_id,
        model_name,
        prompt_type,
        prompt_label,
        prompt_text_hash,
        started_at,
        status,
        completed_at,
        total_duration_ms,
        first_token_ms,
        prompt_eval_count,
        prompt_eval_duration_ms,
        eval_count,
        eval_duration_ms,
        tokens_per_second,
        error_message,
        created_at
      FROM model_benchmarks
      ORDER BY created_at DESC, prompt_type ASC
      LIMIT ?1
      ",
    )?;

    let benchmarks = statement
        .query_map(params![limit.clamp(1, 250)], read_benchmark)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(benchmarks)
}

pub(crate) fn mark_running(
    conn: &Connection,
    benchmark_id: &str,
    started_at: i64,
) -> Result<ModelBenchmark, rusqlite::Error> {
    conn.execute(
        "
      UPDATE model_benchmarks
      SET status = ?2,
          started_at = COALESCE(started_at, ?3)
      WHERE id = ?1
      ",
        params![
            benchmark_id,
            ModelBenchmarkStatus::Running.as_str(),
            started_at
        ],
    )?;

    get(conn, benchmark_id)
}

pub(crate) fn mark_completed(
    conn: &Connection,
    benchmark_id: &str,
    completed_at: i64,
    metrics: &CompletedBenchmarkMetrics,
) -> Result<ModelBenchmark, rusqlite::Error> {
    conn.execute(
        "
      UPDATE model_benchmarks
      SET status = ?2,
          completed_at = ?3,
          total_duration_ms = ?4,
          first_token_ms = ?5,
          prompt_eval_count = ?6,
          prompt_eval_duration_ms = ?7,
          eval_count = ?8,
          eval_duration_ms = ?9,
          tokens_per_second = ?10,
          error_message = NULL
      WHERE id = ?1
      ",
        params![
            benchmark_id,
            ModelBenchmarkStatus::Completed.as_str(),
            completed_at,
            metrics.total_duration_ms,
            metrics.first_token_ms,
            metrics.prompt_eval_count,
            metrics.prompt_eval_duration_ms,
            metrics.eval_count,
            metrics.eval_duration_ms,
            metrics.tokens_per_second,
        ],
    )?;

    get(conn, benchmark_id)
}

pub(crate) fn mark_cancelled(
    conn: &Connection,
    benchmark_id: &str,
    completed_at: i64,
) -> Result<ModelBenchmark, rusqlite::Error> {
    conn.execute(
        "
      UPDATE model_benchmarks
      SET status = ?2,
          completed_at = COALESCE(completed_at, ?3),
          error_message = COALESCE(error_message, 'Benchmark cancelled')
      WHERE id = ?1
      ",
        params![
            benchmark_id,
            ModelBenchmarkStatus::Cancelled.as_str(),
            completed_at
        ],
    )?;

    get(conn, benchmark_id)
}

pub(crate) fn mark_failed(
    conn: &Connection,
    benchmark_id: &str,
    completed_at: i64,
    error_message: &str,
) -> Result<ModelBenchmark, rusqlite::Error> {
    conn.execute(
        "
      UPDATE model_benchmarks
      SET status = ?2,
          completed_at = COALESCE(completed_at, ?3),
          error_message = ?4
      WHERE id = ?1
      ",
        params![
            benchmark_id,
            ModelBenchmarkStatus::Failed.as_str(),
            completed_at,
            error_message
        ],
    )?;

    get(conn, benchmark_id)
}

pub(crate) fn mark_remaining_cancelled(
    conn: &Connection,
    job_id: &str,
    completed_at: i64,
) -> Result<usize, rusqlite::Error> {
    conn.execute(
        "
      UPDATE model_benchmarks
      SET status = ?2,
          completed_at = COALESCE(completed_at, ?3),
          error_message = COALESCE(error_message, 'Benchmark cancelled')
      WHERE job_id = ?1
        AND status IN ('queued', 'running')
      ",
        params![
            job_id,
            ModelBenchmarkStatus::Cancelled.as_str(),
            completed_at
        ],
    )
}

pub(crate) fn mark_remaining_failed(
    conn: &Connection,
    job_id: &str,
    completed_at: i64,
    error_message: &str,
) -> Result<usize, rusqlite::Error> {
    conn.execute(
        "
      UPDATE model_benchmarks
      SET status = ?2,
          completed_at = COALESCE(completed_at, ?3),
          error_message = COALESCE(error_message, ?4)
      WHERE job_id = ?1
        AND status IN ('queued', 'running')
      ",
        params![
            job_id,
            ModelBenchmarkStatus::Failed.as_str(),
            completed_at,
            error_message
        ],
    )
}

fn get(conn: &Connection, benchmark_id: &str) -> Result<ModelBenchmark, rusqlite::Error> {
    conn.query_row(
        "
      SELECT
        id,
        job_id,
        model_name,
        prompt_type,
        prompt_label,
        prompt_text_hash,
        started_at,
        status,
        completed_at,
        total_duration_ms,
        first_token_ms,
        prompt_eval_count,
        prompt_eval_duration_ms,
        eval_count,
        eval_duration_ms,
        tokens_per_second,
        error_message,
        created_at
      FROM model_benchmarks
      WHERE id = ?1
      ",
        params![benchmark_id],
        read_benchmark,
    )
}
