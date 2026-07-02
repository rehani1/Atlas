# Atlas Current-State Architecture

Last audited: 2026-07-02

This document records the current Atlas `1.0.0` architecture before the remake
chunks start changing product behavior. It is descriptive, not the target
architecture.

## Scope

Atlas is a local-first desktop chat app built with Tauri 2, Rust, React, Vite,
Tailwind CSS, SQLite, and Ollama. The current codebase is intentionally compact:

- Frontend UI and state live in `src/App.tsx`.
- Backend state, SQLite access, Ollama access, command handlers, streaming, and
  cancellation live in `src-tauri/src/lib.rs`.
- `src-tauri/src/main.rs` only starts `atlas_lib::run()`.
- Public release docs are `README.md` and `CHANGELOG.md`.

There are no frontend feature folders, typed API wrapper modules, Rust
`commands`, `app`, `domain`, or `infra` modules, database migration files, job
tables, event envelopes, diagnostics views, document indexing, memory, export,
or model benchmark surfaces yet.

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
- Ollama model list, pull, delete, and chat requests.
- Model name validation.
- Assistant generation cancellation state.
- Tauri app setup and app-data directory creation.

React owns UI and interaction state:

- Sidebar and model panel visibility.
- Active chat and message display.
- Composer draft.
- Model selection.
- Search input/results.
- Loading and error states.
- Browser-preview fallback behavior when Tauri is unavailable.

The current Tauri command handlers are not thin IPC wrappers yet. Most business
logic is directly in `src-tauri/src/lib.rs`.

## IPC Command Contract

The backend exposes these Tauri commands:

```text
list_chats() -> Vec<ChatSummary>
search_chats(query: String) -> Vec<ChatSummary>
create_chat(title: Option<String>) -> ChatSummary
get_messages(chat_id: String) -> Vec<ChatMessage>
add_message(chat_id: String, role: String, content: String) -> ChatMessage
delete_chat(chat_id: String) -> bool
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

OllamaModel
- name: string
- size: number
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
```

Current indexes and trigger:

```sql
CREATE INDEX IF NOT EXISTS idx_chats_updated_at
  ON chats(updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_messages_chat_id_created_at
  ON messages(chat_id, created_at, id);

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
4. Registers a cancellation flag in `GenerationTasks` keyed by `chat_id`.
5. Runs blocking Ollama streaming work on Tauri's blocking runtime.
6. Calls `POST http://127.0.0.1:11434/api/chat` with `stream: true`.
7. Reads Ollama JSONL chunks and accumulates `message.content`.
8. Returns an error if cancellation is requested, the request fails, JSON cannot
   be parsed, or the final assistant content is empty.
9. Inserts one final assistant message into SQLite only after a successful full
   response.

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
  entry for that chat.
- Cancellation is scoped by chat ID, so cancelling one chat does not directly
  cancel another chat.

Current limitation: cancellation may not be immediate while a blocking read is
stalled, and partial assistant content is not persisted on cancellation.

## Model Management Flow

Atlas assumes Ollama is local at `127.0.0.1:11434`.

Current model operations:

- `list_ollama_models()` calls `GET /api/tags`.
- `download_ollama_model(model)` validates the model name, calls
  `POST /api/pull` with `{ "name": model, "stream": false }`, then refreshes
  the model list.
- `delete_ollama_model(model)` validates the model name, calls
  `DELETE /api/delete`, then refreshes the model list.

Model names reject empty strings, whitespace, double quotes, and backslashes.

Current limitations:

- Model pull is blocking from the user's perspective.
- Model pull uses `stream: false`, so there is no progress reporting.
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
- Ollama models and selected model.
- Model panel visibility and model action state.
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

## Error Handling

Backend commands return `Result<_, String>`.

Current user-visible error surfaces:

- `historyError` for chat, search-adjacent chat loading, generation, deletion,
  and database errors.
- `modelError` for model list, refresh, download, and delete errors.
- `chatSearchError` for search-specific failures.

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

- `src-tauri/src/lib.rs` mixes domain types, SQLite setup, repositories, Ollama
  HTTP, streaming, cancellation, and Tauri command handlers.
- The frontend calls `invoke()` directly from `src/App.tsx`; there are no typed
  API wrappers.
- The schema is inline and repeatable, but not versioned.
- The SQLite connection is protected by one mutex, so long database work would
  block other database operations.
- Chat generation sends the full conversation every time; there is no prompt
  assembly layer or context diagnostics.
- The generation task registry is in-memory and keyed only by chat ID.
- Cancellation depends on checking a flag between blocking stream reads.
- No final Ollama metadata is parsed or stored.
- Partial assistant output is dropped on cancellation or failure.
- Model downloads have no progress or cancellation path.
- Search uses `LIKE`, so result quality and scalability are limited.
- Raw error strings are shown directly in most UI surfaces.

## Chunk 0 Verification Notes

This chunk adds documentation, low-risk unit coverage, and generated-output
lint hygiene only. It intentionally does not change product behavior, command
names, serialized fields, schema, or Tauri permissions.

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
