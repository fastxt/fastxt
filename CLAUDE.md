# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Fastxt is a decentralized, cross-platform application for saving and syncing text in a local SQLite database without going through any centralized service. It leverages native on-device AI models to help users organize their text information — privately, without sending data to the cloud. It is a sister project to [Local Native](https://localnative.app) with similar design and toolchain.

**License**: AGPL-3.0

## On-Device AI Vision

Fastxt integrates native AI capabilities on each platform to help users organize, tag, summarize, and search their text notes — all processed locally on-device:

- **Apple (iOS/macOS)**: Apple Foundation Models via the Foundation framework — on-device language understanding, summarization, and smart organization
- **Android**: Android App Functions / on-device ML models (e.g., Gemini Nano) for text intelligence
- **Desktop (Linux/Windows)**: Self-hosted LLMs (e.g., llama.cpp, Ollama) for local inference without cloud dependency

### Key AI Features (Roadmap)
- **Smart Tagging**: Auto-suggest tags for saved text based on content
- **Semantic Search**: Find notes by meaning, not just keyword matching
- **Summarization**: Condense long text entries into concise summaries
- **Organization**: AI-assisted grouping and categorization of notes
- **Cross-Device Sync**: AI-generated metadata syncs alongside text via the existing P2P RPC protocol

## Repository Layout

```
fastxt-rs/                    # Rust workspace (core library + apps)
├── fastxt_core/              # Shared core: SQLite, RPC sync, FFI for mobile
│   └── src/
│       ├── lib.rs            # FFI exports (fastxt_run, fastxt_free)
│       ├── exe.rs            # Command dispatcher and database logic
│       ├── cmd/              # CRUD operations, search, sync
│       └── rpc/              # tarpc-based P2P sync protocol
├── fastxt_cli/               # CLI binaries for RPC server/client
└── fastxt_desktop/           # Desktop GUI (Druid)

fastxt-android/               # Native Android app (Kotlin)
fastxt-flutter/               # Flutter app (early stage)
website/                      # Docusaurus website (fastxt.app)
```

## Build Commands

### Rust Core & Desktop
```bash
cd fastxt-rs

# Build all workspace members
cargo build --release

# Run tests
cargo test --workspace

# Run desktop GUI
cargo run -p fastxt_desktop --release

# Run CLI tools
cargo run -p fastxt_cli --bin fastxt-rpc-server
cargo run -p fastxt_cli --bin fastxt-rpc-client-sync
cargo run -p fastxt_cli --bin fastxt-rpc-client-stop-server
cargo run -p fastxt_cli --bin fastxt-upgrade
```

### Website (Docusaurus)
```bash
cd website
npm install
npm start        # Development server
npm run build    # Production build
```

### Flutter App
```bash
cd fastxt-flutter
flutter pub get
flutter run
```

### Android App
Open `fastxt-android/` in Android Studio or run:
```bash
cd fastxt-android
./gradlew assembleDebug
```

## Architecture

### Core Data Flow
- All commands pass through `exe::run(text: &str)` which parses JSON input
- Commands: `select`, `search`, `insert`, `delete`, `server`, `server-addr`, `client-sync`, `client-stop-server`
- Database: SQLite with `note` table (rowid, uuid4, txt, tags, created_at) and `meta` table

### FFI Bridge
- `fastxt_run(json_input: *const c_char) -> *mut c_char` - Main entry point for mobile FFI
- `fastxt_free(s: *mut c_char)` - Free allocated strings from FFI calls

### RPC Sync Protocol
- Uses tarpc for async RPC between Fastxt instances
- Services: `is_version_match`, `diff_uuid4_to_server`, `diff_uuid4_from_server`, `send_note`, `receive_note`, `stop`
- Default server port: 3456

### Database Location
- macOS: `~/Library/Containers/app.fastxt.Fastxt/Data/Fastxt/fastxt.sqlite3`
- Android: `/sdcard/Fastxt/fastxt.sqlite3`
- iOS: `~/Documents/fastxt.sqlite3`
- Desktop/Linux: `~/Fastxt/fastxt.sqlite3`

## Related Projects

- **Local Native** (`/Users/yi/repos/murky-swamp/localnative`) - Similar local-first notes app with P2P sync. Fastxt shares similar architecture patterns but is focused on text notes.
