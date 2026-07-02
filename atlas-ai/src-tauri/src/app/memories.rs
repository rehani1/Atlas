use rusqlite::Connection;

use crate::{
    domain::memory::{Memory, MemoryPromptSetting, MemoryScopeType, PromptMemoryUse},
    infra::memories,
};

const MAX_MEMORY_CHARS: usize = 4_000;
const PROMPT_MEMORY_LIMIT: i64 = 12;

pub(crate) struct CreateMemory<'a> {
    pub(crate) scope_type: MemoryScopeType,
    pub(crate) scope_id: Option<&'a str>,
    pub(crate) content: &'a str,
    pub(crate) source_conversation_id: Option<&'a str>,
    pub(crate) source_message_id: Option<i64>,
    pub(crate) pinned: bool,
    pub(crate) now: i64,
}

pub(crate) fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Memory>, String> {
    memories::list(conn, include_archived).map_err(|error| error.to_string())
}

pub(crate) fn create(conn: &Connection, input: CreateMemory<'_>) -> Result<Memory, String> {
    let content = normalize_memory_content(input.content)?;
    validate_scope(input.scope_type, input.scope_id)?;

    memories::insert(
        conn,
        memories::MemoryInsert {
            scope_type: input.scope_type,
            scope_id: input.scope_id,
            content: &content,
            source_conversation_id: input.source_conversation_id,
            source_message_id: input.source_message_id,
            confidence: Some(1.0),
            pinned: input.pinned,
            now: input.now,
        },
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn update(
    conn: &Connection,
    memory_id: &str,
    content: &str,
    pinned: bool,
    now: i64,
) -> Result<Memory, String> {
    let content = normalize_memory_content(content)?;

    memories::update(
        conn,
        memory_id,
        memories::MemoryUpdate {
            content: &content,
            pinned,
            updated_at: now,
        },
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn archive(conn: &Connection, memory_id: &str, now: i64) -> Result<Memory, String> {
    memories::set_archived(conn, memory_id, Some(now), now).map_err(|error| error.to_string())
}

pub(crate) fn restore(conn: &Connection, memory_id: &str, now: i64) -> Result<Memory, String> {
    memories::set_archived(conn, memory_id, None, now).map_err(|error| error.to_string())
}

pub(crate) fn delete(conn: &Connection, memory_id: &str) -> Result<bool, String> {
    memories::delete(conn, memory_id).map_err(|error| error.to_string())
}

pub(crate) fn prompt_setting(
    conn: &Connection,
    conversation_id: &str,
) -> Result<MemoryPromptSetting, String> {
    memories::get_prompt_setting(conn, conversation_id)
        .map_err(|error| error.to_string())?
        .map(Ok)
        .unwrap_or_else(|| {
            Ok(MemoryPromptSetting {
                conversation_id: conversation_id.to_string(),
                enabled_for_prompt: false,
                created_at: 0,
                updated_at: 0,
            })
        })
}

pub(crate) fn set_prompt_enabled(
    conn: &Connection,
    conversation_id: &str,
    enabled_for_prompt: bool,
    now: i64,
) -> Result<MemoryPromptSetting, String> {
    memories::set_prompt_enabled(conn, conversation_id, enabled_for_prompt, now)
        .map_err(|error| error.to_string())
}

pub(crate) fn prompt_memories(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Vec<Memory>, String> {
    memories::list_prompt_memories(conn, conversation_id, PROMPT_MEMORY_LIMIT)
        .map_err(|error| error.to_string())
}

pub(crate) fn record_generation_uses(
    conn: &Connection,
    generation_run_id: &str,
    memories: &[Memory],
    used_at: i64,
) -> Result<Vec<PromptMemoryUse>, String> {
    memories::insert_generation_uses(conn, generation_run_id, memories, used_at)
        .map_err(|error| error.to_string())
}

fn normalize_memory_content(content: &str) -> Result<String, String> {
    let content = content.trim();

    if content.is_empty() {
        return Err("Memory cannot be empty.".to_string());
    }

    if content.chars().count() > MAX_MEMORY_CHARS {
        return Err(format!(
            "Memory must be {MAX_MEMORY_CHARS} characters or fewer."
        ));
    }

    Ok(content.to_string())
}

fn validate_scope(scope_type: MemoryScopeType, scope_id: Option<&str>) -> Result<(), String> {
    match scope_type {
        MemoryScopeType::Global => {
            if scope_id.is_some_and(|scope_id| !scope_id.trim().is_empty()) {
                return Err("Global memories cannot have a scope ID.".to_string());
            }
        }
        MemoryScopeType::Conversation | MemoryScopeType::Project => {
            if scope_id.map_or(true, |scope_id| scope_id.trim().is_empty()) {
                return Err("Scoped memories require a scope ID.".to_string());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{normalize_memory_content, validate_scope};
    use crate::domain::memory::MemoryScopeType;

    #[test]
    fn normalize_memory_content_trims_and_rejects_empty_text() {
        assert_eq!(
            normalize_memory_content("  Use SQLite for local data.  ").unwrap(),
            "Use SQLite for local data."
        );
        assert_eq!(
            normalize_memory_content("   ").unwrap_err(),
            "Memory cannot be empty."
        );
    }

    #[test]
    fn validate_scope_requires_ids_only_for_scoped_memory() {
        assert!(validate_scope(MemoryScopeType::Global, None).is_ok());
        assert!(validate_scope(MemoryScopeType::Global, Some("chat-1")).is_err());
        assert!(validate_scope(MemoryScopeType::Conversation, Some("chat-1")).is_ok());
        assert!(validate_scope(MemoryScopeType::Conversation, None).is_err());
    }
}
