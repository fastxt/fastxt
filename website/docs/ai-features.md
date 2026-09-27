---
id: ai-features
title: On-Device AI Features
slug: /ai-features/
---

Fastxt includes on-device AI capabilities that help you organize, search, and understand your notes — processed locally on your device, not sent to the cloud.

## Features

### Smart Tagging

Generate relevant tags from note content. The AI prefers your existing tag vocabulary so suggestions stay consistent.

- **Desktop**: Ollama or any OpenAI-compatible local server (llama.cpp, Foundry Local, LM Studio)
- **iOS/iPadOS**: Apple Foundation Models on iOS 26+ where available; Natural Language heuristics elsewhere
- **Android**: keyword and extractive heuristics

### Summarization

Concise summaries of long notes (desktop, iOS, Android).

### Semantic Search

Find notes by meaning, not just keywords. On desktop, semantic search combines the FTS5 full-text index with vector embeddings, fused with reciprocal rank scoring. Works for CJK text too (trigram index).

### AI Organization

Group notes into categories. Existing category names are reused (so renames stick), and you can rename or dismiss categories afterwards.

## Setup

### Desktop (macOS, Windows, Linux)

Fastxt desktop needs a local LLM backend; [Ollama](https://ollama.com) is the default.

1. **Install Ollama**:
   ```bash
   curl -fsSL https://ollama.com/install.sh | sh
   ```

2. **Pull the models** (a chat model for tags/summaries, an embedding model for semantic search):
   ```bash
   ollama pull llama3.2
   ollama pull nomic-embed-text
   ```

3. **Configure Fastxt**:
   - Open Fastxt Desktop → AI Settings
   - Backend: `ollama` (or `llamacpp`, `foundry-local`, `openai` for any OpenAI-compatible server)
   - Endpoint (default `http://localhost:11434` for Ollama), chat model, embedding model
   - Save, then click "Test Connection" — it probes the endpoint you configured and lists any missing models

### iOS / macOS

On iPhone and iPad, Fastxt uses Apple Foundation Models on iOS 26+ when the system model is available, and falls back to Natural Language heuristics on other devices. No setup needed.

### Android

Tag suggestions and summaries use on-device heuristics; no setup needed.

## Sync

Devices sync peer-to-peer: one device runs a server and shows a pairing code (QR or text); the other device scans or pastes it. The channel is TLS-encrypted with the server's certificate pinned from the code, and a session token authorizes every request. AI tags, summaries, categories and embeddings sync along with the notes (embeddings only between devices using the same embedding model and note version).
