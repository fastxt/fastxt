#!/usr/bin/env bash
# Build the Rust core for iOS and package it as an XCFramework the app links.
# Requires the iOS Rust targets (rustup target add aarch64-apple-ios
# aarch64-apple-ios-sim x86_64-apple-ios). Usage: script/build-ios-libs.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/fastxt-rs"
for target in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
  cargo build --release -p fastxt_ffi --target "$target"
done
mkdir -p target/ios-sim
lipo -create \
  target/aarch64-apple-ios-sim/release/libfastxt_ffi.a \
  target/x86_64-apple-ios/release/libfastxt_ffi.a \
  -output target/ios-sim/libfastxt_ffi.a
rm -rf "$ROOT/fastxt-ios/Fastxt.xcframework"
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libfastxt_ffi.a -headers fastxt_ffi/include \
  -library target/ios-sim/libfastxt_ffi.a -headers fastxt_ffi/include \
  -output "$ROOT/fastxt-ios/Fastxt.xcframework"
echo "Wrote fastxt-ios/Fastxt.xcframework"
