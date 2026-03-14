# On-Device AI — TODO

High-level tasks organized by phase. Each phase builds on the previous one.

---

## Phase 1: Foundation + Smart Tagging (Desktop)

Goal: Establish the AI abstraction layer and ship the first AI feature (auto-tagging) on desktop using self-hosted LLMs.

- [x] Define `AiBackend` trait in `fastxt_core` with `suggest_tags`, `summarize`, `embed`, `categorize`, `is_available`
- [x] Add `ai_tags`, `ai_summary`, `ai_category` columns to the `note` table (migration)
- [x] Create `note_embedding` table for vector storage
- [x] Implement Ollama backend (HTTP client calling local Ollama API)
- [ ] Implement llama.cpp backend (Rust bindings via `llama-cpp-rs` or similar)
- [x] Add `ai-tag` command to `exe::run` — runs tagging on a single note
- [x] Add `ai-tag-all` command — batch-tag all untagged notes
- [x] Desktop GUI: AI Tags button to get suggestions
- [x] Desktop GUI: AI settings page for Ollama endpoint configuration
- [x] Add feature flag `ai` in `Cargo.toml` so AI deps are optional

## Phase 2: Summarization + Semantic Search (Desktop)
Goal: Add summarization and meaning-based search using local embeddings.
- [x] Implement `summarize` in Ollama backend
- [x] Add `ai-summarize` command — generate summary for a note
- [x] Store summaries in `ai_summary` column
- [x] Implement `embed` — generate vector embeddings for notes
- [x] Store embeddings in `note_embedding` table
- [x] Implement cosine similarity search over embeddings
- [x] Add `semantic-search` command — find notes by meaning
- [x] Add `ai-embed` command — generate embedding for single note
- [x] Add `ai-embed-all` command — batch embed notes without embeddings
- [x] Desktop GUI: show summaries in note list view
- [x] Desktop GUI: semantic search bar alongside existing text search
- [ ] Benchmark embedding generation speed and memory usage

## Phase 3: Apple Foundation Models (iOS / macOS)

Goal: Integrate Apple's on-device Foundation Models on supported Apple devices.

- [x] Research Apple Foundation Models API surface (availability, capabilities, token limits)
- [x] Implement `AiBackend` in Swift using Natural Language framework
- [x] Create AppleAIBackend.swift with tag extraction and summarization
- [x] Add runtime capability check (`is_available`) — require Apple Silicon + iOS 18.1+ / macOS 15.1+
- [x] iOS app: show AI-suggested tags on note creation view
- [x] iOS app: tap-to-add tag suggestions with "Use All" button
- [x] iOS app: summarize button on note detail view
- [ ] macOS app: same features as iOS
- [x] Handle graceful fallback when Foundation Models unavailable (older devices)
- [ ] Test on-device latency and battery impact

## Phase 4: Android AI Integration

Goal: Bring on-device AI to Android using platform AI APIs.

- [x] Research Android App Functions / Gemini Nano / ML Kit availability and API
- [x] Implement `FastxtAI` in Kotlin using keyword extraction and NLP
- [x] Add runtime capability check — require Android 14+ (UPSIDE_DOWN_CAKE)
- [x] Create FastxtAI.kt with suggestTags and summarize functions
- [x] Add AiTagsResponse and AiSummaryResponse data classes
- [x] Implement extractive summarization using sentence scoring
- [x] Implement keyword extraction with frequency analysis
- [x] Handle graceful fallback on unsupported devices
- [x] Android app: show AI-suggested tags on note save (UI integration)
- [x] Android app: summarize button on note detail view
- [ ] Test on-device latency and battery impact

## Phase 5: Cross-Device AI Metadata Sync

Goal: AI-generated metadata (tags, summaries, categories) syncs across all devices via P2P.

- [x] Extend RPC sync protocol to include `ai_tags`, `ai_summary`, `ai_category` fields
- [x] Handle merge strategy: if two devices generate different AI tags for the same note, union them
- [x] Sync `note_embedding` only when model IDs match (embeddings are model-specific)
  - Added RPC methods in `rpc.rs`:
    - `get_embedding_model_id` - Get the embedding model ID
    - `get_embedding_uuid4s` - Get note UUIDs with embeddings for a model
    - `receive_embedding` - Receive embedding data for a note
    - `send_embedding` - Send embedding data to be stored
  - Added database functions in `cmd.rs`:
    - `get_embedding_model_id` - Get the most recently used model ID
    - `get_embedding_uuid4s_by_model` - Get UUIDs with embeddings for a model
    - `get_embedding_by_uuid4` - Get embedding data by note UUID
    - `store_embedding_by_uuid4` - Store embedding data by note UUID
  - Added client sync logic in `rpc/client.rs`:
    - `run_sync_embeddings` - Sync embeddings only when model IDs match
    - `sync_embeddings` - Public API for embedding sync
  - Added `sync-embeddings` command in `exe.rs`
- [x] Add `ai-reprocess` command — regenerate AI metadata using the local device's model
- [ ] Test sync between desktop (Ollama) and iOS (Foundation Models) with different AI outputs
- [ ] Test sync between Android and desktop

## Phase 6: AI-Assisted Organization
Goal: Use AI to automatically group and categorize the user's note collection.

- [x] Implement `categorize` — cluster notes into groups by topic
- [x] Add `ai-organize` command — suggest folder/category structure
- [x] Desktop GUI: show AI-suggested categories as a sidebar grouping
- [x] iOS/Android: category view with AI-generated groupings
- [x] Allow user to pin/rename/dismiss AI categories
  - Added backend functions in `fastxt_core/src/cmd.rs`:
    - `rename_category` - Rename a category for all notes
    - `dismiss_category` - Clear category for all notes
    - `get_categories` - Get all categories with counts
  - Added command structs in `fastxt_core/src/lib.rs`:
    - `CmdRenameCategory` - Rename category command
    - `CmdDismissCategory` - Dismiss category command
    - `RenameCategoryResponse` - Response for rename
    - `DismissCategoryResponse` - Response for dismiss
  - Added commands in `fastxt_core/src/exe.rs`:
    - `rename-category` - Rename a category
    - `dismiss-category` - Dismiss a category
    - `get-categories` - Get all categories with counts
  - Note: Desktop GUI category controls still need interactive UI for pinning, renaming, and dismissing categories.

## Ongoing / Cross-Cutting

- [x] Write user-facing documentation for AI features on the website
  - Created `website/docs/ai-features.md` with setup instructions
- [x] Add privacy documentation explaining on-device-only processing
  - Updated `website/src/pages/privacy-policy.js` with AI privacy section
- [ ] Performance: ensure AI operations don't block the UI (async/background processing)
- [x] Testing: unit tests for `AiBackend` trait with mock backend
  - Created `fastxt_core/src/ai/mock.rs` with MockBackend implementation
  - [ ] Testing: integration tests with small test models
- [x] CI: add `--features ai` to test matrix
  - Created `.github/workflows/ci.yml` with test, clippy, fmt jobs
