use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    app::knowledge as knowledge_service,
    domain::{
        model::validate_ollama_model_name,
        tools::{ToolCall, ToolCallInsert, ToolPermissionDecision, ToolPermissionScopeType},
    },
    infra::tools,
};

const MAX_SUMMARY_CHARS: usize = 500;

#[derive(Clone, Debug)]
pub(crate) struct ParsedToolRequest {
    pub(crate) tool_name: String,
    pub(crate) arguments_json: String,
    pub(crate) arguments_summary: String,
    request_only: bool,
}

#[derive(Deserialize)]
struct DirectToolRequest {
    tool_name: String,
    arguments: Option<Value>,
}

#[derive(Deserialize)]
struct ToolEnvelope {
    atlas_tool_call: DirectToolRequest,
}

pub(crate) fn parse_model_tool_request(content: &str) -> Option<ParsedToolRequest> {
    let trimmed = content.trim();
    let (candidate, request_only) = extract_json_candidate(trimmed)?;
    let value = serde_json::from_str::<Value>(candidate).ok()?;
    let request = if value.get("atlas_tool_call").is_some() {
        let envelope = serde_json::from_value::<ToolEnvelope>(value).ok()?;
        envelope.atlas_tool_call
    } else if value.get("tool_name").is_some() {
        serde_json::from_value::<DirectToolRequest>(value).ok()?
    } else {
        return None;
    };

    let tool_name = normalize_tool_name(&request.tool_name)?;
    let arguments = request
        .arguments
        .unwrap_or_else(|| Value::Object(Default::default()));
    let arguments_summary = summarize_arguments(&tool_name, &arguments);
    let arguments_json =
        serde_json::to_string(&sanitize_arguments_for_log(&tool_name, &arguments)).ok()?;

    Some(ParsedToolRequest {
        tool_name,
        arguments_json,
        arguments_summary,
        request_only,
    })
}

pub(crate) fn visible_assistant_content(content: &str) -> String {
    let Some(request) = parse_model_tool_request(content) else {
        return content.trim().to_string();
    };

    if request.request_only {
        return format!(
            "Requested Atlas tool: {} ({})",
            request.tool_name, request.arguments_summary
        );
    }

    content.trim().to_string()
}

pub(crate) fn record_pending_from_model_output(
    conn: &Connection,
    conversation_id: &str,
    message_id: i64,
    content: &str,
    now: i64,
) -> Result<Vec<ToolCall>, String> {
    let Some(request) = parse_model_tool_request(content) else {
        return Ok(Vec::new());
    };

    let tool_call = tools::insert_tool_call(
        conn,
        ToolCallInsert {
            conversation_id,
            message_id: Some(message_id),
            tool_name: &request.tool_name,
            arguments_json: &request.arguments_json,
            arguments_summary: &request.arguments_summary,
            created_at: now,
        },
    )
    .map_err(|error| error.to_string())?;

    Ok(vec![tool_call])
}

