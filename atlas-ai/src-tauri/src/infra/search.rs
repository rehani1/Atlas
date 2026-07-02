use rusqlite::{params, Connection, Row};

use crate::domain::search::{ChatSearchResult, SearchResultSource, SearchSnippetPart};

const MATCH_START: &str = "\u{001f}";
const MATCH_END: &str = "\u{001e}";
const SNIPPET_ELLIPSIS: &str = " ... ";
const DEFAULT_LIMIT: i64 = 30;
const MAX_LIMIT: i64 = 100;

#[derive(Default)]
struct ParsedSearchQuery {
    terms: Vec<String>,
    model_filter: Option<String>,
    chat_filter: Option<String>,
    from_ms: Option<i64>,
    before_ms: Option<i64>,
    has_code: bool,
}

struct SearchExecutionFilters<'a> {
    model_filter: Option<&'a str>,
    from_ms: Option<i64>,
    before_ms: Option<i64>,
    has_code: bool,
    chat_pattern: Option<&'a str>,
    limit: i64,
}

pub(crate) fn create_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
      CREATE VIRTUAL TABLE IF NOT EXISTS chat_search
      USING fts5(
        title,
        chat_id UNINDEXED,
        created_at UNINDEXED,
        updated_at UNINDEXED,
        tokenize = 'unicode61'
      );

      CREATE VIRTUAL TABLE IF NOT EXISTS message_search
      USING fts5(
        content,
        chat_id UNINDEXED,
        message_id UNINDEXED,
        role UNINDEXED,
        created_at UNINDEXED,
        tokenize = 'unicode61'
      );

      CREATE TRIGGER IF NOT EXISTS chats_after_insert_search
      AFTER INSERT ON chats
      BEGIN
        INSERT OR REPLACE INTO chat_search (
          rowid,
          title,
          chat_id,
          created_at,
          updated_at
        )
        VALUES (
          NEW.rowid,
          NEW.title,
          NEW.id,
          NEW.created_at,
          NEW.updated_at
        );
      END;

      CREATE TRIGGER IF NOT EXISTS chats_after_update_search
      AFTER UPDATE OF title, updated_at ON chats
      BEGIN
        INSERT OR REPLACE INTO chat_search (
          rowid,
          title,
          chat_id,
          created_at,
          updated_at
        )
        VALUES (
          NEW.rowid,
          NEW.title,
          NEW.id,
          NEW.created_at,
          NEW.updated_at
        );
      END;

      CREATE TRIGGER IF NOT EXISTS chats_after_delete_search
      AFTER DELETE ON chats
      BEGIN
        DELETE FROM chat_search WHERE rowid = OLD.rowid;
        DELETE FROM message_search WHERE chat_id = OLD.id;
      END;

      CREATE TRIGGER IF NOT EXISTS messages_after_insert_search
      AFTER INSERT ON messages
      BEGIN
        INSERT OR REPLACE INTO message_search (
          rowid,
          content,
          chat_id,
          message_id,
          role,
          created_at
        )
        VALUES (
          NEW.id,
          NEW.content,
          NEW.chat_id,
          NEW.id,
          NEW.role,
          NEW.created_at
        );
      END;

      CREATE TRIGGER IF NOT EXISTS messages_after_update_search
      AFTER UPDATE OF chat_id, role, content, created_at ON messages
      BEGIN
        DELETE FROM message_search WHERE rowid = OLD.id;
        INSERT OR REPLACE INTO message_search (
          rowid,
          content,
          chat_id,
          message_id,
          role,
          created_at
        )
        VALUES (
          NEW.id,
          NEW.content,
          NEW.chat_id,
          NEW.id,
          NEW.role,
          NEW.created_at
        );
      END;

      CREATE TRIGGER IF NOT EXISTS messages_after_delete_search
      AFTER DELETE ON messages
      BEGIN
        DELETE FROM message_search WHERE rowid = OLD.id;
      END;
      ",
    )?;

    backfill(conn)
}

fn backfill(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
      DELETE FROM chat_search
      WHERE chat_id NOT IN (SELECT id FROM chats);

      DELETE FROM message_search
      WHERE message_id NOT IN (SELECT id FROM messages)
         OR chat_id NOT IN (SELECT id FROM chats);

      INSERT OR REPLACE INTO chat_search (
        rowid,
        title,
        chat_id,
        created_at,
        updated_at
      )
      SELECT
        c.rowid,
        c.title,
        c.id,
        c.created_at,
        c.updated_at
      FROM chats c;

      INSERT OR REPLACE INTO message_search (
        rowid,
        content,
        chat_id,
        message_id,
        role,
        created_at
      )
      SELECT
        m.id,
        m.content,
        m.chat_id,
        m.id,
        m.role,
        m.created_at
      FROM messages m
      JOIN chats c ON c.id = m.chat_id;
      ",
    )
}

