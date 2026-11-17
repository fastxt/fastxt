---
id: developer-setup
title: Developer Setup
sidebar_label: Developer Setup
---

Fastxt's code is on [GitHub](https://github.com/fastxt/fastxt):

```bash
git clone https://github.com/fastxt/fastxt.git
```

## The core, the `fastxt` command and the desktop window

All in Rust, in one workspace:

```bash
cd fastxt-rs
cargo build --release
cargo test --workspace                              # without the AI backends
cargo test --workspace --features fastxt_core/ai    # with them
cargo run -p fastxt -- serve                        # sync server and pairing code
cargo run -p fastxt_desktop --release               # the desktop window
```

## iPhone and iPad

```bash
script/build-ios-libs.sh       # the Rust core, as Fastxt.xcframework
cd fastxt-ios && xcodebuild -project Fastxt.xcodeproj -scheme Fastxt \
  -destination 'platform=iOS Simulator,name=iPhone 17' test
```

## Android

```bash
script/build-android-libs.sh                    # needs the Android NDK and cargo-ndk
cd fastxt-android && ./gradlew assembleDebug    # needs JDK 17
```

## Where the notes are

- Desktop and the `fastxt` command: `~/Fastxt/fastxt.sqlite3`. `-d <file>`, or the `FASTXT_DB` environment variable, picks another file. Use one to experiment: opening a database upgrades it to the current schema.
- iPhone and iPad: the app's Documents folder.
- Android: the app's private storage.

The repository's `CLAUDE.md` describes the architecture: the core library, sync, the JSON protocol the phones use, and how notes reach coding-agent sessions.
