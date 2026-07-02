# Changelog

## 1.1.0 - 2026-07-02

Current local-first Atlas release.

## 1.0.0 - 2026-06-03

Initial stable release of Atlas.

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
