---
id: agents
title: From Your Phone to Your Coding Agent
sidebar_label: Phone to Coding Agent
slug: /agents/
---

Fastxt keeps the ideas and prompts you speak on your phone, and hands a prompt to a coding agent on your computer when it's ready. There are two ways in: Fastxt types the prompt into a session you have open, or the agent takes it when you ask.

:::info

This is new: it is in Fastxt's source code and not yet in the App Store apps. It needs the iPhone app; Android doesn't have it yet.

:::

## Dictate

On the iPhone, open **Dictate**, pick **English**, **中文**, or **中文 + EN** for both in one sentence, tap the microphone and speak. Correct what was heard by typing, then **Save as Idea** or **Save as Prompt**. Your notes list shows all of them, or only the ideas, the prompts, or the ones already sent.

The phone recognizes your speech itself. For English or Chinese it uses the recognizer built into iOS, set to work on the device only, when the phone can do that for the language. For the two mixed, or where the iOS recognizer would need the network, it uses [SenseVoice Small](https://github.com/FunAudioLLM/SenseVoice) by FunAudioLLM, a speech model inside the app, whose text comes at each pause. The audio is never stored and never leaves the phone.

## Connect the phone to your computer

1. Install the `fastxt` command. Until its first release, from a clone of [the repository](https://github.com/fastxt/fastxt): `cargo install --path fastxt-rs/fastxt`.
2. Run `fastxt serve --agents`. It shows a pairing code and its QR code, and runs until you stop it with Ctrl-C.
3. On the phone, open **Computer**, tap **Pair with your computer**, then scan the code or paste it.

The phone and the computer talk to each other directly, on the same network, over an encrypted connection pinned to the certificate the code names. Each `fastxt serve` makes a new code, so pair again after restarting it. If the computer's firewall is on, let `fastxt` accept incoming connections.

## Send a prompt into a session you have open

A session is a coding agent you started in a terminal in [Orca](https://www.onorca.dev): Claude Code, Codex, Kimi and the others Orca runs.

On a note, **Send to a coding session** lists the sessions open on your computer, by project. Pick one, and Fastxt types the note in and presses Enter, as if you had typed it there. A session that is asking you something, such as a permission question, can't be picked until you answer it on the computer, and nothing is ever typed into a terminal with no agent in it.

On the computer itself, `fastxt sessions` lists the same sessions and `fastxt send --note <note> --to <session>` sends a note.

## Or let the agent take it

Any agent that supports MCP can take your prompts, whether Orca hosts it or not. Add Fastxt's MCP server to it once:

```bash
claude mcp add --scope user fastxt -- fastxt mcp    # Claude Code
codex mcp add fastxt -- fastxt mcp                  # Codex
```

Prompts reach the computer when the phone syncs: on the phone, open **Computer** and tap **Sync notes now** while `fastxt serve` runs. Each prompt then waits until an agent has it.

- In Claude Code, `/mcp__fastxt__take` puts your newest waiting prompt in as your own message, and `/mcp__fastxt__take login` the waiting one with "login" in it.
- In any agent, ask it to take your prompt from Fastxt: it lists the waiting ones and takes one through its tools.

Agents are told to take a prompt only when you ask, and two agents can't take the same one.

## What is recorded

Every send and every take is recorded with its note: when, which agent, which session, and the project folder. The note's page on the phone lists them (a take shows as "Taken by" the agent), and the records sync with the note to all your devices. Deleting the note deletes its records everywhere.

## Privacy

- Your voice is recognized on the phone and never leaves it.
- Notes travel only between your own devices, directly. There is no Fastxt server.
- Once a prompt is in an agent's session, the agent treats it like anything you type into it: an agent that uses a cloud model sends it to that model's provider. An agent you give Fastxt's MCP server to can also search and read your notes, and add, change or delete them.

The [privacy policy](/privacy-policy) has the details.
