use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::domain::job::{Job, JobStatus, JobType};

pub(crate) fn create_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
      CREATE TABLE IF NOT EXISTS jobs (
        id TEXT PRIMARY KEY,
        job_type TEXT NOT NULL CHECK(job_type IN (
          'chat_generation',
          'model_pull',
          'model_delete',
          'export_conversation',
          'document_import',
          'embedding_index',
          'model_benchmark',
          'conversation_summary'
        )),
        status TEXT NOT NULL CHECK(status IN (
          'queued',
          'running',
          'cancelling',
          'cancelled',
          'succeeded',
          'failed'
        )),
        progress_current INTEGER,
        progress_total INTEGER,
        label TEXT NOT NULL,
        payload_json TEXT,
        result_json TEXT,
        error_message TEXT,
        created_at INTEGER NOT NULL,
        started_at INTEGER,
        completed_at INTEGER,
        cancelled_at INTEGER
      );

      CREATE INDEX IF NOT EXISTS idx_jobs_created_at
        ON jobs(created_at DESC);

      CREATE INDEX IF NOT EXISTS idx_jobs_status_created_at
        ON jobs(status, created_at DESC);
      ",
    )
}

fn create_id(conn: &Connection) -> Result<String, rusqlite::Error> {
    conn.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
}

fn read_job(row: &Row<'_>) -> Result<Job, rusqlite::Error> {
    let job_type = row
        .get::<_, String>(1)
        .and_then(|value| JobType::from_str(&value).map_err(to_from_sql_error))?;
    let status = row
        .get::<_, String>(2)
        .and_then(|value| JobStatus::from_str(&value).map_err(to_from_sql_error))?;

    Ok(Job {
        id: row.get(0)?,
        job_type,
        status,
        progress_current: row.get(3)?,
        progress_total: row.get(4)?,
        label: row.get(5)?,
        payload_json: row.get(6)?,
        result_json: row.get(7)?,
        error_message: row.get(8)?,
        created_at: row.get(9)?,
        started_at: row.get(10)?,
        completed_at: row.get(11)?,
        cancelled_at: row.get(12)?,
    })
}

fn to_from_sql_error(error: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
    )
}

