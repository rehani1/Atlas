# Atlas Current-State Architecture

Last audited: 2026-07-02

This document records the current Atlas `1.0.0` architecture before the remake
chunks start changing product behavior. It is descriptive, not the target
architecture.

## Scope

Atlas is a local-first desktop chat app built with Tauri 2, Rust, React, Vite,
Tailwind CSS, SQLite, and Ollama. The current codebase is intentionally compact:

- Frontend UI and state live in `src/App.tsx`.
- Most backend state, SQLite access, command handlers, streaming, and
  cancellation still live in `src-tauri/src/lib.rs`.
- The first backend service slice is split across `src-tauri/src/domain/model.rs`,
  `src-tauri/src/app/models.rs`, and `src-tauri/src/infra/ollama.rs`.
- `src-tauri/src/main.rs` only starts `atlas_lib::run()`.
- Public release docs are `README.md` and `CHANGELOG.md`.

There is a minimal typed frontend API wrapper for touched Ollama status and
model lifecycle commands in `src/shared/api/tauri.ts`. There are no frontend
feature folders, Rust `commands` module, database migration files, job tables,
event envelopes, diagnostics views, document indexing, memory, import, or model
benchmark surfaces yet.

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

- SQLite connection and queries.
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
create_chat(title: Option<String>) -> ChatSummary
get_messages(chat_id: String) -> Vec<ChatMessage>
add_message(chat_id: String, role: String, content: String) -> ChatMessage
delete_chat(chat_id: String) -> bool
export_chat(chat_id: String, format: ChatExportFormat) -> ChatExport
get_ollama_status(selected_model: Option<String>) -> OllamaStatus
list_ollama_models() -> Vec<OllamaModel>
download_ollama_model(model: String) -> Vec<OllamaModel>
delete_ollama_model(model: String) -> Vec<OllamaModel>
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
```

There are no backend-to-frontend token events or job progress events yet.

## SQLite Persistence

SQLite is opened during Tauri setup at:

```text
app.path().app_data_dir()/atlas.sqlite3
```

The app creates the app-data directory if needed. Schema setup is inline in
`ChatStore::new()` with `CREATE TABLE IF NOT EXISTS`; there is no migration
system yet.

Foreign keys are enabled with:

```sql
PRAGMA foreign_keys = ON
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
```

Current indexes and trigger:

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

CREATE TRIGGER IF NOT EXISTS messages_after_insert_update_chat
AFTER INSERT ON messages
BEGIN
  UPDATE chats
  SET updated_at = NEW.created_at
  WHERE id = NEW.chat_id;
END;
```

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
- `export_chat` reads the chat, messages, and joined generation metadata,
  renders Markdown, JSON, or plain text in Rust, returns content with a
  sanitized filename and MIME type, and does not mutate SQLite.
- WAL mode is not configured yet.

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
8. When the command resolves, the frontend appends the assistant message only if
   the active chat still matches the generating chat.
9. The frontend reloads messages and refreshes chat summaries.

The backend generation path:

1. Validates the model name.
2. Loads all messages for the chat from SQLite.
3. Converts them to Ollama chat messages.
4. Creates a `generation_runs` row with status `running`.
5. Registers a cancellation flag in `GenerationTasks` keyed by `chat_id`.
6. Runs blocking Ollama streaming work on Tauri's blocking runtime.
7. Calls `POST http://127.0.0.1:11434/api/chat` with `stream: true`.
8. Reads Ollama JSONL chunks, accumulates `message.content`, tracks first
   non-empty token time, and captures optional final Ollama metadata:
   `total_duration`, `load_duration`, `prompt_eval_count`,
   `prompt_eval_duration`, `eval_count`, and `eval_duration`.
9. Converts Ollama nanosecond durations into rounded milliseconds and calculates
   tokens/sec from `eval_count / eval_duration`.
10. Inserts an assistant message and associates it with the generation run when
    final or partial assistant text exists.
11. Finalizes the generation run as `completed`, `cancelled`, or `failed`.

Important current limitation: chat is streamed from Ollama to Rust, but not
token-streamed from Rust to React. React shows animated progress dots while
waiting and receives one final `ChatMessage` after completion.

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
  `POST /api/pull` with `{ "name": model, "stream": false }`, then refreshes
  the model list.
- `delete_ollama_model(model)` validates the model name, calls
  `DELETE /api/delete`, then refreshes the model list.

Model names reject empty strings, whitespace, double quotes, and backslashes.
An empty Ollama model list is represented as `running_without_models`, not a
backend exception.

