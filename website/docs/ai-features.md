---
id: ai-features
title: On-Device AI Features
slug: /ai-features/
---

Fastxt includes powerful on-device AI capabilities that help you organize, search, and understand your notes — all processed locally on your device without sending data to the cloud.

## Features

### Smart Tagging

Automatically generate relevant tags for your notes based on their content. The AI analyzes your text and suggests tags that help you categorize and find notes later.

- **Desktop**: Uses Ollama with local language models
- **iOS/macOS**: Uses Apple Foundation Models (requires Apple Silicon)
- **Android**: Uses on-device ML models

### Summarization

Get concise summaries of long notes. The AI extracts the key points and presents them in a brief, readable format.

### Semantic Search

Find notes by meaning, not just keywords. Semantic search understands the intent behind your query and returns relevant notes even if they don't contain the exact words you searched for.

### AI Organization

Automatically organize your notes into categories. The AI analyzes your note collection and suggests logical groupings based on content themes.

## Setup

### Desktop (macOS, Windows, Linux)

Fastxt desktop uses [Ollama](https://ollama.ai) for local AI processing.

1. **Install Ollama**:
   ```bash
   # macOS/Linux
   curl -fsSL https://ollama.ai/install.sh | sh

   # Or download from https://ollama.ai
   ```

2. **Pull a model**:
   ```bash
   ollama pull llama3.2
   ```

3. **Configure Fastxt**:
   - Open Fastxt Desktop
   - Go to AI Settings
   - Set the Ollama endpoint (default: `http://localhost:11434`)
   - Set the model name (default: `llama3.2`)
   - Click "Test Connection" to verify

### iOS / macOS

On Apple devices, Fastxt uses Apple Foundation Models when available.

**Requirements**:
- Apple Silicon Mac (M1 or later)
- iOS 18.1+ or macOS 15.1+
- Siri enabled in system settings

No additional setup required — if your device supports it, AI features are automatically enabled.

### Android

Fastxt for Android uses on-device ML capabilities.

**Requirements**:
- Android 14+ (API level 34)
- Sufficient device memory

AI features are automatically enabled on supported devices.

## Usage

### Getting AI Tags

1. Create or open a note
2. Click the "AI Tags" button
3. Review the suggested tags
4. Click "Use" to add individual tags or "Use All" to add all suggestions

### Summarizing Notes

1. Open a note
2. Click the "Summarize" button
3. The AI-generated summary appears in the note details

### Semantic Search

1. Toggle the search mode to "Semantic" in the sidebar
2. Enter your search query
3. Results are ranked by semantic similarity to your query

### Batch Operations

In AI Settings, you can:

- **Tag All Notes**: Generate AI tags for all notes that don't have them yet
- **Embed All Notes**: Generate embeddings for semantic search
- **Organize Notes**: Categorize all notes into AI-suggested groups

## Privacy

All AI processing happens entirely on your device:

- **No cloud processing**: Your notes never leave your device
- **No data collection**: We don't collect or transmit any of your data
- **Offline capable**: AI features work without internet connection (after initial model download for Ollama)

### Desktop Privacy

When using Ollama:
- The model runs locally on your machine
- No API calls to external services
- Your notes stay on your computer

### Mobile Privacy

- iOS: Uses Apple's on-device Neural Engine
- Android: Uses on-device ML acceleration
- Neither platform sends your data to external servers

## Troubleshooting

### Ollama Connection Failed

1. Ensure Ollama is running: `ollama serve`
2. Check the endpoint URL in AI Settings
3. Verify the model is downloaded: `ollama list`

### AI Features Not Available (iOS/macOS)

- Ensure you have Apple Silicon (M1/M2/M3)
- Check iOS version is 18.1+ or macOS is 15.1+
- Enable Siri in System Settings

### Slow Performance

- **Desktop**: Use a smaller model like `llama3.2:1b` for faster responses
- **Mobile**: Close other apps to free up memory for AI processing

### Memory Issues

If you encounter memory errors:
- Close other applications
- Use a smaller model
- Process notes in smaller batches

## Supported Models

### Ollama (Desktop)

Recommended models:
- `llama3.2` - Best balance of speed and quality (default)
- `llama3.2:1b` - Fastest, lower quality
- `mistral` - Good alternative
- `phi3` - Lightweight option

To switch models:
```bash
ollama pull mistral
```

Then update the model name in AI Settings.

### Apple Foundation Models (iOS/macOS)

Uses the system's built-in language model. No model selection needed.

### Android

Uses platform-provided on-device models. No model selection needed.

## API Commands

Advanced users can interact with AI features via the command interface:

```json
// Get AI tags for text
{"action": "ai-tag", "text": "Your note text here"}

// Summarize a note
{"action": "ai-summarize", "rowid": 123}

// Generate embedding for semantic search
{"action": "ai-embed", "rowid": 123}

// Semantic search
{"action": "semantic-search", "query": "find similar notes", "limit": 10}

// Organize notes into categories
{"action": "ai-organize", "limit": 100}
```

## Feedback

Found a bug or have a suggestion? [Open an issue on GitHub](https://github.com/fastxt/fastxt/issues).
