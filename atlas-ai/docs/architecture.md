# Atlas Current-State Architecture

Last audited: 2026-07-02

This document records the current Atlas `1.0.0` architecture during the remake
chunks. It is descriptive, not the target architecture.

## Scope

Atlas is a local-first desktop chat app built with Tauri 2, Rust, React, Vite,
Tailwind CSS, SQLite, and Ollama. The current codebase is intentionally compact:

- Frontend UI and state live in `src/App.tsx`.
- Most backend state, SQLite repositories, command handlers, streaming, and
  cancellation still live in `src-tauri/src/lib.rs`.
- Backend service slices now include model management, jobs, database
  setup/diagnostics, FTS search, model benchmarks, summaries, and memories:
  `src-tauri/src/domain/model.rs`, `src-tauri/src/app/models.rs`,
  `src-tauri/src/domain/job.rs`, `src-tauri/src/app/jobs.rs`,
  `src-tauri/src/domain/benchmark.rs`, `src-tauri/src/app/benchmarks.rs`,
  `src-tauri/src/domain/summary.rs`, `src-tauri/src/app/summaries.rs`,
  `src-tauri/src/domain/memory.rs`, `src-tauri/src/app/memories.rs`,
  `src-tauri/src/infra/jobs.rs`, `src-tauri/src/domain/database.rs`,
  `src-tauri/src/domain/search.rs`, `src-tauri/src/infra/search.rs`,
  `src-tauri/src/infra/benchmarks.rs`, `src-tauri/src/infra/summaries.rs`,
  `src-tauri/src/infra/memories.rs`, `src-tauri/src/infra/sqlite.rs`, and
  `src-tauri/src/infra/ollama.rs`.
- `src-tauri/src/main.rs` only starts `atlas_lib::run()`.
- Public release docs are `README.md` and `CHANGELOG.md`.

There is a minimal typed frontend API wrapper for touched Ollama status, model
lifecycle, export, jobs, database diagnostics, rich search, summaries, memory,
and benchmark commands in `src/shared/api/tauri.ts`. There are no frontend
feature folders, Rust `commands` module, database migrations directory,
document indexing, or import surfaces yet.

`src/App.tsx` now also owns a small frontend-only command registry and
`Cmd/Ctrl+K` command palette. The registry uses stable command IDs and routes
enabled commands to the same local handlers used by the visible UI controls.

## Tooling

The app directory is `atlas-ai/`. Existing scripts and checks are:

```bash
npm install
npm run dev
npm run build
npm run lint
npm run tauri:dev
npm run tauri:build
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

`npm run build` runs `tsc -b && vite build`. There is no dedicated
`npm run typecheck` script.

Current Tauri permissions are limited to `core:default` in
`src-tauri/capabilities/default.json`.

## Runtime Boundaries

Rust owns privileged operations:

- SQLite connection, schema setup, diagnostics, FTS search, benchmark storage,
  memory storage, and queries.
- Chat and message persistence.
- Ollama readiness, model list, pull, delete, and chat requests.
- Model name validation.
- Assistant generation cancellation state.
- Tauri app setup and app-data directory creation.

React owns UI and interaction state:

- Sidebar and model panel visibility.
- Active chat and message display.
- Composer draft.
- Model selection.
- Search input/results.
- Command palette visibility, query, and active result.
- Loading and error states.
- Browser-preview fallback behavior when Tauri is unavailable.

Most Tauri command handlers are not thin IPC wrappers yet. The model status and
model listing commands now call the model app service; the rest of the backend
is still being migrated one vertical slice at a time.

## IPC Command Contract

The backend exposes these Tauri commands:

```text
list_chats() -> Vec<ChatSummary>
search_chats(query: String) -> Vec<ChatSummary>
search_conversations(query: String, limit: Option<i64>) -> Vec<ChatSearchResult>
create_chat(title: Option<String>) -> ChatSummary
get_messages(chat_id: String) -> Vec<ChatMessage>
add_message(chat_id: String, role: String, content: String) -> ChatMessage
delete_chat(chat_id: String) -> bool
export_chat(chat_id: String, format: ChatExportFormat) -> ChatExport
get_ollama_status(selected_model: Option<String>) -> OllamaStatus
list_ollama_models() -> Vec<OllamaModel>
download_ollama_model(model: String) -> Job
delete_ollama_model(model: String) -> Vec<OllamaModel>
list_jobs(limit: Option<i64>) -> Vec<Job>
get_database_diagnostics() -> DatabaseDiagnostics
get_conversation_summary(chat_id: String) -> Option<ConversationSummary>
save_conversation_summary(chat_id: String, summary: String, enabled_for_prompt: bool) -> ConversationSummary
set_conversation_summary_enabled(chat_id: String, enabled_for_prompt: bool) -> ConversationSummary
delete_conversation_summary(chat_id: String) -> bool
generate_conversation_summary(chat_id: String, model: String) -> ConversationSummary
list_memories(include_archived: Option<bool>) -> Vec<Memory>
create_memory(scope_type: MemoryScopeType, scope_id: Option<String>, content: String, source_conversation_id: Option<String>, source_message_id: Option<i64>, pinned: bool) -> Memory
update_memory(memory_id: String, content: String, pinned: bool) -> Memory
archive_memory(memory_id: String) -> Memory
restore_memory(memory_id: String) -> Memory
delete_memory(memory_id: String) -> bool
get_memory_prompt_setting(chat_id: String) -> MemoryPromptSetting
set_memory_prompt_enabled(chat_id: String, enabled_for_prompt: bool) -> MemoryPromptSetting
list_model_benchmarks(limit: Option<i64>) -> Vec<ModelBenchmark>
list_model_usage() -> Vec<ModelUsage>
cancel_job(job_id: String) -> Job
start_model_benchmark(model: String) -> Job
generate_assistant_response(chat_id: String, model: String) -> ChatMessage
cancel_ollama_generation(chat_id: String) -> bool
```

Frontend calls use camelCase keys, such as `{ chatId }`, and Tauri maps them to
Rust snake_case parameters, such as `chat_id`.

Current serialized types:

```text
ChatSummary
- id: string
- title: string
- created_at: number
- updated_at: number
- message_count: number

