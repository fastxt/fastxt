# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Fastxt is a decentralized, cross-platform application for saving and syncing text in a local SQLite database without going through any centralized service. It uses on-device AI to help users organize their text — privately, without sending data to the cloud. It is a sister project to [Local Native](https://localnative.app) with similar design and toolchain.

**License**: AGPL-3.0

## Repository Layout

```
fastxt-rs/                    # Rust workspace (core library + apps)
├── fastxt_core/              # Everything: notes, search, vectors, AI, sync
│   └── src/
│       ├── store.rs          # Fastxt typed handle: CRUD, settings, sync merge
│       ├── schema.rs         # DDL + migrations (v0.6: HLC stamps, tombstones)
│       ├── search.rs         # FTS5 trigram text search, LIKE fallback, hybrid RRF
│       ├── vector.rs         # Embeddings: per-model sqlite-vec index, semantic search
│       ├── ai/               # AiBackend transports (ollama, openai_compatible, mock)
│       │                       + orchestrator: prompts, voting, batch jobs
│       ├── sync/             # tarpc service, TLS server, pairing client
│       ├── pairing.rs        # FASTXT1:host:port:<cert-fingerprint>:<token> codes
│       ├── clock.rs          # Hybrid logical clock stamps for merge
│       ├── tags.rs           # Tag normalization (commas only; multi-word tags)
│       ├── json.rs           # JSON command dispatcher (mobile FFI contract)
│       ├── ffi.rs            # C ABI: fastxt_run / fastxt_free / fastxt_set_db_dir
│       └── error.rs          # thiserror; nothing in core panics on bad input
├── fastxt_cli/               # fastxt-server (pairing code), fastxt-sync (client)
├── fastxt_desktop/           # Iced GUI (app.rs view, commands.rs typed logic)
├── fastxt_mcp/               # MCP server: notes + AI + agent-memory tools
└── fastxt_ffi/               # staticlib (iOS) + cdylib (Android/JNI), include/fastxt.h

fastxt-android/               # Android app (Kotlin, AGP 9, JNI to fastxt_ffi)
fastxt-ios/                   # iOS app (SwiftUI, Fastxt.xcframework from fastxt_ffi)
website/                      # Docusaurus website (fastxt.app)
archive/                      # Dormant prototypes (fastxt-mac, fastxt-win, fastxt-flutter) — not built
script/                       # build-android-libs.sh, build-ios-libs.sh
```

## Build Commands

### Rust workspace

```bash
cd fastxt-rs
cargo build --release
cargo test --workspace                              # no AI deps
cargo test --workspace --features fastxt_core/ai    # with AI backends
cargo clippy --workspace --all-targets --features fastxt_core/ai -- -D warnings
cargo run -p fastxt_cli --bin fastxt-server          # sync server + pairing code
cargo run -p fastxt_cli --bin fastxt-sync -- FASTXT1:192.168.1.5:3456:…:…
cargo run -p fastxt_desktop --release                # desktop GUI
cargo run -p fastxt_mcp                              # MCP server (stdio)
```

### Desktop releases (CI)

Plain `vX.Y.Z` tags build .dmg/.msi/.zip/.deb/.tar.gz via `.github/workflows/release.yml`.

### Mobile

```bash
script/build-android-libs.sh   # cargo-ndk → fastxt-android/app/src/main/jniLibs
cd fastxt-android && ./gradlew assembleDebug        # needs JDK 17

script/build-ios-libs.sh       # staticlibs + lipo → fastxt-ios/Fastxt.xcframework
cd fastxt-ios && xcodebuild -project Fastxt.xcodeproj -scheme Fastxt \
  -destination 'generic/platform=iOS Simulator' build
```

### Website

```bash
cd website && npm install && npm start
```

## Architecture

### Layers

- **`fastxt_core::store::Fastxt`** is the typed API every client uses (desktop, MCP, CLI). It owns one SQLite connection (WAL), the device clock and migrations, and returns `Result` — never panics on bad input.
- **`fastxt_core::json`** is the JSON command protocol used by the mobile apps over the FFI. Responses are serde-built; errors are `{"error": ...}`. Keep it backwards compatible with the Swift/Kotlin callers.
- **Desktop** (`fastxt_desktop`) calls the typed API from background threads; `commands.rs` has no GUI types, `app.rs` has no core types.

### Sync (protocol v2)

- TLS with a per-session self-signed server certificate; the client pins its fingerprint from the **pairing code** (`FASTXT1:host:port:<16 hex>:<10 base32>`). A session token authorizes every RPC; there is no remote stop.
- Each note has two hybrid-logical-clock stamps: `updated_at` (text/tags) and `ai_updated_at` (AI fields). Merge is per group, newest wins; deletes are tombstones and propagate.
- `PROTOCOL_VERSION` in `sync/mod.rs` is independent of the schema version.
- Sync server: `sync::server::serve(db, port)` (also a process-global server for the JSON API). Client: `sync::client::sync(code, db)`.

### Database

- Migrations in `schema.rs` run in one transaction; pre-0.6 DBs are rebuilt in place (FTS rebuilt, embeddings re-keyed by UUID, legacy stamps backfilled from `created_at`).
- `note` holds text/tags + AI fields + stamps + `deleted`. `embedding` holds one vector per (note, model) with its dim and note version; `vec_<dim>` sqlite-vec tables (cosine, partition-keyed by model) are rebuilt from it.
- FTS5 trigram index covers txt/tags/ai_tags (CJK-capable; short terms fall back to LIKE).

### AI

- Settings live in the DB `meta` table (`Settings`), shared by all clients.
- `AiBackend` = transport; `Ai` orchestrator = prompts, JSON-schema output, majority voting, model selection. Default backend `ollama`; `llamacpp`/`foundry-local`/`openai` share the OpenAI-compatible transport.
- Chat model (tags/summaries/categories) and embedding model are separate settings.
- Without the `ai` feature, HTTP backends are absent and `Ai::check()` reports unavailable.

## Conventions

- Conventional commits (`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`). No Co-Author trailers.
- `cargo clippy` and `cargo fmt` before committing; run `cargo test --workspace` (both feature sets when touching core) and fix all errors before reporting completion.
- Never edit Rust files with sed — use the Edit tool.
- Keep the no-leak policy: this repo is public; do not reference closed-source sibling projects by name.

## Key References

- **localnative/** — sister app with the same JSON/FFI and sync lineage
- **docs/TODO.md** — phase tracker (only end-to-end-done items ticked)
- **docs/on-device-ai-plan.md** — original design doc (the deferred roadmap now lives in the review tracker)