pub(crate) fn resolve_tool_call(
    conn: &Connection,
    tool_call_id: &str,
    decision: ToolPermissionDecision,
    now: i64,
) -> Result<ToolCall, String> {
    let tool_call = tools::get_tool_call(conn, tool_call_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Tool call was not found.".to_string())?;

    if tool_call.status.as_str() != "pending" {
        return Ok(tool_call);
    }

    match decision {
        ToolPermissionDecision::Deny => {
            tools::mark_denied(conn, tool_call_id, now).map_err(|error| error.to_string())
        }
        ToolPermissionDecision::AllowOnce => execute_and_mark(conn, &tool_call, now),
        ToolPermissionDecision::AlwaysAllowWorkspace => {
            let workspace_id = infer_workspace_scope(conn, &tool_call)?
                .ok_or_else(|| "This tool call has no indexed workspace scope.".to_string())?;
            tools::upsert_permission(
                conn,
                ToolPermissionScopeType::Workspace,
                &workspace_id,
                &tool_call.tool_name,
                now,
            )
            .map_err(|error| error.to_string())?;
            execute_and_mark(conn, &tool_call, now)
        }
    }
}

fn execute_and_mark(conn: &Connection, tool_call: &ToolCall, now: i64) -> Result<ToolCall, String> {
    match execute_tool(conn, tool_call) {
        Ok(result_summary) => {
            tools::mark_succeeded(conn, &tool_call.id, &truncate_summary(&result_summary), now)
                .map_err(|error| error.to_string())
        }
        Err(error) => tools::mark_failed(conn, &tool_call.id, &error, now)
            .map_err(|sql_error| sql_error.to_string()),
    }
}

fn execute_tool(conn: &Connection, tool_call: &ToolCall) -> Result<String, String> {
    let arguments = serde_json::from_str::<Value>(&tool_call.arguments_json)
        .map_err(|error| error.to_string())?;

    match tool_call.tool_name.as_str() {
        "search_index" => execute_search_index(conn, &arguments),
        "read_file_chunk" => execute_read_file_chunk(conn, &arguments),
        "get_model_stats" => execute_get_model_stats(conn, &arguments),
        _ => Err(format!(
            "{} is not enabled in this MVP. Available tools: search_index, read_file_chunk, get_model_stats.",
            tool_call.tool_name
        )),
    }
}

fn execute_search_index(conn: &Connection, arguments: &Value) -> Result<String, String> {
    let query = string_argument(arguments, "query")?;
    let results = knowledge_service::search_documents(conn, &query, Some(5))?;

    if results.is_empty() {
        return Ok(format!("No indexed chunks matched query \"{query}\"."));
    }

    let matches = results
        .iter()
        .map(|result| {
            format!(
                "{} chunk {} lines {}-{}",
                result.file_name, result.chunk_id, result.start_line, result.end_line
            )
        })
        .collect::<Vec<_>>()
        .join("; ");

    Ok(format!(
        "Found {} indexed chunk {} for \"{}\": {}",
        results.len(),
        if results.len() == 1 {
            "match"
        } else {
            "matches"
        },
        query,
        matches
    ))
}

fn execute_read_file_chunk(conn: &Connection, arguments: &Value) -> Result<String, String> {
    let chunk_id = string_argument(arguments, "chunk_id")?;
    let chunk = knowledge_service::get_chunk(conn, &chunk_id)?;

    Ok(format!(
        "Read approved indexed chunk {} from document {} lines {}-{}. Content length: {} chars. Full content is not stored in the tool-call log.",
        chunk.id,
        chunk.document_id,
        chunk.start_line,
        chunk.end_line,
        chunk.content.chars().count()
    ))
}

fn execute_get_model_stats(conn: &Connection, arguments: &Value) -> Result<String, String> {
    let model_name = validate_ollama_model_name(&string_argument(arguments, "model_name")?)?;
    let (generation_count, last_used_at, average_tokens_per_second): (
        i64,
        Option<i64>,
        Option<f64>,
    ) = conn
        .query_row(
            "
      SELECT
        COUNT(*),
        MAX(started_at),
        AVG(tokens_per_second)
      FROM generation_runs
      WHERE model_name = ?1
        AND status = 'completed'
      ",
            params![model_name],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| error.to_string())?;
    let benchmark_count: i64 = conn
        .query_row(
            "
      SELECT COUNT(*)
      FROM model_benchmarks
      WHERE model_name = ?1
        AND status = 'completed'
      ",
            params![model_name],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or(0);
    let speed = average_tokens_per_second
        .map(|value| format!("{value:.1} tokens/s"))
        .unwrap_or_else(|| "no speed recorded".to_string());
    let last_used = last_used_at
        .map(|value| value.to_string())
        .unwrap_or_else(|| "never".to_string());

    Ok(format!(
        "{} has {} completed generations, {} completed benchmarks, average speed {}, last used {}.",
        model_name, generation_count, benchmark_count, speed, last_used
    ))
}

fn infer_workspace_scope(
    conn: &Connection,
    tool_call: &ToolCall,
) -> Result<Option<String>, String> {
    let arguments = serde_json::from_str::<Value>(&tool_call.arguments_json)
        .map_err(|error| error.to_string())?;

    match tool_call.tool_name.as_str() {
        "read_file_chunk" => {
            let chunk_id = string_argument(&arguments, "chunk_id")?;
            conn.query_row(
                "
          SELECT d.workspace_id
          FROM document_chunks dc
          JOIN documents d ON d.id = dc.document_id
          WHERE dc.id = ?1
            AND d.deleted_at IS NULL
          ",
                params![chunk_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())
        }
        "search_index" => Ok(arguments
            .get("workspace_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)),
        _ => Ok(None),
    }
}

fn string_argument(arguments: &Value, key: &str) -> Result<String, String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| format!("Missing required string argument: {key}"))
}

fn extract_json_candidate(content: &str) -> Option<(&str, bool)> {
    if content.starts_with("```") {
        let without_start = content
            .strip_prefix("```json")
            .or_else(|| content.strip_prefix("```"))
            .unwrap_or(content)
            .trim();
        if let Some(end_index) = without_start.rfind("```") {
            let candidate = without_start[..end_index].trim();
            return Some((candidate, true));
        }
    }

    if content.starts_with('{') && content.ends_with('}') {
        return Some((content, true));
    }

    let start = content.find('{')?;
    let end = content.rfind('}')?;
    if end <= start {
        return None;
    }

    Some((content[start..=end].trim(), false))
}

fn normalize_tool_name(tool_name: &str) -> Option<String> {
    let normalized = tool_name.trim().to_ascii_lowercase();
    (!normalized.is_empty()
        && normalized
            .chars()
            .all(|character| character.is_ascii_lowercase() || character == '_'))
    .then_some(normalized)
}

fn summarize_arguments(tool_name: &str, arguments: &Value) -> String {
    match tool_name {
        "search_index" => arguments
            .get("query")
            .and_then(Value::as_str)
            .map(|query| format!("query: \"{}\"", truncate_inline(query, 120)))
            .unwrap_or_else(|| "query: missing".to_string()),
        "read_file_chunk" => arguments
            .get("chunk_id")
            .and_then(Value::as_str)
            .map(|chunk_id| format!("chunk_id: {}", truncate_inline(chunk_id, 80)))
            .unwrap_or_else(|| "chunk_id: missing".to_string()),
        "get_model_stats" => arguments
            .get("model_name")
            .and_then(Value::as_str)
            .map(|model_name| format!("model_name: {}", truncate_inline(model_name, 80)))
            .unwrap_or_else(|| "model_name: missing".to_string()),
        "summarize_document" => arguments
            .get("document_id")
            .and_then(Value::as_str)
            .map(|document_id| format!("document_id: {}", truncate_inline(document_id, 80)))
            .unwrap_or_else(|| "document_id: missing".to_string()),
        "create_note" => {
            let title = arguments
                .get("title")
                .and_then(Value::as_str)
                .map(|title| truncate_inline(title, 80))
                .unwrap_or_else(|| "missing".to_string());
            let content_chars = arguments
                .get("content")
                .and_then(Value::as_str)
                .map(|content| content.chars().count())
                .unwrap_or(0);
            format!("title: {title}; content: {content_chars} chars")
        }
        "export_chat" => {
            let conversation_id = arguments
                .get("conversation_id")
                .and_then(Value::as_str)
                .map(|conversation_id| truncate_inline(conversation_id, 80))
                .unwrap_or_else(|| "missing".to_string());
            let format = arguments
                .get("format")
                .and_then(Value::as_str)
                .map(|format| truncate_inline(format, 40))
                .unwrap_or_else(|| "missing".to_string());
            format!("conversation_id: {conversation_id}; format: {format}")
        }
        _ => {
            let keys = arguments
                .as_object()
                .map(|object| object.keys().cloned().collect::<Vec<_>>().join(", "))
                .unwrap_or_else(|| "none".to_string());
            format!("argument keys: {keys}")
        }
    }
}

fn sanitize_arguments_for_log(tool_name: &str, arguments: &Value) -> Value {
    match tool_name {
        "search_index" | "read_file_chunk" | "get_model_stats" | "summarize_document"
        | "export_chat" => arguments.clone(),
        "create_note" => {
            let mut sanitized = serde_json::Map::new();
            if let Some(title) = arguments.get("title").and_then(Value::as_str) {
                sanitized.insert(
                    "title".to_string(),
                    Value::String(truncate_inline(title, 120)),
                );
            }
            if let Some(content) = arguments.get("content").and_then(Value::as_str) {
                sanitized.insert(
                    "content".to_string(),
                    Value::String(format!("<redacted {} chars>", content.chars().count())),
                );
            }
            Value::Object(sanitized)
        }
        _ => arguments
            .as_object()
            .map(|object| {
                Value::Object(
                    object
                        .keys()
                        .map(|key| (key.clone(), Value::String("<redacted>".to_string())))
                        .collect(),
                )
            })
            .unwrap_or_else(|| Value::String("<redacted>".to_string())),
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

fn truncate_summary(value: &str) -> String {
    truncate_inline(value, MAX_SUMMARY_CHARS)
}

#[cfg(test)]
mod tests {
    use super::{parse_model_tool_request, visible_assistant_content};

    #[test]
    fn parses_wrapped_tool_request() {
        let request = parse_model_tool_request(
            r#"{"atlas_tool_call":{"tool_name":"search_index","arguments":{"query":"SQLite WAL"}}}"#,
        )
        .unwrap();

        assert_eq!(request.tool_name, "search_index");
        assert_eq!(request.arguments_summary, "query: \"SQLite WAL\"");
    }

    #[test]
    fn visible_content_summarizes_tool_only_response() {
        assert_eq!(
            visible_assistant_content(
                r#"{"tool_name":"read_file_chunk","arguments":{"chunk_id":"abc"}}"#
            ),
            "Requested Atlas tool: read_file_chunk (chunk_id: abc)"
        );
    }

    #[test]
    fn ignores_regular_json_without_tool_name() {
        assert!(parse_model_tool_request(r#"{"answer":"no tool"}"#).is_none());
    }

    #[test]
    fn redacts_private_write_arguments_from_log_json() {
        let request = parse_model_tool_request(
            r#"{"tool_name":"create_note","arguments":{"title":"Private","content":"secret body"}}"#,
        )
        .unwrap();

        assert_eq!(
            request.arguments_summary,
            "title: Private; content: 11 chars"
        );
        assert_eq!(
            request.arguments_json,
            r#"{"content":"<redacted 11 chars>","title":"Private"}"#
        );
    }
}
