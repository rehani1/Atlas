use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct DatabaseTableCount {
    pub(crate) table_name: String,
    pub(crate) row_count: i64,
}

#[derive(Serialize)]
pub(crate) struct DatabaseDiagnostics {
    pub(crate) path: String,
    pub(crate) database_size_bytes: i64,
    pub(crate) wal_size_bytes: i64,
    pub(crate) shm_size_bytes: i64,
    pub(crate) journal_mode: String,
    pub(crate) user_version: i64,
    pub(crate) page_count: i64,
    pub(crate) page_size: i64,
    pub(crate) freelist_count: i64,
    pub(crate) integrity_check: String,
    pub(crate) table_counts: Vec<DatabaseTableCount>,
}
