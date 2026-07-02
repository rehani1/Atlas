use rusqlite::{params, Connection, Row};

use crate::domain::context::{
    GenerationContextItem, GenerationContextItemDraft, GenerationContextItemType,
};

pub(crate) fn create_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
      CREATE TABLE IF NOT EXISTS generation_context_items (
        id TEXT PRIMARY KEY,
        generation_run_id TEXT NOT NULL REFERENCES generation_runs(id) ON DELETE CASCADE,
        item_type TEXT NOT NULL CHECK(item_type IN (
          'system_prompt',
          'summary',
          'memory',
          'prior_message',
          'document_chunk',
          'user_message',
          'model_options',
          'truncation_notice'
        )),
        item_id TEXT,
        label TEXT NOT NULL,
        token_count_estimate INTEGER NOT NULL,
        order_index INTEGER NOT NULL,
        metadata_json TEXT,
        created_at INTEGER NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_generation_context_items_run
        ON generation_context_items(generation_run_id, order_index ASC);
      ",
    )
}

fn create_id(conn: &Connection) -> Result<String, rusqlite::Error> {
    conn.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
}

fn to_from_sql_error(error: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
    )
}

fn read_context_item(row: &Row<'_>) -> Result<GenerationContextItem, rusqlite::Error> {
    let item_type = row
        .get::<_, String>(2)
        .and_then(|value| GenerationContextItemType::from_str(&value).map_err(to_from_sql_error))?;

    Ok(GenerationContextItem {
        id: row.get(0)?,
        generation_run_id: row.get(1)?,
        item_type,
        item_id: row.get(3)?,
        label: row.get(4)?,
        token_count_estimate: row.get(5)?,
        order_index: row.get(6)?,
        metadata_json: row.get(7)?,
        created_at: row.get(8)?,
    })
}

pub(crate) fn insert_generation_context_items(
    conn: &Connection,
    generation_run_id: &str,
    items: &[GenerationContextItemDraft],
    created_at: i64,
) -> Result<Vec<GenerationContextItem>, rusqlite::Error> {
    conn.execute(
        "DELETE FROM generation_context_items WHERE generation_run_id = ?1",
        params![generation_run_id],
    )?;

    let mut item_ids = Vec::with_capacity(items.len());
    for item in items {
        let id = create_id(conn)?;
        item_ids.push(id.clone());
        conn.execute(
            "
        INSERT INTO generation_context_items (
          id,
          generation_run_id,
          item_type,
          item_id,
          label,
          token_count_estimate,
          order_index,
          metadata_json,
          created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ",
            params![
                id,
                generation_run_id,
                item.item_type.as_str(),
                item.item_id,
                item.label,
                item.token_count_estimate,
                item.order_index,
                item.metadata_json,
                created_at,
            ],
        )?;
    }

    item_ids
        .into_iter()
        .map(|item_id| get_context_item(conn, &item_id))
        .collect()
}

fn get_context_item(
    conn: &Connection,
    item_id: &str,
) -> Result<GenerationContextItem, rusqlite::Error> {
    conn.query_row(
        "
      SELECT
        id,
        generation_run_id,
        item_type,
        item_id,
        label,
        token_count_estimate,
        order_index,
        metadata_json,
        created_at
      FROM generation_context_items
      WHERE id = ?1
      ",
        params![item_id],
        read_context_item,
    )
}

pub(crate) fn list_generation_context_items(
    conn: &Connection,
    generation_run_id: &str,
) -> Result<Vec<GenerationContextItem>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "
      SELECT
        id,
        generation_run_id,
        item_type,
        item_id,
        label,
        token_count_estimate,
        order_index,
        metadata_json,
        created_at
      FROM generation_context_items
      WHERE generation_run_id = ?1
      ORDER BY order_index ASC, id ASC
      ",
    )?;

    let items = statement
        .query_map(params![generation_run_id], read_context_item)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(items)
}