ChatMessage
- id: number
- chat_id: string
- role: "user" | "assistant" | "system"
- content: string
- created_at: number
- generation_run: GenerationRun | null

GenerationRun
- id: string
- conversation_id: string
- message_id: number | null
- model_name: string
- started_at: number
- first_token_at: number | null
- completed_at: number | null
- status: "running" | "completed" | "cancelled" | "failed"
- total_duration_ms: number | null
- load_duration_ms: number | null
- prompt_eval_count: number | null
- prompt_eval_duration_ms: number | null
- eval_count: number | null
- eval_duration_ms: number | null
- tokens_per_second: number | null
- error_message: string | null
- memory_uses: PromptMemoryUse[]

ChatExportFormat
- "markdown" | "json" | "plain_text"

ChatExport
- file_name: string
- mime_type: string
- content: string

OllamaModel
- name: string
- size: number

OllamaStatus
- status: "unavailable" | "running_with_models" | "running_without_models" | "selected_model_missing"
- models: OllamaModel[]
- selected_model: string | null
- error: string | null

Job
- id: string
- job_type: "chat_generation" | "model_pull" | "model_delete" | "export_conversation" | "document_import" | "embedding_index" | "model_benchmark" | "conversation_summary"
- status: "queued" | "running" | "cancelling" | "cancelled" | "succeeded" | "failed"
- progress_current: number | null
- progress_total: number | null
- label: string
- payload_json: string | null
- result_json: string | null
- error_message: string | null
- created_at: number
- started_at: number | null
- completed_at: number | null
- cancelled_at: number | null

JobEvent
- job_id: string
- job_type: Job.job_type
- job: Job

DatabaseTableCount
- table_name: string
- row_count: number

DatabaseDiagnostics
- path: string
- database_size_bytes: number
- wal_size_bytes: number
- shm_size_bytes: number
- journal_mode: string
- user_version: number
- page_count: number
- page_size: number
- freelist_count: number
- integrity_check: string
- table_counts: DatabaseTableCount[]

SearchSnippetPart
- text: string
- is_match: bool

ChatSearchResult
- chat_id: string
- chat_title: string
- message_id: number | null
- role: "user" | "assistant" | "system" | null
- created_at: number
- updated_at: number
- message_count: number
- source: "title" | "message"
- score: number
- snippet: SearchSnippetPart[]

ConversationSummary
- id: string
- conversation_id: string
- summary: string
- source_message_start_id: number | null
- source_message_end_id: number | null
- model_name: string
- version: number
- enabled_for_prompt: bool
- created_at: number
- updated_at: number

MemoryScopeType
- "global" | "conversation" | "project"

Memory
- id: string
- scope_type: MemoryScopeType
- scope_id: string | null
- content: string
- source_conversation_id: string | null
- source_message_id: number | null
- confidence: number | null
- pinned: bool
- archived_at: number | null
- created_at: number
- updated_at: number

MemoryPromptSetting
- conversation_id: string
- enabled_for_prompt: bool
- created_at: number
- updated_at: number

PromptMemoryUse
- id: string
- generation_run_id: string
- memory_id: string | null
- content: string
- scope_type: MemoryScopeType
- scope_id: string | null
- source_conversation_id: string | null
- source_message_id: number | null
- used_at: number

