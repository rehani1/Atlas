# Atlas

Atlas is a local-first desktop chat app built with Tauri, Rust, React, Vite,
Tailwind CSS, SQLite, and Ollama.

Version `1.0.0` provides a barebones interface for running local
models, keeping chat history on the device, and managing downloaded Ollama
models without a hosted backend.

## Demo

<p align="center">
  <img src="atlas-ai/src/assets/atlas-demo.gif" alt="Atlas demo" width="900">
</p>

## Features

- Local chat interface with a fixed sidebar, collapsible navigation, and a
  bottom composer.
- SQLite-backed chat history stored in the Tauri app data directory.
- Search across saved chat titles and message content.
- Local Ollama model selection, refresh, download, and delete controls.
- Recommended model downloads for `llama3.2:1b` and `llama3.2:3b`.
- Chat deletion that removes the conversation and its messages from SQLite.
- Streaming Ollama responses with visible progress dots in the active chat.
- Cancel control for stopping an in-progress model response.
- Safe pending-response behavior when switching chats during generation.
- Dark Tailwind UI with overflow guards for narrow Tauri windows.

## Requirements

- Node.js and npm.
- Rust with Cargo.
- Tauri desktop prerequisites for the target operating system.
- Ollama running locally on `127.0.0.1:11434`.

## Development

Install dependencies:

```bash
npm install
```

Run the Vite frontend:

```bash
npm run dev
```

Run the Tauri desktop app:

```bash
npm run tauri:dev
```

Build the frontend:

```bash
npm run build
```

Build the desktop app:

```bash
npm run tauri:build
```

Run checks:

```bash
npm run lint
cargo check --manifest-path src-tauri/Cargo.toml
```

## Architecture

The frontend lives in `src/App.tsx`. It owns the visible UI state, including
the sidebar, active chat, message list, model panel, composer, search input,
and in-progress response state.

The Rust backend lives in `src-tauri/src/lib.rs`. It exposes Tauri commands for
chat history, search, model management, response generation, and cancellation.

SQLite is initialized at startup as `atlas.sqlite3` in the Tauri app data
directory. The database contains `chats` and `messages` tables, with cascading
message deletion when a chat is deleted.

Ollama requests are sent to the local Ollama HTTP API. Model listing,
downloads, and deletions use local API calls. Chat responses are streamed from
`/api/chat` and saved back into SQLite as assistant messages.

## Release

This repository is prepared for Atlas `1.0.0`.

See `CHANGELOG.md` for release notes.
