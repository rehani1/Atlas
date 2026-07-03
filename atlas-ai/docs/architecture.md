# Atlas Architecture

Last audited: 2026-07-03

Atlas `1.1.0` is a local-first desktop AI workspace built with Tauri 2, Rust,
React, Vite, Tailwind CSS, SQLite, and Ollama. The app keeps local data in the
Tauri app data directory, sends model requests to the user's local Ollama server,
and exposes privileged operations through Rust-side Tauri commands.

## Runtime Model

Atlas runs as a Tauri desktop application:

- React renders the desktop UI and owns interaction state.
- Rust owns local persistence, filesystem access, Ollama HTTP calls, job
  execution, diagnostics, and permissioned tool execution.
- SQLite stores conversations, messages, generation diagnostics, jobs,
  summaries, memories, indexed knowledge, context traces, and tool-call audit
  records.
- Ollama is expected at `127.0.0.1:11434`.
- Tauri IPC commands are the boundary between UI state and privileged local work.

There is no hosted backend required for the v1.1.0 local workspace features.

## Frontend

The frontend lives primarily in `src/App.tsx`. It owns:

- Sidebar, active chat, message list, model panel, and composer state.
- Command palette state and command routing.
- Chat search, knowledge search, memory inspector, summary panel, Model Lab, and
  Diagnostics Center UI state.
- Loading, cancellation, empty-state, and error-state presentation.
- Browser-preview fallback behavior when Tauri is unavailable.

Typed wrappers for Tauri calls live in `src/shared/api/tauri.ts`. These wrappers
use frontend camelCase keys and call Rust commands that receive snake_case
parameters through Tauri's serializer.

## Backend Organization

The backend entry point is `src-tauri/src/lib.rs`. It initializes the Tauri app,
creates the SQLite database in the app data directory, registers managed state,
and exposes the Tauri command handler list.

Newer backend slices are organized around three layers:

- `domain`: serialized types, enums, validation helpers, and app-level domain
  concepts.
- `app`: workflow logic for models, jobs, diagnostics, benchmarks, summaries,
  memories, knowledge, context assembly, and tools.
- `infra`: SQLite repositories, FTS setup, Ollama HTTP integration, and other
  local infrastructure.

Some orchestration and legacy command logic still lives in `src-tauri/src/lib.rs`.
The important boundary is that privileged operations stay on the Rust side, not
inside React.

## IPC Boundaries

The Tauri command surface is grouped around local workspace capabilities:

- Chat and messages: list, search, create, load, add, delete, generate, cancel,
  and export conversations.
- Models and Ollama: get readiness state, list local models, download a model,
  and delete a model.
- Jobs: list persisted jobs, cancel active jobs, and emit `job_updated` events
  for long-running work.
- Diagnostics: return SQLite diagnostics and aggregate Diagnostics Center data.
- Summaries: get, save, enable, disable, delete, and generate conversation
  summaries.
- Memory: list, create, update, archive, restore, delete, and control per-chat
  prompt inclusion.
- Knowledge: index an approved path, list workspaces/documents, search indexed
  chunks, load a chunk, remove a workspace, and control per-chat prompt
  inclusion.
- Benchmarks: start model benchmarks, list benchmark history, and list model
  usage.
- Tools: resolve model-requested Atlas tool calls after a user permission
  decision.

Rust command handlers return serializable DTOs rather than exposing database rows
directly to the frontend.

## SQLite Persistence

SQLite is opened during Tauri setup at:

```text
<tauri app data>/atlas.sqlite3
```

The connection enables foreign keys, WAL journaling, normal synchronous mode,
and a busy timeout. The current schema is recorded as `PRAGMA user_version = 8`.

The schema is grouped by feature area:

- Core conversation tables: `chats`, `messages`, and `generation_runs`.
- Jobs: `jobs`.
- Search: `chat_search` and `message_search` FTS tables.
- Benchmarks: `model_benchmarks`.
- Summaries: `conversation_summaries`.
- Memory: `memories`, `memory_prompt_settings`, and
  `generation_memory_uses`.
- Knowledge: `workspaces`, `documents`, `document_chunks`,
  `document_chunk_search`, `knowledge_prompt_settings`, `retrieval_runs`, and
  `generation_document_sources`.
- Context diagnostics: `generation_context_items`.
- Tool calls: `tool_calls` and `tool_permissions`.

There is not a standalone migrations directory yet. The current build creates
the current schema during setup and records the schema version.

## Jobs

Long-running work is represented by persisted `jobs` rows. Job records include a
type, status, progress fields, label, payload/result JSON, errors, and lifecycle
timestamps.

Current job-backed workflows include:

- Ollama model downloads.
- Knowledge indexing.
- Model benchmarks.
- Conversation summary generation.

The frontend subscribes to `job_updated` events and also fetches recent jobs
from SQLite. On startup, stale queued, running, or cancelling jobs from a
previous process are marked failed with an interruption message.

## Ollama Integration

Atlas assumes Ollama is local at `127.0.0.1:11434`.

Rust handles:

- Readiness checks and model list retrieval.
- Model pulls and deletes.
- Model-name validation.
- Chat requests to Ollama's local `/api/chat` endpoint.
- Generation cancellation tokens.
- Generation metrics derived from Ollama response metadata.

Ollama chat responses are streamed from Ollama to Rust, accumulated there, saved
to SQLite on completion, and returned to React as the assistant message. The
frontend shows an in-progress state while the command is running; v1.1.0 does
not stream individual tokens from Rust events into React.

## Search And Indexing

Atlas uses SQLite FTS for local search surfaces:

- `search_chats` remains a compatibility command for chat-title search.
- `search_conversations` searches chat titles and message content through FTS.
- Knowledge search uses indexed document chunks and `document_chunk_search`.

The Local Knowledge Workspace MVP accepts approved text/code files with these
extensions:

```text
.md, .txt, .json, .rs, .ts, .tsx, .js, .jsx, .py, .toml, .yaml, .yml
```

Knowledge indexing validates the path, loads supported files, chunks text with
line ranges, stores chunk metadata, and indexes chunk content into SQLite FTS.
The current implementation does not include embeddings, vector search, hybrid
semantic retrieval, automatic file watching, or PDF/DOCX parsing.

## Context Assembly

Before a model request, Rust assembles a prompt from explicit local sources:

- Atlas system prompt.
- Enabled conversation summary.
- Enabled user memories.
- Prior conversation messages within the context budget.
- Retrieved knowledge chunks when knowledge is enabled.
- Latest user message.
- Model options and truncation notices.

Context assembly records diagnostic items so the UI can show which summaries,
memories, prior messages, document chunks, model settings, and truncation states
were used for a generation. Atlas also snapshots memory and document source use
for completed generations, so later edits do not rewrite historical generation
diagnostics.

## Memory, Knowledge, And Tools

Memory is user-controlled. Atlas creates memory only when the user explicitly
adds it or uses a source-linked memory action. Memory can be edited, archived,
restored, forgotten, pinned, and enabled or disabled for prompt inclusion per
chat.

Knowledge is opt-in per indexed path and prompt inclusion is controlled per
chat. The app reads only approved supported files during indexing and stores
chunks locally in SQLite.

Permissioned local tools are read-limited in v1.1.0. The model can request an
Atlas tool call, but Rust stores it as pending and the UI asks the user to deny,
allow once, or allow for the workspace before execution. Enabled MVP tools are:

- `search_index(query)`
- `read_file_chunk(chunk_id)`
- `get_model_stats(model_name)`

Arbitrary shell execution is not included.

## Diagnostics

Diagnostics Center aggregates operational state without including prompt bodies,
message bodies, indexed file content, or memory content. It reports:

- App version and generation timestamp.
- Ollama readiness, selected model, and installed model count.
- Running and recently failed jobs.
- SQLite file size, WAL/shared-memory size, schema version, and table counts.
- Knowledge workspace/document/chunk counts.
- Chat and benchmark speed history by model.
- Recent backend errors.

Per-message diagnostics show generation metrics and prompt context traces when
available.

## Privacy Constraints

Atlas is designed around local-first defaults:

- Chat data, memories, knowledge indexes, summaries, jobs, diagnostics metadata,
  benchmarks, and tool-call audit records stay in local SQLite.
- Model inference is sent to the local Ollama server.
- No cloud sync, hosted provider integration, mobile client, voice mode,
  multi-agent workflow system, or plugin marketplace is included in v1.1.0.
- Tool execution is limited to explicit Atlas read tools and requires a user
  permission decision.

## Tooling

Common development commands:

```bash
npm install
npm run dev
npm run build
npm run lint
npm run tauri:dev
npm run tauri:build
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

`npm run build` runs TypeScript project build and Vite production build. There
is no dedicated `npm run typecheck` script.

## Known Limitations

- Frontend token-by-token streaming from Rust events is not implemented yet.
- Embeddings, vector search, and hybrid retrieval are not implemented yet.
- Automatic file watching and re-indexing are not implemented yet.
- PDF/DOCX ingestion is not implemented yet.
- The frontend is still concentrated in `src/App.tsx`.
- There is no standalone database migrations directory yet.
- Hosted model providers, cloud sync, mobile, voice, multi-agent workflows, and
  plugin marketplace support are outside v1.1.0.