Current limitations:

- Model pull is blocking from the user's perspective.
- Model pull uses `stream: false`, so there is no progress reporting.
- Model pull/delete still have command-level orchestration in `lib.rs`; they
  share extracted validation and Ollama HTTP infra, but are not fully service
  commands yet.
- There is no persistent job record for downloads or deletes.

## Search Behavior

`search_chats(query)`:

- Trims the query.
- Returns `list_chats()` for an empty query.
- Escapes `%`, `_`, and `\` for `LIKE`.
- Searches `LOWER(chats.title)` and `LOWER(messages.content)`.
- Returns `ChatSummary` objects only.
- Orders results by `updated_at DESC`.

Current limitations:

- There is no SQLite FTS table.
- Results do not include message IDs, snippets, ranks, highlighted ranges,
  filters, or direct jumps to matching messages.

## Frontend State Ownership

`src/App.tsx` owns all visible UI and app state:

- Sidebar open/closed state.
- Chat summaries and active chat ID.
- Current chat messages.
- Composer draft.
- Chat/history errors.
- Optional assistant-message generation run details.
- Ollama readiness status and readiness loading state.
- Ollama models and selected model.
- Model panel visibility and model action state.
- Export menu visibility and active export format.
- Command palette state, command query, active command index, command registry,
  fuzzy filtering, and disabled command reasons.
- Generation state, including `isResponding` and `respondingChatId`.
- Deleting chat state.
- Chat search panel state, query, loading state, results, and errors.

Existing stale-state guards:

- Startup chat/model loads use local `ignore` flags in effects.
- Message loading for active chat uses an effect-level `ignore` flag.
- Chat search debounces for 180 ms and uses an `ignore` flag.
- `activeChatIdRef` guards against appending/reloading assistant messages into a
  chat that is no longer active.
- `respondingChatId` scopes the visible progress indicator and cancellation.

Browser preview behavior is explicit: when not running in Tauri, chat responses
and model management show limited frontend-only states instead of calling Rust.

The command palette opens with `Cmd/Ctrl+K`, focuses its search field, supports
arrow/enter keyboard selection, and closes on escape or backdrop click. Initial
enabled commands call existing handlers for new chat, chat search, model manager
open, model refresh, model selection, recommended model downloads, and active
chat deletion when valid. It also exposes active-chat export commands for
Markdown, JSON, and plain text. Future surfaces such as settings, diagnostics,
Model Lab, and folder indexing are represented as disabled commands with visible
reasons instead of placeholder business logic.

The active chat action group includes an export menu. Export rendering is
backend-owned through `export_chat`; the frontend turns the returned content
into a local browser/WebView download. No dialog plugin or additional Tauri
capability has been added.

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
- `chatSearchError` for search-specific failures.
- Readiness notices for Ollama offline, no local models, selected model missing,
  and browser preview.
- Per-message generation details for assistant messages with associated
  `generation_run` metadata.

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

- `src-tauri/src/lib.rs` still mixes chat/export domain types, SQLite setup,
  repositories, chat-generation Ollama HTTP, streaming, cancellation, search,
  export, model pull/delete orchestration, and most Tauri command handlers.
- Only the model listing/status slice currently has `domain`, `app`, and
  `infra` boundaries.
- The frontend still calls many chat/search `invoke()` commands directly from
  `src/App.tsx`; only the touched Ollama/model commands have typed wrappers.
- The command registry is centralized in `src/App.tsx`, but it is still coupled
  to local component state and handlers until frontend feature modules exist.
- The schema is inline and repeatable, but not versioned.
- The SQLite connection is protected by one mutex, so long database work would
  block other database operations.
- Chat generation sends the full conversation every time; there is no prompt
  assembly layer or context diagnostics.
- The generation task registry is in-memory and keyed only by chat ID.
- Cancellation depends on checking a flag between blocking stream reads.
- Failed runs with no assistant text are persisted but only surface as inline
  `historyError` in the current UI.
- Model downloads have no progress or cancellation path.
- Search uses `LIKE`, so result quality and scalability are limited.
- Raw error strings are shown directly in most UI surfaces.

## Verification Notes

The baseline, first-run, and message-diagnostics chunks intentionally preserve
existing chat, search, and model lifecycle command names. Chunk 2 adds the
repeatable `generation_runs` schema extension. Chunk 4 adds `export_chat`
without adding a dialog plugin or changing Tauri permissions. Chunk 5 starts the
backend service-layer migration with model listing/status and validation only;
command names and serialized model/status fields stay unchanged.

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
