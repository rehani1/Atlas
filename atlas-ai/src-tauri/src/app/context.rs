use rusqlite::Connection;

use crate::{
    domain::{
        context::{
            AssembledContext, ContextPromptMessage, GenerationContextItem,
            GenerationContextItemDraft, GenerationContextItemType,
        },
        knowledge::GenerationDocumentSourceUse,
        memory::{Memory, MemoryScopeType},
        summary::ConversationSummary,
    },
    infra::context,
    ChatMessage,
};

const MAX_CONTEXT_TOKEN_ESTIMATE: i64 = 12_000;
const PRIOR_MESSAGE_RESERVE_TOKENS: i64 = 1_000;
const PREVIEW_CHARS: usize = 180;
const SYSTEM_PROMPT: &str = "You are Atlas, a private local AI workspace. Answer directly from the visible conversation and explicitly provided context. Do not claim access to files, memory, or summaries unless those sources are provided in this prompt. Do not reveal hidden chain-of-thought.";
const NO_KNOWLEDGE_SOURCES_CONTEXT: &str = "The user enabled Atlas local knowledge for this chat, but no matching indexed source chunks were retrieved for this message. Do not invent file citations; if the answer depends on local files, say the indexed sources do not contain enough information.";

pub(crate) struct AssemblyInput<'a> {
    pub(crate) model_name: &'a str,
    pub(crate) messages: &'a [ChatMessage],
    pub(crate) prompt_summary: Option<&'a ConversationSummary>,
    pub(crate) prompt_memories: &'a [Memory],
    pub(crate) document_sources: &'a [GenerationDocumentSourceUse],
    pub(crate) knowledge_enabled: bool,
}

