# On-Device AI Plan

Fastxt integrates native on-device AI to help users organize their text — privately, with no cloud dependency. Each platform uses its own native AI runtime so inference stays local and data never leaves the device.

## Design Principles

1. **Privacy first** — All AI processing happens on-device. No text is sent to external servers.
2. **Platform-native** — Use each OS's best available AI runtime rather than bundling a single cross-platform model.
3. **Graceful degradation** — AI features are optional enhancements. Fastxt works fully without them (older devices, unsupported OS versions).
4. **Sync-compatible** — AI-generated metadata (tags, summaries, embeddings) is stored alongside notes in SQLite and syncs via the existing P2P RPC protocol.

## Platform AI Backends

### Apple (iOS 26+ / macOS 26+)

- **Runtime**: Apple Foundation Models via the Foundation framework
- **Capabilities**: Text summarization, entity extraction, language understanding, guided generation (structured JSON output)
- **Integration point**: Swift layer calls Foundation Models API, passes results back to `fastxt_core` via FFI
- **Requirements**: Apple Silicon (iPhone 16+, M-series Macs); older devices skip AI features gracefully

### Android

- **Runtime**: Android App Functions / Gemini Nano via ML Kit or AICore
- **Capabilities**: On-device text understanding, summarization, smart replies
- **Integration point**: Kotlin layer calls Android AI APIs, passes results to `fastxt_core` via JNI/FFI
- **Requirements**: Android 14+ with supported chipset; feature-gated at runtime

### Desktop (Linux / Windows)

- **Runtime**: Self-hosted LLMs via llama.cpp (C/Rust bindings) or Ollama (HTTP API)
- **Capabilities**: Full LLM inference — tagging, summarization, semantic search, organization
- **Integration point**: Rust crate in `fastxt_core` talks to llama.cpp as a library or Ollama as a local HTTP service
- **Requirements**: User downloads/configures a model; Fastxt provides UI to select model path or Ollama endpoint

## Architecture

```
┌─────────────────────────────────────────────┐
│                  Fastxt App                 │
│           (Swift / Kotlin / Rust)           │
├─────────────────────────────────────────────┤
│              fastxt_core (Rust)             │
│  ┌──────────┐  ┌──────────┐  ┌───────────┐ │
│  │ Commands │  │ AI Trait  │  │ RPC Sync  │ │
│  │ (CRUD)   │  │ (abstract)│  │ (tarpc)   │ │
│  └──────────┘  └────┬─────┘  └───────────┘ │
│                     │                       │
├─────────────────────┼───────────────────────┤
│        Platform AI Backend (per OS)         │
│  ┌──────────┐ ┌──────────┐ ┌────────────┐  │
│  │  Apple   │ │ Android  │ │  Desktop   │  │
│  │Foundation│ │App Funcs │ │ llama.cpp  │  │
│  │ Models   │ │/Gemini   │ │ / Ollama   │  │
│  └──────────┘ └──────────┘ └────────────┘  │
└─────────────────────────────────────────────┘
```

### AI Trait (Rust Core)

A common trait in `fastxt_core` that all platform backends implement:

```rust
pub trait AiBackend {
    fn suggest_tags(&self, text: &str) -> Vec<String>;
    fn summarize(&self, text: &str) -> String;
    fn embed(&self, text: &str) -> Vec<f32>;
    fn categorize(&self, texts: &[&str]) -> Vec<String>;
    fn is_available(&self) -> bool;
}
```

Platform-specific implementations are provided via FFI callbacks (Apple/Android) or direct Rust integration (desktop).

## Database Schema Changes

New columns/tables to store AI-generated metadata:

```sql
-- Add to existing note table
ALTER TABLE note ADD COLUMN ai_tags TEXT;       -- JSON array of auto-generated tags
ALTER TABLE note ADD COLUMN ai_summary TEXT;    -- AI-generated summary
ALTER TABLE note ADD COLUMN ai_category TEXT;   -- AI-assigned category

-- New table for embeddings (semantic search)
CREATE TABLE note_embedding (
    note_rowid INTEGER PRIMARY KEY REFERENCES note(rowid),
    embedding BLOB NOT NULL,                    -- f32 vector as bytes
    model_id TEXT NOT NULL,                     -- which model generated this
    created_at TEXT NOT NULL
);
```

AI-generated fields sync via the existing RPC protocol — they are treated as regular note data.

## Phased Rollout

See [TODO.md](./TODO.md) for detailed tasks per phase.

| Phase | Focus | Platforms |
|-------|-------|-----------|
| 1 | Smart tagging | Desktop (llama.cpp/Ollama) |
| 2 | Summarization + semantic search | Desktop |
| 3 | Apple Foundation Models integration | iOS, macOS |
| 4 | Android AI integration | Android |
| 5 | Cross-device AI metadata sync | All |