pub(crate) fn search_conversations(
    conn: &Connection,
    query: &str,
    limit: Option<i64>,
) -> Result<Vec<ChatSearchResult>, String> {
    let parsed = parse_search_query(query)?;
    let fts_query = build_fts_query(&parsed);

    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let chat_pattern = parsed
        .chat_filter
        .as_ref()
        .map(|filter| format!("%{}%", escape_like_pattern(&filter.to_lowercase())));
    let filters = SearchExecutionFilters {
        model_filter: parsed.model_filter.as_deref(),
        from_ms: parsed.from_ms,
        before_ms: parsed.before_ms,
        has_code: parsed.has_code,
        chat_pattern: chat_pattern.as_deref(),
        limit,
    };

    let mut results = Vec::new();

    if parsed.model_filter.is_none() && !parsed.has_code {
        results.extend(search_title_results(conn, &fts_query, &filters)?);
    }

    results.extend(search_message_results(conn, &fts_query, &filters)?);

    results.sort_by(|left, right| {
        left.score
            .partial_cmp(&right.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| left.chat_title.cmp(&right.chat_title))
    });
    results.truncate(limit as usize);

    Ok(results)
}

fn search_title_results(
    conn: &Connection,
    fts_query: &str,
    filters: &SearchExecutionFilters<'_>,
) -> Result<Vec<ChatSearchResult>, String> {
    let mut statement = conn
        .prepare(
            "
      SELECT
        c.id,
        c.title,
        c.created_at,
        c.updated_at,
        (
          SELECT COUNT(*)
          FROM messages count_m
          WHERE count_m.chat_id = c.id
        ) AS message_count,
        snippet(chat_search, 0, ?2, ?3, ?4, 12) AS snippet,
        bm25(chat_search) AS score
      FROM chat_search
      JOIN chats c ON c.id = chat_search.chat_id
      WHERE chat_search MATCH ?1
        AND (?5 IS NULL OR c.updated_at >= ?5)
        AND (?6 IS NULL OR c.updated_at < ?6)
        AND (?7 IS NULL OR LOWER(c.title) LIKE ?7 ESCAPE '\\')
      ORDER BY score ASC, c.updated_at DESC
      LIMIT ?8
      ",
        )
        .map_err(|error| error.to_string())?;

    let results = statement
        .query_map(
            params![
                fts_query,
                MATCH_START,
                MATCH_END,
                SNIPPET_ELLIPSIS,
                filters.from_ms,
                filters.before_ms,
                filters.chat_pattern,
                filters.limit
            ],
            read_title_result,
        )
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(results)
}

fn search_message_results(
    conn: &Connection,
    fts_query: &str,
    filters: &SearchExecutionFilters<'_>,
) -> Result<Vec<ChatSearchResult>, String> {
    let mut statement = conn
        .prepare(
            "
      SELECT
        c.id,
        c.title,
        message_search.message_id,
        message_search.role,
        m.created_at,
        c.updated_at,
        (
          SELECT COUNT(*)
          FROM messages count_m
          WHERE count_m.chat_id = c.id
        ) AS message_count,
        snippet(message_search, 0, ?2, ?3, ?4, 18) AS snippet,
        bm25(message_search) AS score
      FROM message_search
      JOIN messages m ON m.id = message_search.message_id
      JOIN chats c ON c.id = message_search.chat_id
      LEFT JOIN generation_runs g ON g.message_id = m.id
      WHERE message_search MATCH ?1
        AND (?5 IS NULL OR g.model_name = ?5)
        AND (?6 IS NULL OR m.created_at >= ?6)
        AND (?7 IS NULL OR m.created_at < ?7)
        AND (?8 = 0 OR m.content LIKE '%```%')
        AND (?9 IS NULL OR LOWER(c.title) LIKE ?9 ESCAPE '\\')
      ORDER BY score ASC, c.updated_at DESC
      LIMIT ?10
      ",
        )
        .map_err(|error| error.to_string())?;

    let results = statement
        .query_map(
            params![
                fts_query,
                MATCH_START,
                MATCH_END,
                SNIPPET_ELLIPSIS,
                filters.model_filter,
                filters.from_ms,
                filters.before_ms,
                if filters.has_code { 1 } else { 0 },
                filters.chat_pattern,
                filters.limit
            ],
            read_message_result,
        )
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(results)
}