pub(crate) fn assemble(input: AssemblyInput<'_>) -> Result<AssembledContext, String> {
    let latest_user_index = input
        .messages
        .iter()
        .rposition(|message| message.role == "user")
        .ok_or_else(|| "Chat has no user message to send to Ollama".to_string())?;
    let latest_user_message = &input.messages[latest_user_index];
    let prior_messages = &input.messages[..latest_user_index];
    let mut order_index = 0_i64;
    let mut prompt_messages = Vec::new();
    let mut items = Vec::new();
    let mut prompt_token_estimate = 0_i64;

    push_prompt_message(
        &mut prompt_messages,
        &mut items,
        &mut order_index,
        ContextPromptMessage {
            role: "system".to_string(),
            content: SYSTEM_PROMPT.to_string(),
        },
        item(
            GenerationContextItemType::SystemPrompt,
            Some(format!("hash:{}", stable_hash(SYSTEM_PROMPT.as_bytes()))),
            "Atlas default system prompt".to_string(),
            SYSTEM_PROMPT,
            Some(metadata_json(&[
                ("hash", stable_hash(SYSTEM_PROMPT.as_bytes())),
                ("preview", preview(SYSTEM_PROMPT)),
            ])?),
        ),
        &mut prompt_token_estimate,
    );

    if let Some(summary) = input.prompt_summary {
        let content = summary_prompt_context(summary);
        push_prompt_message(
            &mut prompt_messages,
            &mut items,
            &mut order_index,
            ContextPromptMessage {
                role: "system".to_string(),
                content: content.clone(),
            },
            item(
                GenerationContextItemType::Summary,
                Some(summary.id.clone()),
                format!("Conversation summary v{}", summary.version),
                &content,
                Some(metadata_json(&[
                    ("model", summary.model_name.clone()),
                    (
                        "source_message_start_id",
                        summary
                            .source_message_start_id
                            .map_or_else(|| "unknown".to_string(), |id| id.to_string()),
                    ),
                    (
                        "source_message_end_id",
                        summary
                            .source_message_end_id
                            .map_or_else(|| "unknown".to_string(), |id| id.to_string()),
                    ),
                    ("preview", preview(&summary.summary)),
                ])?),
            ),
            &mut prompt_token_estimate,
        );
    }

    if !input.prompt_memories.is_empty() {
        let content = memory_prompt_context(input.prompt_memories);
        push_prompt_message(
            &mut prompt_messages,
            &mut items,
            &mut order_index,
            ContextPromptMessage {
                role: "system".to_string(),
                content: content.clone(),
            },
            item(
                GenerationContextItemType::Memory,
                None,
                format!(
                    "{} enabled {}",
                    input.prompt_memories.len(),
                    if input.prompt_memories.len() == 1 {
                        "memory"
                    } else {
                        "memories"
                    }
                ),
                &content,
                Some(metadata_json(&[(
                    "memory_ids",
                    input
                        .prompt_memories
                        .iter()
                        .map(|memory| memory.id.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                )])?),
            ),
            &mut prompt_token_estimate,
        );

        for memory in input.prompt_memories {
            let label = if memory.pinned {
                "Pinned memory"
            } else {
                "Enabled memory"
            };
            push_metadata_item(
                &mut items,
                &mut order_index,
                GenerationContextItemType::Memory,
                Some(memory.id.clone()),
                format!("{label}: {}", preview(&memory.content)),
                estimate_tokens(&memory.content),
                Some(metadata_json(&[
                    ("scope", memory_scope_label(memory)),
                    ("pinned", memory.pinned.to_string()),
                    ("preview", preview(&memory.content)),
                ])?),
            );
        }
    }

    let document_context_token_reserve = if !input.document_sources.is_empty() {
        estimate_tokens(&document_prompt_context(input.document_sources))
    } else if input.knowledge_enabled {
        estimate_tokens(NO_KNOWLEDGE_SOURCES_CONTEXT)
    } else {
        0
    };
    let latest_user_tokens = estimate_tokens(&latest_user_message.content);
    let base_remaining = MAX_CONTEXT_TOKEN_ESTIMATE
        .saturating_sub(prompt_token_estimate)
        .saturating_sub(document_context_token_reserve)
        .saturating_sub(latest_user_tokens)
        .saturating_sub(PRIOR_MESSAGE_RESERVE_TOKENS);
    let mut prior_budget = base_remaining.max(0);
    let mut selected_prior_messages = Vec::new();
    let mut omitted_prior_message_count = 0_i64;

    for message in prior_messages.iter().rev() {
        let token_count = estimate_tokens(&message.content);
        if token_count <= prior_budget {
            selected_prior_messages.push(message);
            prior_budget -= token_count;
        } else {
            omitted_prior_message_count += 1;
        }
    }
    selected_prior_messages.reverse();

    for message in selected_prior_messages {
        push_prompt_message(
            &mut prompt_messages,
            &mut items,
            &mut order_index,
            ContextPromptMessage {
                role: message.role.clone(),
                content: message.content.clone(),
            },
            item(
                GenerationContextItemType::PriorMessage,
                Some(message.id.to_string()),
                format!("{} message {}", message.role, message.id),
                &message.content,
                Some(metadata_json(&[
                    ("role", message.role.clone()),
                    ("preview", preview(&message.content)),
                ])?),
            ),
            &mut prompt_token_estimate,
        );
    }

    if omitted_prior_message_count > 0 {
        push_metadata_item(
            &mut items,
            &mut order_index,
            GenerationContextItemType::TruncationNotice,
            None,
            format!(
                "Omitted {omitted_prior_message_count} older prior {} to stay under the context budget",
                if omitted_prior_message_count == 1 {
                    "message"
                } else {
                    "messages"
                }
            ),
            0,
            Some(metadata_json(&[(
                "omitted_prior_message_count",
                omitted_prior_message_count.to_string(),
            )])?),
        );
    }

    if !input.document_sources.is_empty() {
        let content = document_prompt_context(input.document_sources);
        push_prompt_message(
            &mut prompt_messages,
            &mut items,
            &mut order_index,
            ContextPromptMessage {
                role: "system".to_string(),
                content: content.clone(),
            },
            item(
                GenerationContextItemType::DocumentChunk,
                None,
                format!(
                    "{} retrieved source {}",
                    input.document_sources.len(),
                    if input.document_sources.len() == 1 {
                        "chunk"
                    } else {
                        "chunks"
                    }
                ),
                &content,
                Some(metadata_json(&[(
                    "source_ids",
                    input
                        .document_sources
                        .iter()
                        .map(|source| source.source_id.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                )])?),
            ),
            &mut prompt_token_estimate,
        );

        for source in input.document_sources {
            push_metadata_item(
                &mut items,
                &mut order_index,
                GenerationContextItemType::DocumentChunk,
                source.chunk_id.clone(),
                format!(
                    "[{}] {} lines {}-{}",
                    source.source_id, source.file_name, source.start_line, source.end_line
                ),
                estimate_tokens(&source.content),
                Some(metadata_json(&[
                    ("source_id", source.source_id.clone()),
                    ("file_name", source.file_name.clone()),
                    ("path", source.path.clone()),
                    ("preview", preview(&source.content)),
                ])?),
            );
        }
    } else if input.knowledge_enabled {
        let content = NO_KNOWLEDGE_SOURCES_CONTEXT.to_string();
        push_prompt_message(
            &mut prompt_messages,
            &mut items,
            &mut order_index,
            ContextPromptMessage {
                role: "system".to_string(),
                content: content.clone(),
            },
            item(
                GenerationContextItemType::DocumentChunk,
                None,
                "No matching knowledge chunks retrieved".to_string(),
                &content,
                Some(metadata_json(&[("knowledge_enabled", "true".to_string())])?),
            ),
            &mut prompt_token_estimate,
        );
    }

    push_prompt_message(
        &mut prompt_messages,
        &mut items,
        &mut order_index,
        ContextPromptMessage {
            role: latest_user_message.role.clone(),
            content: latest_user_message.content.clone(),
        },
        item(
            GenerationContextItemType::UserMessage,
            Some(latest_user_message.id.to_string()),
            format!("Current user message {}", latest_user_message.id),
            &latest_user_message.content,
            Some(metadata_json(&[(
                "preview",
                preview(&latest_user_message.content),
            )])?),
        ),
        &mut prompt_token_estimate,
    );

    push_metadata_item(
        &mut items,
        &mut order_index,
        GenerationContextItemType::ModelOptions,
        Some(input.model_name.to_string()),
        format!("Ollama chat request for {}", input.model_name),
        0,
        Some(metadata_json(&[
            ("model", input.model_name.to_string()),
            ("stream", "true".to_string()),
            (
                "max_context_token_estimate",
                MAX_CONTEXT_TOKEN_ESTIMATE.to_string(),
            ),
            ("prompt_token_estimate", prompt_token_estimate.to_string()),
        ])?),
    );

    Ok(AssembledContext {
        messages: prompt_messages,
        items,
    })
}

pub(crate) fn record_generation_context(
    conn: &Connection,
    generation_run_id: &str,
    items: &[GenerationContextItemDraft],
    created_at: i64,
) -> Result<Vec<GenerationContextItem>, String> {
    context::insert_generation_context_items(conn, generation_run_id, items, created_at)
        .map_err(|error| error.to_string())
}

fn push_prompt_message(
    prompt_messages: &mut Vec<ContextPromptMessage>,
    items: &mut Vec<GenerationContextItemDraft>,
    order_index: &mut i64,
    prompt_message: ContextPromptMessage,
    item: GenerationContextItemDraft,
    prompt_token_estimate: &mut i64,
) {
    *prompt_token_estimate += item.token_count_estimate;
    prompt_messages.push(prompt_message);
    push_item(items, order_index, item);
}

fn push_metadata_item(
    items: &mut Vec<GenerationContextItemDraft>,
    order_index: &mut i64,
    item_type: GenerationContextItemType,
    item_id: Option<String>,
    label: String,
    token_count_estimate: i64,
    metadata_json: Option<String>,
) {
    push_item(
        items,
        order_index,
        GenerationContextItemDraft {
            item_type,
            item_id,
            label,
            token_count_estimate,
            order_index: *order_index,
            metadata_json,
        },
    );
}

fn push_item(
    items: &mut Vec<GenerationContextItemDraft>,
    order_index: &mut i64,
    mut item: GenerationContextItemDraft,
) {
    item.order_index = *order_index;
    *order_index += 1;
    items.push(item);
}

fn item(
    item_type: GenerationContextItemType,
    item_id: Option<String>,
    label: String,
    content: &str,
    metadata_json: Option<String>,
) -> GenerationContextItemDraft {
    GenerationContextItemDraft {
        item_type,
        item_id,
        label,
        token_count_estimate: estimate_tokens(content),
        order_index: 0,
        metadata_json,
    }
}

fn summary_prompt_context(summary: &ConversationSummary) -> String {
    format!(
        "The user enabled this conversation summary for prompt context. Use it only as a transparent aid for this chat; the full visible message history follows.\n\nConversation summary v{} covering messages {}-{}:\n{}",
        summary.version,
        summary
            .source_message_start_id
            .map_or_else(|| "unknown".to_string(), |id| id.to_string()),
        summary
            .source_message_end_id
            .map_or_else(|| "unknown".to_string(), |id| id.to_string()),
        summary.summary
    )
}

fn memory_prompt_context(memories: &[Memory]) -> String {
    let mut content = String::from(
        "The user enabled these Atlas memories for this chat. Treat them as user-owned context, not hidden model memory. Use them only when relevant, and do not invent additional memories.\n\n",
    );

    for (index, memory) in memories.iter().enumerate() {
        let scope = memory_scope_label(memory);
        let pinned = if memory.pinned { " pinned" } else { "" };
        content.push_str(&format!(
            "{}. [{}{}] {}\n",
            index + 1,
            scope,
            pinned,
            memory.content
        ));
    }

    content
}

fn document_prompt_context(sources: &[GenerationDocumentSourceUse]) -> String {
    let mut content = String::from(
        "The user enabled Atlas local knowledge for this chat. Answer from the provided source excerpts when they are relevant. Cite sources with their bracketed IDs like [S1]. Do not invent citations. If these sources are insufficient, say that the indexed sources do not contain enough information.\n\n",
    );

    for source in sources {
        content.push_str(&format!(
            "[{}] {} lines {}-{} (document_id={}, chunk_id={})\n{}\n\n",
            source.source_id,
            source.file_name,
            source.start_line,
            source.end_line,
            source.document_id.as_deref().unwrap_or("unknown"),
            source.chunk_id.as_deref().unwrap_or("unknown"),
            source.content
        ));
    }

    content
}

fn memory_scope_label(memory: &Memory) -> String {
    match memory.scope_type {
        MemoryScopeType::Global => "global".to_string(),
        MemoryScopeType::Conversation => memory.scope_id.as_ref().map_or_else(
            || "conversation".to_string(),
            |scope_id| format!("conversation:{scope_id}"),
        ),
        MemoryScopeType::Project => memory.scope_id.as_ref().map_or_else(
            || "project".to_string(),
            |scope_id| format!("project:{scope_id}"),
        ),
    }
}

fn estimate_tokens(content: &str) -> i64 {
    ((content.chars().count() as f64) / 4.0).ceil().max(1.0) as i64
}

fn preview(content: &str) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= PREVIEW_CHARS {
        return normalized;
    }

    let mut preview = normalized
        .chars()
        .take(PREVIEW_CHARS - 3)
        .collect::<String>();
    preview.push_str("...");
    preview
}

fn stable_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;

    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }

    format!("{hash:016x}")
}