ModelBenchmark
- id: string
- job_id: string
- model_name: string
- prompt_type: string
- prompt_label: string
- prompt_text_hash: string
- started_at: number | null
- completed_at: number | null
- status: "queued" | "running" | "completed" | "cancelled" | "failed"
- total_duration_ms: number | null
- first_token_ms: number | null
- prompt_eval_count: number | null
- prompt_eval_duration_ms: number | null
- eval_count: number | null
- eval_duration_ms: number | null
- tokens_per_second: number | null
- error_message: string | null
- created_at: number

ModelUsage
- model_name: string
- last_used_at: number | null
- generation_count: number
```

The backend emits `job_updated` events for job creation, start, progress,
cancel, success, and failure. Events include both `job_id` and `job_type` so the
frontend can ignore stale updates by job identity.

## SQLite Persistence

SQLite is opened during Tauri setup at:

```text
app.path().app_data_dir()/atlas.sqlite3
```

The app creates the app-data directory if needed. `ChatStore::new()` opens the
database and calls `infra::sqlite::setup_database(&conn)`, which configures the
connection and runs repeatable baseline schema setup. The current
`chats`/`messages`/`generation_runs`/`jobs` schema is treated as baseline
version 1. Chunk 8 adds FTS search tables and records the current schema as
version 2. Chunk 9 adds `model_benchmarks` and records the current schema as
`PRAGMA user_version = 3`. Chunk 10 adds `conversation_summaries` and records
the current schema as `PRAGMA user_version = 4`. Chunk 11 adds transparent
memory tables and records the current schema as `PRAGMA user_version = 5`.

There is no migrations directory yet. Future schema changes should add
idempotent versions after the v1 baseline instead of editing historical setup in
ways that would break existing `atlas.sqlite3` files.

Connection setup applies:

```sql
PRAGMA busy_timeout = 5000;
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
```

Current tables:

```sql
CREATE TABLE IF NOT EXISTS chats (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
  content TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS generation_runs (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  message_id INTEGER REFERENCES messages(id) ON DELETE SET NULL,
  model_name TEXT NOT NULL,
  started_at INTEGER NOT NULL,
  first_token_at INTEGER,
  completed_at INTEGER,
  status TEXT NOT NULL CHECK(status IN ('running', 'completed', 'cancelled', 'failed')),
  total_duration_ms INTEGER,
  load_duration_ms INTEGER,
  prompt_eval_count INTEGER,
  prompt_eval_duration_ms INTEGER,
  eval_count INTEGER,
  eval_duration_ms INTEGER,
  tokens_per_second REAL,
  error_message TEXT
);

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

CREATE TABLE IF NOT EXISTS conversation_summaries (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL UNIQUE REFERENCES chats(id) ON DELETE CASCADE,
  summary TEXT NOT NULL,
  source_message_start_id INTEGER REFERENCES messages(id) ON DELETE SET NULL,
  source_message_end_id INTEGER REFERENCES messages(id) ON DELETE SET NULL,
  model_name TEXT NOT NULL,
  version INTEGER NOT NULL,
  enabled_for_prompt INTEGER NOT NULL DEFAULT 0 CHECK(enabled_for_prompt IN (0, 1)),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS memories (
  id TEXT PRIMARY KEY,
  scope_type TEXT NOT NULL CHECK(scope_type IN ('global', 'conversation', 'project')),
  scope_id TEXT,
  content TEXT NOT NULL,
  source_conversation_id TEXT REFERENCES chats(id) ON DELETE SET NULL,
  source_message_id INTEGER REFERENCES messages(id) ON DELETE SET NULL,
  confidence REAL,
  pinned INTEGER NOT NULL DEFAULT 0 CHECK(pinned IN (0, 1)),
  archived_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_prompt_settings (
  conversation_id TEXT PRIMARY KEY REFERENCES chats(id) ON DELETE CASCADE,
  enabled_for_prompt INTEGER NOT NULL DEFAULT 0 CHECK(enabled_for_prompt IN (0, 1)),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS generation_memory_uses (
  id TEXT PRIMARY KEY,
  generation_run_id TEXT NOT NULL REFERENCES generation_runs(id) ON DELETE CASCADE,
  memory_id TEXT REFERENCES memories(id) ON DELETE SET NULL,
  content_snapshot TEXT NOT NULL,
  scope_type TEXT NOT NULL CHECK(scope_type IN ('global', 'conversation', 'project')),
  scope_id TEXT,
  source_conversation_id TEXT,
  source_message_id INTEGER,
  used_at INTEGER NOT NULL
);
```

Current indexes and triggers:

```sql
CREATE INDEX IF NOT EXISTS idx_chats_updated_at
  ON chats(updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_messages_chat_id_created_at
  ON messages(chat_id, created_at, id);

CREATE INDEX IF NOT EXISTS idx_generation_runs_conversation_started
  ON generation_runs(conversation_id, started_at DESC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_generation_runs_message_id
  ON generation_runs(message_id)
  WHERE message_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_jobs_created_at
  ON jobs(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_jobs_status_created_at
  ON jobs(status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_model_benchmarks_model_completed
  ON model_benchmarks(model_name, completed_at DESC);

CREATE INDEX IF NOT EXISTS idx_model_benchmarks_job
  ON model_benchmarks(job_id);

CREATE INDEX IF NOT EXISTS idx_model_benchmarks_created
  ON model_benchmarks(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_conversation_summaries_updated
  ON conversation_summaries(updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_conversation_summaries_enabled
  ON conversation_summaries(conversation_id, enabled_for_prompt);

CREATE INDEX IF NOT EXISTS idx_memories_scope
  ON memories(scope_type, scope_id, archived_at, pinned DESC, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_memories_updated
  ON memories(archived_at, pinned DESC, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_memories_source_message
  ON memories(source_message_id);

CREATE INDEX IF NOT EXISTS idx_generation_memory_uses_run
  ON generation_memory_uses(generation_run_id, used_at ASC);

CREATE TRIGGER IF NOT EXISTS messages_after_insert_update_chat
AFTER INSERT ON messages
BEGIN
  UPDATE chats
  SET updated_at = NEW.created_at
  WHERE id = NEW.chat_id;
END;
```

Search indexing behavior:

- `infra::search::create_schema()` creates `chat_search` and `message_search`
  and runs an idempotent backfill from `chats` and `messages`.
- `chat_search` is synchronized on chat insert, title/update timestamp changes,
  and chat delete.
- `message_search` is synchronized on message insert, message update, message
  delete, and parent chat delete.
- Backfill also removes FTS rows whose source chat/message no longer exists, so
  deleted chats do not leave stale searchable rows.

Persistence behavior:

- Chat IDs are generated in SQLite with `lower(hex(randomblob(16)))`.
- Chat titles are trimmed, default to `New chat`, cap at 64 characters, and add
  `...` when truncated.
- Message roles are limited to `user`, `assistant`, and `system`.
- Message content is trimmed and empty content is rejected.
- Chat deletion relies on `ON DELETE CASCADE` to remove messages.
- Chat lists sort by `chats.updated_at DESC`.
- Message lists sort by `created_at ASC, id ASC`.
- `generation_runs` rows are created before generation starts and are finalized
  with completed, cancelled, or failed status.
- Assistant messages expose at most one associated generation run.
- `jobs` rows persist long-running work. Chunk 6 uses them for model pulls with
  status, progress bytes when Ollama provides totals, payload/result JSON, and
  readable failure messages.
- `model_benchmarks` rows persist one fixed-suite prompt result per benchmark
  job, including prompt hash, status, total duration, first-token latency,
  prompt/eval token counts, durations, completion tokens/sec, and readable
  failure/cancellation messages.
- `conversation_summaries` stores one current summary per chat, the source
  message ID range, model name, monotonically increasing version, and an
  explicit `enabled_for_prompt` flag. New generated summaries default to prompt
  use off unless the user had already enabled the existing summary.
- `memories` stores manual, user-owned memory rows with global, conversation,
  or future project scope. The MVP never writes memory automatically. Source
  chat/message IDs are stored only when the user explicitly creates memory from
  a visible message or supplies a source.
- `memory_prompt_settings` stores the per-chat memory inclusion switch. Missing
  rows behave as disabled.
- `generation_memory_uses` stores a snapshot of each memory included in a
  prompt for a generation run so message diagnostics can show what was used
  even if the memory is edited, archived, or forgotten later.
- During startup, queued/running/cancelling jobs from a previous process are
  marked `failed` with an interruption message so stale jobs do not remain
  cancellable forever.
- `export_chat` reads the chat, messages, and joined generation metadata,
  renders Markdown, JSON, or plain text in Rust, returns content with a
  sanitized filename and MIME type, and does not mutate SQLite.
- WAL mode is enabled for the file-backed desktop database.
- `get_database_diagnostics` reads the database path, database/WAL/SHM sizes,
  journal mode, schema user version, page counts, free pages, integrity check,
  and table counts for core tables, FTS tables, benchmark tables, summary
  tables, and memory tables. It does not export chat content or mutate user
  data.

## Chat Generation Flow

The user flow starts in `src/App.tsx`:

1. `handleSubmit()` trims the draft and exits if empty or already responding.
2. Browser preview mode appends only a local user message and shows that chat
   responses require the desktop app.
3. Desktop mode requires a selected model.
4. If there is no active chat, `create_chat` is called with a title derived from
   the first user message.
5. `add_message` persists the user message.
6. The frontend refreshes chat summaries.
7. `generate_assistant_response` is called with `chatId` and `model`.
8. If the active conversation summary exists and `enabled_for_prompt` is true,
   the backend prepends it as an explicit system context block before the
   visible chat messages.
9. If memory use is enabled for the chat, the backend loads active global and
   conversation-scoped memories, snapshots them to `generation_memory_uses`,
   and prepends them as an explicit system context block.
10. When the command resolves, the frontend appends the assistant message only if
   the active chat still matches the generating chat.
11. The frontend reloads messages and refreshes chat summaries.

The backend generation path:

1. Validates the model name.
2. Loads all messages for the chat from SQLite.
3. Loads the enabled conversation summary, if the user has turned prompt use on
   for this chat.
4. Loads active prompt memories only if `memory_prompt_settings` is enabled for
   this chat. Active prompt memories are unarchived global memories plus
   unarchived conversation memories scoped to the chat.
5. Creates a `generation_runs` row with status `running`.
6. Snapshots any prompt memories into `generation_memory_uses` for diagnostics.
7. Converts the optional summary, optional memory context, and all visible
   messages to Ollama chat messages. Summary and memory context blocks both
   state that the user enabled them.
8. Registers a cancellation flag in `GenerationTasks` keyed by `chat_id`.
9. Runs blocking Ollama streaming work on Tauri's blocking runtime.
10. Calls `POST http://127.0.0.1:11434/api/chat` with `stream: true`.
11. Reads Ollama JSONL chunks, accumulates `message.content`, tracks first
   non-empty token time, and captures optional final Ollama metadata:
   `total_duration`, `load_duration`, `prompt_eval_count`,
   `prompt_eval_duration`, `eval_count`, and `eval_duration`.
12. Converts Ollama nanosecond durations into rounded milliseconds and calculates
   tokens/sec from `eval_count / eval_duration`.
13. Inserts an assistant message and associates it with the generation run when
    final or partial assistant text exists.
14. Finalizes the generation run as `completed`, `cancelled`, or `failed`.

Important current limitation: chat is streamed from Ollama to Rust, but not
token-streamed from Rust to React. React shows animated progress dots while
waiting and receives one final `ChatMessage` after completion.

## Conversation Summary Flow

Conversation summaries are manual and per-chat:

- `get_conversation_summary(chat_id)` returns the current summary row, if one
  exists.
- `save_conversation_summary(chat_id, summary, enabled_for_prompt)` saves user
  edits, stores the current message source range, sets `model_name` to
  `manual`, and preserves the explicit prompt-use choice supplied by the UI.
- `generate_conversation_summary(chat_id, model)` creates a
  `conversation_summary` job, validates that the selected model is installed,
  sends the visible chat transcript to Ollama with a summary-specific prompt,
  and saves the generated summary with the source message range and model name.
- `set_conversation_summary_enabled(chat_id, enabled_for_prompt)` toggles
  whether the summary is included in future chat prompts for that conversation.
- `delete_conversation_summary(chat_id)` removes the summary and disables any
  future prompt use for that chat.

Generated summaries are cancellable through `cancel_job(job_id)`. Cancellation
does not persist partial summary text. Failures leave the previous summary, if
any, unchanged and are shown through the job surface and summary panel.

Prompt use is off by default for a newly generated summary. If the user had
already enabled a previous summary for that chat, generating an update preserves
that setting. When enabled, the chat screen shows a visible "Summary context on"
state and the summary action button is highlighted.

## Memory Flow

Memory is manual in the MVP:

- `list_memories(include_archived)` returns stored user memories for the Memory
  Inspector.
- `create_memory(...)` writes only user-submitted memory content. It supports
  `global` and `conversation` scope in the UI; `project` is reserved for future
  workspaces.
- Source message IDs are stored only when the user clicks `Remember` on a
  visible message. The backend validates that the source message belongs to the
  supplied source chat.
- `update_memory`, `archive_memory`, `restore_memory`, and `delete_memory`
  support edit, archive, restore, and forget actions from the inspector.
- `get_memory_prompt_setting(chat_id)` returns a disabled setting when no row
  exists. `set_memory_prompt_enabled(chat_id, enabled)` stores the visible
  per-chat memory-use switch.

Prompt inclusion is transparent:

- Memory use defaults off for every chat.
- When enabled, Atlas includes unarchived global memories and unarchived
  conversation memories scoped to the active chat.
- The active chat shows "Memory context on" and the active memory count.
- Assistant message diagnostics show `generation_run.memory_uses`, which are
  snapshots of the memories used for that prompt.
- Archiving or forgetting a memory prevents future prompt inclusion. Existing
  generation diagnostics keep the historical snapshot.

## Cancellation Flow

`GenerationTasks` stores `HashMap<String, Arc<AtomicBool>>` under a mutex, keyed
by `chat_id`.

- Starting a second generation for the same chat replaces the existing flag and
  flips the old flag to cancelled.
- `cancel_ollama_generation(chat_id)` removes the matching flag and sets it to
  cancelled.
- `stream_ollama_chat()` checks the flag before each blocking `read_line()`.
- After the blocking generation future resolves, the command removes the task
  entry for that chat only if it still owns the same cancellation flag.
- Cancellation is scoped by chat ID, so cancelling one chat does not directly
  cancel another chat.

Cancellation with partial assistant text now persists that partial text as an
assistant message and marks its generation run `cancelled`. Current limitation:
cancellation may not be immediate while a blocking read is stalled, and
cancellation before the first token still leaves no assistant message.

## Model Management Flow

Atlas assumes Ollama is local at `127.0.0.1:11434`.

The first service-layer slice is model management:

- `domain/model.rs` owns `OllamaModel`, `OllamaStatus`,
  `OllamaStatusKind`, model-name validation, and status derivation.
- `infra/ollama.rs` owns the blocking HTTP request helper, Ollama error-body
  parsing, and `/api/tags` model-list parsing.
- `app/models.rs` owns the model-listing and readiness-status service calls.

Current model operations:

- `get_ollama_status(selected_model)` calls `GET /api/tags` and returns one of
  four readiness states:
  `unavailable`, `running_with_models`, `running_without_models`, or
  `selected_model_missing`.
- `get_ollama_status(selected_model)` and `list_ollama_models()` are thin Tauri
  wrappers over `app::models`.
- `list_ollama_models()` calls `GET /api/tags` through `infra::ollama`.
- `download_ollama_model(model)` validates the model name, calls
  `POST /api/pull` with `{ "name": model, "stream": true }`, creates a
  persistent `model_pull` job, updates progress from Ollama JSON-line pull
  events, and emits `job_updated`.
- `delete_ollama_model(model)` validates the model name, calls
  `DELETE /api/delete`, then refreshes the model list.
- `cancel_job(job_id)` marks non-terminal jobs `cancelling` and flips the
  matching in-memory cancellation token when the job is active in this process.
- `list_jobs(limit)` returns recent jobs for the frontend status surface.
- `start_model_benchmark(model)` validates the model name, creates a
  `model_benchmark` job, creates queued `model_benchmarks` rows for the fixed
  five-prompt suite, verifies the model is already installed, then runs the
  prompts one at a time through local Ollama streaming.
- Only one benchmark job is allowed at a time in this process. Benchmark jobs
  use the same `cancel_job` path and store cancelled/failed rows when stopped.
- `list_model_benchmarks(limit)` returns persisted benchmark rows for Model Lab
  history.
- `list_model_usage()` groups `generation_runs` by model to show last chat usage
  and generation counts.

Model names reject empty strings, whitespace, double quotes, and backslashes.
An empty Ollama model list is represented as `running_without_models`, not a
backend exception.

Current limitations:

- Model pull still awaits command completion, but progress is visible through
  job events while the command runs.
- Model benchmark still awaits command completion, but progress is visible
  through job events while the command runs.
- Model pull cancellation is checked between blocking stream reads, so a stalled
  read may delay cancellation.
- Model benchmark cancellation is checked between blocking stream reads, so a
  stalled benchmark response may delay cancellation.
- Model delete still has command-level orchestration in `lib.rs` and no job
  record.
- Chat generation still uses `GenerationTasks` keyed by `chat_id`; it has not
  moved to the job system yet.

## Search Behavior

`search_chats(query)` remains as a compatibility command:

- Trims the query.
- Returns `list_chats()` for an empty query.
- Escapes `%`, `_`, and `\` for `LIKE`.
- Searches `LOWER(chats.title)` and `LOWER(messages.content)`.
- Returns `ChatSummary` objects only.
- Orders results by `updated_at DESC`.

`search_conversations(query, limit)` powers the desktop sidebar search:

- Parses free-text terms into a quoted FTS5 prefix query.
- Supports title matches through `chat_search`.
- Supports message-content matches through `message_search`.
- Supports basic future-compatible filters:
  `model:<name>`, `has:code`, `chat:<title>`, `from:YYYY-MM-DD`, and
  `before:YYYY-MM-DD`.
- Returns `ChatSearchResult` rows with chat ID/title, optional message ID, role,
  timestamps, message count, result source, rank score, and sanitized snippet
  parts.
- Uses SQLite `snippet()` markers only inside Rust and converts them into
  `SearchSnippetPart[]`; the frontend renders text spans rather than raw HTML.
- Sorts by FTS rank, then recent chat update time.
- Limits results to 30 by default and clamps requested limits to 1-100.

Current limitations:

- Search filters are intentionally small and token-based; quoted multi-word
  filter values are not supported yet.
- Message results can jump directly to a message, but there is not yet a
  full-page search surface or persistent result history.

## Frontend State Ownership

`src/App.tsx` owns all visible UI and app state:

- Sidebar open/closed state.
- Chat summaries and active chat ID.
- Current chat messages.
- Active conversation summary, editable summary draft, summary panel visibility,
  summary loading/action state, and summary errors.
- Memory list, active chat memory prompt setting, Memory Inspector visibility,
  memory form/edit/source state, memory loading/action state, and memory errors.
- Composer draft.
- Chat/history errors.
- Optional assistant-message generation run details.
- Ollama readiness status and readiness loading state.
- Ollama models and selected model.
- Model panel visibility and model action state.
- Model Lab visibility, benchmark history, model usage rows, loading/action
  state, and error state.
- Recent job records from `list_jobs` and `job_updated` events.
- Database diagnostics modal, loading state, and error state.
- Export menu visibility and active export format.
- Command palette state, command query, active command index, command registry,
  fuzzy filtering, and disabled command reasons.
- Generation state, including `isResponding` and `respondingChatId`.
- Deleting chat state.
- Chat search panel state, query, loading state, rich results, pending message
  jump, highlighted message, and errors.

Existing stale-state guards:

- Startup chat/model loads use local `ignore` flags in effects.
- Message loading for active chat uses an effect-level `ignore` flag.
- Chat search debounces for 180 ms and uses an `ignore` flag.
- Job event handling upserts jobs by `job_id`, so stale events cannot overwrite
  unrelated jobs.
- Model benchmark terminal job events refresh Model Lab history and usage.
- Conversation summary terminal job events refresh the active chat summary only
  when the job payload `chat_id` still matches the open chat.
- Active chat memory prompt settings are loaded with active unarchived memories
  using the active-chat stale guard.
- `activeChatIdRef` guards against appending/reloading assistant messages into a
  chat that is no longer active.
- Search result jumps are scoped by `chat_id` and `message_id`, so selecting a
  result changes active chat first and scrolls only after the matching message
  has rendered.
- `respondingChatId` scopes the visible progress indicator and cancellation.

Browser preview behavior is explicit: when not running in Tauri, chat responses
and model management show limited frontend-only states instead of calling Rust.

The command palette opens with `Cmd/Ctrl+K`, focuses its search field, supports
arrow/enter keyboard selection, and closes on escape or backdrop click. Initial
enabled commands call existing handlers for new chat, chat search, model manager
open, model refresh, model selection, recommended model downloads, database
diagnostics, Model Lab, chat summaries, and active chat deletion when valid. It
also exposes Memory Inspector and active-chat export commands for Markdown,
JSON, and plain text.
Future surfaces
such as settings and folder indexing are represented as disabled commands with
visible reasons instead of placeholder business logic.

The Model Lab modal is reachable from the command palette and the model manager.
It shows installed models, last chat usage from `generation_runs`, benchmark
history from `model_benchmarks`, an active benchmark progress/cancel surface,
and the fastest measured local model by completion tokens/sec only. It does not
claim quality rankings or auto-download models.

The Conversation Summary panel is reachable from the active chat action group
and command palette. It shows the current summary, source message range, model,
version, updated time, a prompt-use toggle, manual edit/save/delete actions, and
a job-backed generate/update action. The active chat shows an explicit summary
context indicator when prompt use is enabled.

The Memory Inspector is reachable from the command palette and active chat
action group. It lists global and conversation memories, archived memories,
source message links where present, pin/archive/restore/forget controls, a
manual create/edit form, and a visible per-chat memory-use switch. Message rows
include a user-triggered `Remember` action that creates source-linked memory
only when the user confirms the form.

The sidebar search uses `search_conversations` in the Tauri desktop app. Results
show conversation title, result source, date, message count, and snippet parts
with highlighted matches. Selecting a message result opens the conversation,
scrolls to that message, and briefly outlines it. Browser preview mode keeps the
old title-only local filter.

The diagnostics command opens a focused database diagnostics modal in desktop
mode. It shows SQLite path, database/WAL/SHM sizes, journal mode, schema version,
integrity result, page stats, and core table counts. It is intentionally smaller
than the future diagnostics center planned for later chunks.

The active chat action group includes an export menu. Export rendering is
backend-owned through `export_chat`; the frontend turns the returned content
into a local browser/WebView download. No dialog plugin or additional Tauri
capability has been added.

The job status surface shows active jobs and failed jobs. Model pull jobs show
progress bytes when Ollama reports totals. Model benchmark jobs show fixed-suite
prompt progress. Conversation summary jobs show one-step summary progress. These
surfaces show readable errors and a cancel action that calls
`cancel_job(job_id)`.

The first-run readiness surface handles:

- `unavailable`: short offline copy, retry action, and expandable technical
  details when available.
- `running_without_models`: no-model copy, model manager action, and retry.
- `selected_model_missing`: missing selected-model copy and model manager action.
- `running_with_models`: no readiness notice.

## Error Handling

Backend commands return `Result<_, String>`.

Current user-visible error surfaces:

- `historyError` for chat, search-adjacent chat loading, generation, deletion,
  export, and database errors.
- `modelError` for model refresh, download, and delete errors, with expandable
  technical details where available.
- `summaryError` for summary loading, generation, editing, prompt-use toggling,
  and deletion failures.
- `memoryError` for memory loading, creation, editing, archive/restore, forget,
  source jumping, and prompt-use toggling failures.
- `chatSearchError` for search-specific failures.
- `databaseDiagnosticsError` for database diagnostics loading failures.
- Readiness notices for Ollama offline, no local models, selected model missing,
  and browser preview.
- Per-message generation details for assistant messages with associated
  `generation_run` metadata, including memory snapshots when memories were
  included in the prompt.

Ollama connection failures are normalized to:

```text
Ollama is not running. Open Ollama and try again.
```

Generation cancellation returns:

```text
Generation cancelled
```

The frontend suppresses that cancellation message in the active chat error UI.

## Fragile Areas Before Refactoring

- `src-tauri/src/lib.rs` still mixes chat/export domain types, SQLite
  repositories, chat-generation Ollama HTTP, streaming, cancellation, search,
  export, model pull/delete orchestration, and most Tauri command handlers.
- Model listing/status, jobs, and benchmarks now have partial `domain`, `app`,
  and `infra` boundaries. SQLite setup/diagnostics and search have `domain` and
  `infra` modules. Chat, export, and generation are still mostly in `lib.rs`.
- The frontend still calls many chat `invoke()` commands directly from
  `src/App.tsx`; touched model, export, jobs, diagnostics, and rich search
  commands have typed wrappers.
- The command registry is centralized in `src/App.tsx`, but it is still coupled
  to local component state and handlers until frontend feature modules exist.
- Schema setup records current user version 5, but there is not yet an
  incremental migrations directory for future versions.
- The SQLite connection is protected by one mutex, so long database work would
  block other database operations.
- Database diagnostics are a focused modal, not the full diagnostics center
  planned for later chunks.
- Chat generation still sends the full conversation every time. The only prompt
  assembly behavior today is optional user-enabled summary and memory system
  context; there is no broader context diagnostics surface yet.
- The generation task registry is in-memory and keyed only by chat ID.
- Cancellation depends on checking a flag between blocking stream reads.
- Failed runs with no assistant text are persisted but only surface as inline
  `historyError` in the current UI.
- Model delete still has no progress or cancellation path.
- `search_chats` remains `LIKE`-based for compatibility, while the primary
  sidebar search now uses FTS5.
- Model Lab reports speed and latency only; it does not evaluate quality.
- Raw error strings are shown directly in most UI surfaces.

## Verification Notes

The baseline, first-run, and message-diagnostics chunks intentionally preserve
existing chat, search, and model lifecycle command names. Chunk 2 adds the
repeatable `generation_runs` schema extension. Chunk 4 adds `export_chat`
without adding a dialog plugin or changing Tauri permissions. Chunk 5 starts the
backend service-layer migration with model listing/status and validation only;
command names and serialized model/status fields stay unchanged.
Chunk 6 adds the persistent `jobs` table, `job_updated` events, `list_jobs`,
`cancel_job`, and a streamed `model_pull` job behind `download_ollama_model`.
Chat generation remains on its existing cancellation path for now.
Chunk 7 moves SQLite setup into `infra::sqlite`, treats the current schema as
baseline user version 1, enables WAL, keeps indexes scoped to current query
paths, and adds a read-only database diagnostics command plus command-palette
modal.
Chunk 8 adds FTS5-backed `chat_search` and `message_search` tables, idempotent
backfill/sync triggers, `search_conversations`, highlighted snippet parts, and
direct message jumps from sidebar search results.
Chunk 9 adds the persistent `model_benchmarks` table, benchmark service and
job-backed runner, `start_model_benchmark`, `list_model_benchmarks`,
`list_model_usage`, and a Model Lab modal for installed models, measured speed,
latency, history, and cancellation.
Chunk 10 adds the persistent `conversation_summaries` table, manual summary
save/edit/delete/toggle commands, a cancellable `conversation_summary` job,
optional user-visible prompt inclusion, and a Conversation Summary panel.
Chunk 11 adds the persistent `memories`, `memory_prompt_settings`, and
`generation_memory_uses` tables, manual Memory Inspector CRUD, per-chat memory
prompt toggles, explicit source-message memory creation, and memory-use
snapshots in assistant message diagnostics.

Relevant checks:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run lint
npm run build
npm run tauri:build
```
