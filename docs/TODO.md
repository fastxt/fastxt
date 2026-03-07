# On-Device AI — TODO

High-level tasks organized by phase. Each phase builds on the previous one.

---

## Phase 1: Foundation + Smart Tagging (Desktop)

Goal: Establish the AI abstraction layer and ship the first AI feature (auto-tagging) on desktop using self-hosted LLMs.

- [ ] Define `AiBackend` trait in `fastxt_core` with `suggest_tags`, `summarize`, `embed`, `categorize`, `is_available`
- [ ] Add `ai_tags`, `ai_summary`, `ai_category` columns to the `note` table (migration)
- [ ] Create `note_embedding` table for vector storage
- [ ] Implement Ollama backend (HTTP client calling local Ollama API)
- [ ] Implement llama.cpp backend (Rust bindings via `llama-cpp-rs` or similar)
- [ ] Add `ai-tag` command to `exe::run` — runs tagging on a single note
- [ ] Add `ai-tag-all` command — batch-tag all untagged notes
- [ ] Desktop GUI: show AI-suggested tags, let user accept/edit/dismiss
- [ ] Desktop GUI: settings panel for model path / Ollama endpoint
- [ ] Add feature flag `ai` in `Cargo.toml` so AI deps are optional

## Phase 2: Summarization + Semantic Search (Desktop)

Goal: Add summarization and meaning-based search using local embeddings.

- [ ] Implement `summarize` in Ollama and llama.cpp backends
- [ ] Add `ai-summarize` command — generate summary for a note
- [ ] Store summaries in `ai_summary` column
- [ ] Implement `embed` — generate vector embeddings for notes
- [ ] Store embeddings in `note_embedding` table
- [ ] Implement cosine similarity search over embeddings
- [ ] Add `semantic-search` command — find notes by meaning
- [ ] Desktop GUI: show summaries in note list view
- [ ] Desktop GUI: semantic search bar alongside existing text search
- [ ] Benchmark embedding generation speed and memory usage

## Phase 3: Apple Foundation Models (iOS / macOS)

Goal: Integrate Apple's on-device Foundation Models on supported Apple devices.

- [ ] Research Apple Foundation Models API surface (availability, capabilities, token limits)
- [ ] Implement `AiBackend` in Swift using Foundation Models framework
- [ ] Bridge Swift AI results back to `fastxt_core` via FFI (JSON strings through `fastxt_run`)
- [ ] Add runtime capability check (`is_available`) — require Apple Silicon + iOS 26+ / macOS 26+
- [ ] iOS app: show AI-suggested tags on note save
- [ ] iOS app: summarize button on note detail view
- [ ] macOS app: same features as iOS
- [ ] Handle graceful fallback when Foundation Models unavailable (older devices)
- [ ] Test on-device latency and battery impact

## Phase 4: Android AI Integration

Goal: Bring on-device AI to Android using platform AI APIs.

- [ ] Research Android App Functions / Gemini Nano / ML Kit availability and API
- [ ] Implement `AiBackend` in Kotlin using Android AI APIs
- [ ] Bridge Kotlin AI results back to `fastxt_core` via JNI/FFI
- [ ] Add runtime capability check — require Android 14+ with supported chipset
- [ ] Android app: show AI-suggested tags on note save
- [ ] Android app: summarize and semantic search features
- [ ] Handle graceful fallback on unsupported devices
- [ ] Test on-device latency and battery impact

## Phase 5: Cross-Device AI Metadata Sync

Goal: AI-generated metadata (tags, summaries, categories) syncs across all devices via P2P.

- [ ] Extend RPC sync protocol to include `ai_tags`, `ai_summary`, `ai_category` fields
- [ ] Handle merge strategy: if two devices generate different AI tags for the same note, union them
- [ ] Sync `note_embedding` only when model IDs match (embeddings are model-specific)
- [ ] Add `ai-reprocess` command — regenerate AI metadata using the local device's model
- [ ] Test sync between desktop (Ollama) and iOS (Foundation Models) with different AI outputs
- [ ] Test sync between Android and desktop

## Phase 6: AI-Assisted Organization

Goal: Use AI to automatically group and categorize the user's note collection.

- [ ] Implement `categorize` — cluster notes into groups by topic
- [ ] Add `ai-organize` command — suggest folder/category structure
- [ ] Desktop GUI: show AI-suggested categories as a sidebar grouping
- [ ] iOS/Android: category view with AI-generated groupings
- [ ] Allow user to pin/rename/dismiss AI categories

## Ongoing / Cross-Cutting

- [ ] Write user-facing documentation for AI features on the website
- [ ] Add privacy documentation explaining on-device-only processing
- [ ] Performance: ensure AI operations don't block the UI (async/background processing)
- [ ] Testing: unit tests for `AiBackend` trait with mock backend
- [ ] Testing: integration tests with small test models
- [ ] CI: add `--features ai` to test matrix