pub(crate) fn insert(
    conn: &Connection,
    job_type: JobType,
    label: &str,
    payload_json: Option<&str>,
    created_at: i64,
) -> Result<Job, rusqlite::Error> {
    let id = create_id(conn)?;
    conn.execute(
        "
      INSERT INTO jobs (
        id,
        job_type,
        status,
        label,
        payload_json,
        created_at
      )
      VALUES (?1, ?2, ?3, ?4, ?5, ?6)
      ",
        params![
            id,
            job_type.as_str(),
            JobStatus::Queued.as_str(),
            label,
            payload_json,
            created_at,
        ],
    )?;

    get(conn, &id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn get(conn: &Connection, job_id: &str) -> Result<Option<Job>, rusqlite::Error> {
    conn.query_row(
        "
      SELECT
        id,
        job_type,
        status,
        progress_current,
        progress_total,
        label,
        payload_json,
        result_json,
        error_message,
        created_at,
        started_at,
        completed_at,
        cancelled_at
      FROM jobs
      WHERE id = ?1
      ",
        params![job_id],
        read_job,
    )
    .optional()
}

pub(crate) fn list_recent(conn: &Connection, limit: i64) -> Result<Vec<Job>, rusqlite::Error> {
    let limit = limit.clamp(1, 50);
    let mut statement = conn.prepare(
        "
      SELECT
        id,
        job_type,
        status,
        progress_current,
        progress_total,
        label,
        payload_json,
        result_json,
        error_message,
        created_at,
        started_at,
        completed_at,
        cancelled_at
      FROM jobs
      ORDER BY created_at DESC
      LIMIT ?1
      ",
    )?;

    let jobs = statement
        .query_map(params![limit], read_job)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(jobs)
}

pub(crate) fn list_active(conn: &Connection, limit: i64) -> Result<Vec<Job>, rusqlite::Error> {
    let limit = limit.clamp(1, 50);
    let mut statement = conn.prepare(
        "
      SELECT
        id,
        job_type,
        status,
        progress_current,
        progress_total,
        label,
        payload_json,
        result_json,
        error_message,
        created_at,
        started_at,
        completed_at,
        cancelled_at
      FROM jobs
      WHERE status IN ('queued', 'running', 'cancelling')
      ORDER BY created_at DESC
      LIMIT ?1
      ",
    )?;

    let jobs = statement
        .query_map(params![limit], read_job)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(jobs)
}

pub(crate) fn list_failed(conn: &Connection, limit: i64) -> Result<Vec<Job>, rusqlite::Error> {
    let limit = limit.clamp(1, 50);
    let mut statement = conn.prepare(
        "
      SELECT
        id,
        job_type,
        status,
        progress_current,
        progress_total,
        label,
        payload_json,
        result_json,
        error_message,
        created_at,
        started_at,
        completed_at,
        cancelled_at
      FROM jobs
      WHERE status = 'failed'
      ORDER BY COALESCE(completed_at, created_at) DESC
      LIMIT ?1
      ",
    )?;

    let jobs = statement
        .query_map(params![limit], read_job)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(jobs)
}

pub(crate) fn mark_interrupted(conn: &Connection, now: i64) -> Result<usize, rusqlite::Error> {
    conn.execute(
        "
      UPDATE jobs
      SET status = ?1,
          error_message = COALESCE(error_message, 'Job interrupted because Atlas was closed.'),
          completed_at = COALESCE(completed_at, ?2)
      WHERE status IN ('queued', 'running', 'cancelling')
      ",
        params![JobStatus::Failed.as_str(), now],
    )
}

pub(crate) fn mark_running(
    conn: &Connection,
    job_id: &str,
    started_at: i64,
) -> Result<Job, rusqlite::Error> {
    conn.execute(
        "
      UPDATE jobs
      SET status = ?2,
          started_at = COALESCE(started_at, ?3)
      WHERE id = ?1
      ",
        params![job_id, JobStatus::Running.as_str(), started_at],
    )?;

    get(conn, job_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn update_progress(
    conn: &Connection,
    job_id: &str,
    progress_current: Option<i64>,
    progress_total: Option<i64>,
    label: Option<&str>,
) -> Result<Job, rusqlite::Error> {
    conn.execute(
        "
      UPDATE jobs
      SET progress_current = ?2,
          progress_total = ?3,
          label = COALESCE(?4, label)
      WHERE id = ?1
      ",
        params![job_id, progress_current, progress_total, label],
    )?;

    get(conn, job_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn mark_cancelling(
    conn: &Connection,
    job_id: &str,
    cancelled_at: i64,
) -> Result<Job, rusqlite::Error> {
    conn.execute(
        "
      UPDATE jobs
      SET status = ?2,
          cancelled_at = COALESCE(cancelled_at, ?3)
      WHERE id = ?1
        AND status IN ('queued', 'running')
      ",
        params![job_id, JobStatus::Cancelling.as_str(), cancelled_at],
    )?;

    get(conn, job_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn mark_cancelled(
    conn: &Connection,
    job_id: &str,
    completed_at: i64,
) -> Result<Job, rusqlite::Error> {
    conn.execute(
        "
      UPDATE jobs
      SET status = ?2,
          completed_at = ?3,
          cancelled_at = COALESCE(cancelled_at, ?3)
      WHERE id = ?1
      ",
        params![job_id, JobStatus::Cancelled.as_str(), completed_at],
    )?;

    get(conn, job_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn mark_succeeded(
    conn: &Connection,
    job_id: &str,
    result_json: Option<&str>,
    completed_at: i64,
) -> Result<Job, rusqlite::Error> {
    conn.execute(
        "
      UPDATE jobs
      SET status = ?2,
          progress_current = COALESCE(progress_total, progress_current),
          result_json = ?3,
          completed_at = ?4
      WHERE id = ?1
      ",
        params![
            job_id,
            JobStatus::Succeeded.as_str(),
            result_json,
            completed_at
        ],
    )?;

    get(conn, job_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub(crate) fn mark_failed(
    conn: &Connection,
    job_id: &str,
    error_message: &str,
    completed_at: i64,
) -> Result<Job, rusqlite::Error> {
    conn.execute(
        "
      UPDATE jobs
      SET status = ?2,
          error_message = ?3,
          completed_at = ?4
      WHERE id = ?1
      ",
        params![
            job_id,
            JobStatus::Failed.as_str(),
            error_message,
            completed_at
        ],
    )?;

    get(conn, job_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}
