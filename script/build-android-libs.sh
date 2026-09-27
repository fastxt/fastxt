#!/usr/bin/env bash
# Build the Rust core for Android and place the shared libraries where the
# app's jniLibs picks them up. Requires the Android NDK and cargo-ndk
# (cargo install cargo-ndk). Usage: script/build-android-libs.sh
set -euo pipefail
cd "$(dirname "$0")/../fastxt-rs"
cargo ndk \
  -t arm64-v8a \
  -t armeabi-v7a \
  -t x86_64 \
  -t x86 \
  -o ../fastxt-android/app/src/main/jniLibs \
  build --release -p fastxt_ffi
