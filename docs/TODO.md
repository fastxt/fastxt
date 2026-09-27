# On-Device AI — TODO

Status as of the v0.6.0 core rewrite (2026-09). Only items that are done end-to-end (implemented, wired into a shipped UI, tested) are ticked.

---

## Phase 1: Foundation + Smart Tagging (Desktop)

- [x] `AiBackend` transports (Ollama, OpenAI-compatible: llama.cpp / Foundry Local / any server) with prompts, structured output and self-consistency voting in one orchestrator (`fastxt_core::ai`)
- [x] `ai_tags`, `ai_summary`, `ai_category` columns on `note`; `embedding` table (per note + model, with dim and note version)
- [x] Settings persisted in the database (`backend`, `endpoint`, chat model, embedding model, timeout, voting rounds) and shared by desktop, CLI and MCP
- [x] Backend chosen from settings; "Test connection" probes the configured endpoint and lists missing models
- [x] Tag prompts reuse your existing tag vocabulary
- [x] Desktop GUI: AI Tags button, AI settings page, persisted settings
- [x] Batch tagging (`ai-tag-all`) with progress and cancel

## Phase 2: Summarization + Semantic Search (Desktop)

- [x] Summarize text without saving a note (`ai-summarize`); saving keeps the summary with the note
- [x] Embeddings per (note, model) with the producing model and note version recorded; stale embeddings are dropped on edit/delete
- [x] Vector index sized from the real embedding (any dim), cosine distance, partitioned by model — sqlite-vec knn queries with `k = ?`
- [x] `semantic-search` and `hybrid-search` (FTS5 trigram + vectors, RRF fused); text search uses the FTS index (CJK-capable), LIKE fallback for short terms
- [x] Desktop GUI: summaries on cards, semantic search toggle with graceful degradation to text search
- [ ] Benchmark embedding generation speed and memory usage

## Phase 3: Apple (iOS / iPadOS)

- [x] `fastxt_ffi` static lib + XCFramework; DB in app Documents; no panics across FFI
- [x] iOS app builds on modern Xcode (all sources in target; deduplicated models)
- [x] Tag suggestions: Apple Foundation Models on iOS 26+ where available, NL heuristics elsewhere (NLTagger scheme bug fixed; CJK bigrams)
- [x] Summaries: same fallback chain
- [x] Sync UI: pairing-code server and client (QR + text)
- [ ] macOS app (SwiftUI port of desktop AI) — desktop macOS uses the Rust app + Ollama
- [ ] Test on-device latency and battery impact on real hardware

## Phase 4: Android

- [x] Modern toolchain (AGP 9 / Kotlin / Gradle 9 / SDK 36); `fastxt_ffi` cdylib per ABI via cargo-ndk; DB in app-private storage
- [x] JNI bridge (`fastxtRun`, `setDbDir`) actually exported by the core
- [x] Unicode-aware tag extraction (CJK bigrams) and extractive summaries
- [x] CategoryActivity registered and compiled; organize via `ai-organize`
- [ ] Gemini Nano / ML Kit GenAI generation (needs a supported device)
- [ ] Test on-device latency and battery impact on real hardware

## Phase 5: Cross-Device Sync

- [x] Protocol v2: TLS + per-session certificate pinned from the pairing code; session token on every request; no remote stop; bounded frames
- [x] Hybrid logical clocks; per-field-group merge (text/tags vs AI fields, delete wins); tombstones (deletes propagate, no resurrection)
- [x] Protocol version independent of schema version
- [x] Embeddings sync per model, only for matching note versions; set-based diff
- [x] Two-peer end-to-end tests (including bad-token and wrong-fingerprint rejection)
- [ ] Multi-peer smoke test with real devices on one network

## Phase 6: AI-Assisted Organization

- [x] Categorize with vocabulary reuse (renames stick); batch offset bug fixed
- [x] Categories with counts, rename, dismiss — desktop sidebar and MCP tools; Android category view
- [x] `ai_tags` indexed and searchable alongside text and tags
- [ ] Clustering-based organization (embeddings → LLM-named clusters; needs shared embedding model across devices — R1 in the roadmap doc)

## Ongoing / Cross-Cutting

- [x] Typed `Fastxt` API; JSON dispatcher only for the mobile FFI; no panics in core paths
- [x] Temp-DB tests for dispatcher and merge logic; two-peer sync tests
- [x] CI: Rust on Linux/macOS/Windows, Android (Rust lib + APK), iOS (XCFramework + simulator build)
- [x] Website and privacy policy match shipped behavior
- [ ] Bundled multilingual embedding model so mobile devices share one vector space (R1)
- [ ] UniFFI bindings with callbacks so mobile platforms can supply AI to the core (R3)
- [ ] iroh transport for sync beyond one LAN (R4)