fn read_title_result(row: &Row<'_>) -> Result<ChatSearchResult, rusqlite::Error> {
    let snippet: String = row.get(5)?;

    Ok(ChatSearchResult {
        chat_id: row.get(0)?,
        chat_title: row.get(1)?,
        message_id: None,
        role: None,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        message_count: row.get(4)?,
        source: SearchResultSource::Title,
        score: row.get(6)?,
        snippet: parse_snippet_parts(&snippet),
    })
}

fn read_message_result(row: &Row<'_>) -> Result<ChatSearchResult, rusqlite::Error> {
    let snippet: String = row.get(7)?;

    Ok(ChatSearchResult {
        chat_id: row.get(0)?,
        chat_title: row.get(1)?,
        message_id: row.get(2)?,
        role: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        message_count: row.get(6)?,
        source: SearchResultSource::Message,
        score: row.get(8)?,
        snippet: parse_snippet_parts(&snippet),
    })
}

fn parse_search_query(query: &str) -> Result<ParsedSearchQuery, String> {
    let mut parsed = ParsedSearchQuery::default();

    for token in query.split_whitespace() {
        if let Some(value) = token.strip_prefix("model:") {
            if !value.is_empty() {
                parsed.model_filter = Some(value.to_string());
            }
            continue;
        }

        if let Some(value) = token.strip_prefix("chat:") {
            if !value.is_empty() {
                parsed.chat_filter = Some(value.to_string());
            }
            continue;
        }

        if let Some(value) = token.strip_prefix("from:") {
            parsed.from_ms = Some(parse_date_start_ms(value, "from")?);
            continue;
        }

        if let Some(value) = token.strip_prefix("before:") {
            parsed.before_ms = Some(parse_date_start_ms(value, "before")?);
            continue;
        }

        if let Some(value) = token.strip_prefix("has:") {
            if value == "code" {
                parsed.has_code = true;
                continue;
            }
        }

        parsed.terms.extend(tokenize_search_text(token));
    }

    if parsed.terms.is_empty() {
        if let Some(chat_filter) = &parsed.chat_filter {
            parsed.terms = tokenize_search_text(chat_filter);
        }
    }

    Ok(parsed)
}

fn build_fts_query(parsed: &ParsedSearchQuery) -> String {
    parsed
        .terms
        .iter()
        .map(|term| format!("\"{term}\"*"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokenize_search_text(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();

    for character in text.chars() {
        if character.is_alphanumeric() || character == '_' {
            current.push(character.to_ascii_lowercase());
        } else if !current.is_empty() {
            terms.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        terms.push(current);
    }

    terms
}

fn parse_date_start_ms(value: &str, filter_name: &str) -> Result<i64, String> {
    let [year, month, day] = parse_date_parts(value)
        .ok_or_else(|| format!("Invalid {filter_name}: date. Use {filter_name}:YYYY-MM-DD."))?;

    if !(1..=12).contains(&month) {
        return Err(format!(
            "Invalid {filter_name}: date. Use {filter_name}:YYYY-MM-DD."
        ));
    }

    let max_day = days_in_month(year, month);
    if day < 1 || day > max_day {
        return Err(format!(
            "Invalid {filter_name}: date. Use {filter_name}:YYYY-MM-DD."
        ));
    }

    Ok(days_from_civil(year, month, day) * 86_400_000)
}

fn parse_date_parts(value: &str) -> Option<[i64; 3]> {
    let mut parts = value.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;

    if parts.next().is_some() {
        return None;
    }

    Some([year, month, day])
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_for_day = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_for_day + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era - 719_468
}

fn parse_snippet_parts(snippet: &str) -> Vec<SearchSnippetPart> {
    let mut parts = Vec::new();
    let mut remaining = snippet;
    let mut is_match = false;

    while !remaining.is_empty() {
        let marker = if is_match { MATCH_END } else { MATCH_START };

        if let Some(index) = remaining.find(marker) {
            push_snippet_part(&mut parts, &remaining[..index], is_match);
            remaining = &remaining[index + marker.len()..];
            is_match = !is_match;
        } else {
            push_snippet_part(&mut parts, remaining, is_match);
            break;
        }
    }

    parts
}

fn push_snippet_part(parts: &mut Vec<SearchSnippetPart>, text: &str, is_match: bool) {
    if text.is_empty() {
        return;
    }

    if let Some(last) = parts.last_mut() {
        if last.is_match == is_match {
            last.text.push_str(text);
            return;
        }
    }

    parts.push(SearchSnippetPart {
        text: text.to_string(),
        is_match,
    });
}

fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