fn metadata_json(pairs: &[(&str, String)]) -> Result<String, String> {
    let mut map = serde_json::Map::new();

    for (key, value) in pairs {
        map.insert((*key).to_string(), serde_json::Value::String(value.clone()));
    }

    serde_json::to_string(&serde_json::Value::Object(map)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{assemble, AssemblyInput};
    use crate::{
        domain::{memory::MemoryScopeType, summary::ConversationSummary},
        ChatMessage,
    };

    fn message(id: i64, role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            id,
            chat_id: "chat-1".to_string(),
            role: role.to_string(),
            content: content.to_string(),
            created_at: id,
            generation_run: None,
        }
    }

    #[test]
    fn assemble_context_keeps_latest_user_and_truncates_old_messages() {
        let mut messages = Vec::new();
        for id in 1..45 {
            messages.push(message(id, "user", &"older context ".repeat(900)));
        }
        messages.push(message(45, "user", "What changed?"));

        let assembled = assemble(AssemblyInput {
            model_name: "llama3.2:3b",
            messages: &messages,
            prompt_summary: None,
            prompt_memories: &[],
            document_sources: &[],
            knowledge_enabled: false,
        })
        .unwrap();

        assert!(assembled
            .items
            .iter()
            .any(|item| item.item_type.as_str() == "truncation_notice"));
        assert!(assembled
            .items
            .iter()
            .any(|item| item.item_type.as_str() == "user_message"
                && item.item_id.as_deref() == Some("45")));
        assert_eq!(assembled.messages.last().unwrap().content, "What changed?");
    }

    #[test]
    fn assemble_context_records_summary_metadata() {
        let messages = vec![message(1, "user", "Summarize this.")];
        let summary = ConversationSummary {
            id: "summary-1".to_string(),
            conversation_id: "chat-1".to_string(),
            summary: "Current topic: context assembly".to_string(),
            source_message_start_id: Some(1),
            source_message_end_id: Some(1),
            model_name: "manual".to_string(),
            version: 2,
            enabled_for_prompt: true,
            created_at: 1,
            updated_at: 2,
        };

        let assembled = assemble(AssemblyInput {
            model_name: "llama3.2:3b",
            messages: &messages,
            prompt_summary: Some(&summary),
            prompt_memories: &[],
            document_sources: &[],
            knowledge_enabled: false,
        })
        .unwrap();

        let summary_item = assembled
            .items
            .iter()
            .find(|item| item.item_type.as_str() == "summary")
            .unwrap();
        assert_eq!(summary_item.item_id.as_deref(), Some("summary-1"));
        assert!(summary_item.label.contains("v2"));
    }

    #[test]
    fn assemble_context_rejects_chat_without_user_message() {
        let messages = vec![message(1, "assistant", "Hello")];

        assert!(assemble(AssemblyInput {
            model_name: "llama3.2:3b",
            messages: &messages,
            prompt_summary: None,
            prompt_memories: &[],
            document_sources: &[],
            knowledge_enabled: false,
        })
        .is_err());
    }

    #[test]
    fn assemble_context_includes_memory_metadata_items() {
        let messages = vec![message(1, "user", "Use memory?")];
        let memory = crate::domain::memory::Memory {
            id: "memory-1".to_string(),
            scope_type: MemoryScopeType::Global,
            scope_id: None,
            content: "User prefers local-only storage.".to_string(),
            source_conversation_id: None,
            source_message_id: None,
            confidence: Some(1.0),
            pinned: true,
            archived_at: None,
            created_at: 1,
            updated_at: 1,
        };

        let assembled = assemble(AssemblyInput {
            model_name: "llama3.2:3b",
            messages: &messages,
            prompt_summary: None,
            prompt_memories: &[memory],
            document_sources: &[],
            knowledge_enabled: false,
        })
        .unwrap();

        assert!(assembled
            .items
            .iter()
            .any(|item| item.item_type.as_str() == "memory"
                && item.item_id.as_deref() == Some("memory-1")));
    }
}
