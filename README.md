# Fastxt

[fastxt.app](https://fastxt.app)

Fastxt, as a sister project to [Local Native](https://localnative.app), with very similar design and toolchain, is a decentralized cross-platform application to save and sync your txt in a local SQLite database without going through any centralized service.

Fastxt uses **on-device AI** to help you organize your text information — privately, without sending data to the cloud.

## Sync

Devices sync peer-to-peer over an authenticated, encrypted channel:

- The server prints a **pairing code** (also shown as a QR code) containing its address, a certificate fingerprint and a one-time session token.
- The client pins the certificate and presents the token with every request — no password, no account, no CA.
- Edits, deletes and AI metadata propagate in both directions; a pairing code is valid only for one server session.

## The `fastxt` command

One command does everything on a computer apart from the desktop window:

```sh
cargo install fastxt     # or: pip install fastxt, uvx fastxt, npm install -g fastxt
fastxt serve             # let your phone sync with this computer (shows the pairing code)
fastxt sync <code>       # sync this computer with another one, once
fastxt mcp               # let agents use your notes (an MCP server on stdio)
```

## Sending a note to a coding agent

Started with `fastxt serve --agents`, a computer also shows paired devices the coding-agent sessions you have open in [Orca](https://www.onorca.dev) (Claude Code, Codex, Kimi and the others it runs), and types a note into the one you pick, as if you had typed it there. Nothing is typed into a session that is asking you a permission question, and nothing into a terminal with no agent in it. Every device keeps a record of what was sent where.

On the computer itself, `fastxt sessions` lists them and `fastxt send --note <note> --to <session>` sends one; with `--on <pairing code>`, both work on a paired computer instead.

## On-Device AI

Per platform, today:

- **Desktop (macOS/Windows/Linux)** — Ollama or any OpenAI-compatible local server (llama.cpp, Foundry Local, LM Studio): smart tagging, summaries, semantic search (FTS5 + embeddings, hybrid ranked), and AI categories. The desktop has AI Settings to pick the backend and models.
- **iOS/iPadOS** — Apple Foundation Models on iOS 26+ where available, Natural Language heuristics elsewhere: tag suggestions and summaries. (No embeddings on mobile yet.)
- **Android** — keyword and extractive heuristics: tag suggestions and summaries. (No embeddings on mobile yet.)

The core desktop features (AI tagging, summarization, semantic search, AI categories, embedding sync) are implemented and tested; mobile semantic search is on the roadmap.

# Videos

[Local Native YouTube Channel](https://www.youtube.com/channel/UCO3qFIyK0eSmqvMknsslWRw)

## Articles

[Updates](https://fastxt.app/blog)

# Sub-directories

- fastxt-android: Android app (Kotlin, modern toolchain; Rust core via JNI)
- fastxt-ios: iOS and iPadOS app (SwiftUI; Rust core via XCFramework)
- fastxt-rs: Rust workspace — `fastxt` (the command: serve, sync, sessions, send, mcp), `fastxt_core` (notes, sync, AI), `fastxt_desktop` (the desktop window, `fastxt-desktop`), `fastxt_ffi` (C/JNI bindings for the phones)
- packaging/npm, pyproject.toml: the `fastxt` command's npm and PyPI packages
- script: build helpers (`build-android-libs.sh`, `build-ios-libs.sh`)
- archive: dormant prototypes (fastxt-mac SwiftUI, fastxt-win UWP, fastxt-flutter) — superseded, not built

# Developer Setup

[here](https://fastxt.app/developer-setup.html)

# License

[AGPL-3.0](https://www.gnu.org/licenses/agpl-3.0.en.html)

## Screenshot

### Desktop Application
![Fastxt desktop application](https://fastxt.app/img/fastxt-sync-server-qr-mac.png)

## Support

<a href="https://opencollective.com/fastxt/donate" target="_blank">
  <img src="https://opencollective.com/localnative/donate/button@2x.png?color=blue" width=300 />
</a>
