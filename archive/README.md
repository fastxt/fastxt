# Archived desktop/prototype clients

These are **dormant, superseded** Fastxt client prototypes, kept for reference
and git history. None are build/release targets.

| Directory | Stack | Last active | Superseded by |
|---|---|---|---|
| `fastxt-mac/` | SwiftUI (native macOS) | 2020 | `fastxt-rs/fastxt_desktop` (Iced) |
| `fastxt-win/` | UWP C#/XAML | 2020 | `fastxt-rs/fastxt_desktop` (Iced) |
| `fastxt-flutter/` | Flutter (mobile skeleton) | 2020 | `fastxt-android` (native Kotlin) |

## Why

Desktop (Windows/macOS/Linux) was consolidated onto a **single Iced (Rust) GUI**
at `fastxt-rs/fastxt_desktop`, which links `fastxt_core` directly (zero FFI) and
runs the same view architecture as the sister project's `localnative_iced`. The
`fastxt-mac` (SwiftUI) and `fastxt-win` (UWP — itself deprecated by Microsoft)
prototypes were never finished and are replaced by that one cross-platform binary.

`fastxt-flutter` was a 2020 `flutter create` skeleton targeting mobile only
(desktop platforms were never enabled); the real Android client is the native
Kotlin app in `fastxt-android`.

These directories are intentionally excluded from CI and the workspace. If a
prototype is ever revived, move it back out of `archive/` first.
