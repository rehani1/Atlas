# Changelog

## v1.1.0 - Atlas Local Workspace Upgrade

### Added

- Diagnostics Center for app version, Ollama state, jobs, SQLite health,
  knowledge counts, model speed history, and recent backend errors.
- Model Lab MVP for local model benchmarking and benchmark history.
- Persistent job system for long-running work such as model downloads,
  knowledge indexing, benchmarks, and conversation summaries.
- Command palette for faster desktop navigation.
- Chat export in Markdown, JSON, and plain text formats.
- Conversation summaries with per-chat prompt inclusion controls.
- Transparent memory MVP with create, edit, archive, restore, forget, pin, and
  source-link flows.
- Local Knowledge Workspace MVP for approved text/code files.
- Context assembly diagnostics for summaries, memories, prior messages,
  document chunks, model options, and truncation notices.
- Permissioned local tool-call surfaces for read-limited Atlas tools.

### Improved

- First-run, empty, loading, and error states across local model and chat flows.
- SQLite durability, indexing, diagnostics, and local schema coverage.
- Search quality with FTS-backed chat, message, and knowledge results where
  implemented.
- Rust backend organization around service-layer boundaries for models, jobs,
  search, diagnostics, benchmarks, summaries, memories, knowledge, context, and
  tools.
- Local model diagnostics with timing, token, and speed metrics when Ollama
  returns generation metadata.
- Safe chat switching and cancellation behavior during in-progress generation.

### Not Included Yet

- Frontend token streaming from Rust events.
- Embeddings, vector search, or hybrid semantic retrieval.
- Automatic file watching and re-indexing.
- PDF/DOCX parsing.
- Cloud sync, hosted model providers, mobile support, voice mode, multi-agent
  workflows, or plugin marketplace.
- Arbitrary shell execution.

## v1.0.0 - Initial Stable Release

### Added

- Tauri desktop shell for Atlas.
- React, Vite, and Tailwind CSS frontend.
- Collapsible Atlas sidebar with ship-wheel branding.
- SQLite chat storage with chat summaries and message history.
- Chat search across titles and saved message content.
- Local Ollama model listing, selection, download, and deletion.
- Recommended model downloads for `llama3.2:1b` and `llama3.2:3b`.
- Ollama-backed assistant responses saved into chat history.
- In-progress response dots scoped to the active chat.
- Cancel button for in-progress responses.
- Chat deletion with SQLite cascade cleanup.
- Layout overflow protections for desktop Tauri windows.

### Cleaned

- Removed unused Tauri log plugin dependency and debug plugin setup.
- Removed unused Ollama model timestamp fields from app data models.
- Removed generated placeholder metadata comments.
