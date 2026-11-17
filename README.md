# Fastxt

[fastxt.app](https://fastxt.app)

Fastxt is a notebook for the ideas and prompts you speak, and a way to hand them to the coding agents on your computer. You dictate on your phone, in English or Chinese, and the phone recognizes the speech itself. Your notes sync directly between your own devices, with no account and no Fastxt server in between. When a prompt is ready, Fastxt types it into a coding-agent session you have open, or an agent takes it when you ask.

Underneath, it is still a sister project to [Local Native](https://localnative.app), with very similar design and toolchain: a cross-platform app that keeps your text in a local SQLite database and syncs it without going through any centralized service, with **on-device AI** to help you organize it, privately.

## Where it stands

- **iPhone**: dictation, ideas and prompts, pairing with a computer and sending to its sessions are built and tested in the simulator. They aren't in an App Store release yet, and dictation has yet to be tried on a real phone.
- **Computer**: the `fastxt` command and the desktop window. The command isn't on crates.io, PyPI or npm yet; until its first release, install it from a clone (below).
- **Android**: notes, with on-device tag suggestions and summaries. Dictation and sending come to it after the iPhone gets a speech model of its own (SenseVoice Small).

## Dictating on the phone

The iPhone app is a notebook for the ideas and prompts you speak. Dictation, in English or Chinese, runs on the phone only: Fastxt uses the recognizer built into iOS, on the device, and doesn't start when the phone can't recognize the language without the network, so the audio never leaves the phone and is never stored. Correct what was heard by typing, then save it as an idea or a prompt.

## Sending a note to a coding agent

Started with `fastxt serve --agents`, a computer also shows paired devices the coding-agent sessions you have open in [Orca](https://www.onorca.dev) (Claude Code, Codex, Kimi and the others it runs), and types a note into the one you pick, as if you had typed it there. Nothing is typed into a session that is asking you a permission question, and nothing into a terminal with no agent in it. Every device keeps a record of what was sent where.

On the phone, pair with that computer (**Computer**, then scan or paste the code it shows), and a note's **Send to a coding session** lists the sessions open there, by project. On the computer itself, `fastxt sessions` lists them and `fastxt send --note <note> --to <session>` sends one; with `--on <pairing code>`, both work on a paired computer instead.

## Taking a prompt in any agent

An agent can also take a prompt itself, in any session that has Fastxt's MCP server, whether Orca hosts it or not. Add the server once:

```sh
claude mcp add --scope user fastxt -- fastxt mcp    # Claude Code
codex mcp add fastxt -- fastxt mcp                  # Codex
```

Once the phone has synced with the computer (on the phone: **Computer**, then **Sync notes now**), the prompts no agent has had yet wait there. In Claude Code, `/mcp__fastxt__take` puts the newest one in as your own message, and `/mcp__fastxt__take login` the waiting one with "login" in it; in any agent, asking it to take your prompt from Fastxt does the same through its tools. Either way the prompt stops waiting, and after its next sync the phone shows which agent took it, and in which folder.

## The `fastxt` command

One command does everything on a computer apart from the desktop window. Until its first release, install it from a clone of this repository:

```sh
cargo install --path fastxt-rs/fastxt
```

From the first release on, it's `cargo install fastxt`, `pip install fastxt` (or `uvx fastxt`), or `npm install -g fastxt`.

```sh
fastxt serve                     # let your phone sync with this computer (shows the pairing code)
fastxt serve --agents            # ...and send notes to the agent sessions open here
fastxt sync <code>               # sync this computer with another one, once
fastxt sessions                  # the agent sessions open in Orca
fastxt send --note 7 --to 5e47   # type note 7 into one of them
fastxt mcp                       # let agents use your notes and take your prompts (an MCP server on stdio)
```

The notes are in `~/Fastxt/fastxt.sqlite3`; `-d <file>`, or `FASTXT_DB`, picks another database.

## Sync

Devices sync peer-to-peer over an authenticated, encrypted channel:

- The server prints a **pairing code** (also shown as a QR code) containing its address, a certificate fingerprint and a one-time session token.
- The client pins the certificate and presents the token with every request — no password, no account, no CA.
- Edits, deletes and AI metadata propagate in both directions; a pairing code is valid only for one server session.

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

[here](https://fastxt.app/docs/developer-setup)

# License

[AGPL-3.0](https://www.gnu.org/licenses/agpl-3.0.en.html)

## Screenshot

### Desktop Application
![Fastxt desktop application](https://fastxt.app/img/fastxt-sync-server-qr-mac.png)

## Support

<a href="https://opencollective.com/fastxt/donate" target="_blank">
  <img src="https://opencollective.com/localnative/donate/button@2x.png?color=blue" width=300 />
</a>
