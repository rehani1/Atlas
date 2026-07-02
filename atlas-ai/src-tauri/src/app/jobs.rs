use rusqlite::Connection;

use crate::{
    domain::job::{Job, JobStatus, JobType},
    infra::jobs,
};

pub(crate) fn create(
    conn: &Connection,
    job_type: JobType,
    label: &str,
    payload_json: Option<&str>,
    now: i64,
) -> Result<Job, String> {
    jobs::insert(conn, job_type, label, payload_json, now).map_err(|error| error.to_string())
}

pub(crate) fn get(conn: &Connection, job_id: &str) -> Result<Option<Job>, String> {
    jobs::get(conn, job_id).map_err(|error| error.to_string())
}

pub(crate) fn list_recent(conn: &Connection, limit: i64) -> Result<Vec<Job>, String> {
    jobs::list_recent(conn, limit).map_err(|error| error.to_string())
}

pub(crate) fn start(conn: &Connection, job_id: &str, now: i64) -> Result<Job, String> {
    jobs::mark_running(conn, job_id, now).map_err(|error| error.to_string())
}

pub(crate) fn update_progress(
    conn: &Connection,
    job_id: &str,
    progress_current: Option<i64>,
    progress_total: Option<i64>,
    label: Option<&str>,
) -> Result<Job, String> {
    jobs::update_progress(conn, job_id, progress_current, progress_total, label)
        .map_err(|error| error.to_string())
}

pub(crate) fn request_cancel(conn: &Connection, job_id: &str, now: i64) -> Result<Job, String> {
    jobs::mark_cancelling(conn, job_id, now).map_err(|error| error.to_string())
}

pub(crate) fn finish_cancelled(conn: &Connection, job_id: &str, now: i64) -> Result<Job, String> {
    jobs::mark_cancelled(conn, job_id, now).map_err(|error| error.to_string())
}

pub(crate) fn finish_succeeded(
    conn: &Connection,
    job_id: &str,
    result_json: Option<&str>,
    now: i64,
) -> Result<Job, String> {
    jobs::mark_succeeded(conn, job_id, result_json, now).map_err(|error| error.to_string())
}

pub(crate) fn finish_failed(
    conn: &Connection,
    job_id: &str,
    error_message: &str,
    now: i64,
) -> Result<Job, String> {
    jobs::mark_failed(conn, job_id, error_message, now).map_err(|error| error.to_string())
}

pub(crate) fn is_terminal(status: JobStatus) -> bool {
    matches!(
        status,
        JobStatus::Cancelled | JobStatus::Succeeded | JobStatus::Failed
    )
}
