#!/usr/bin/env bash
# Build the Rust core for iOS and package it as an XCFramework the app links,
# and fetch the two speech libraries the app links beside it: sherpa-onnx and
# the ONNX Runtime it runs on, as their projects build them for iOS, each
# checked against the SHA-256 the sherpa-onnx Swift package pins.
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

sherpa_onnx=v2.0.1
sherpa_onnx_sha256=88aa569f71b7903e0bbdfbe6e2e114b3b95862b1ef0759eea3024f82926d7374
onnxruntime=1.28.2
onnxruntime_sha256=2c2299acbb461d26d4bac4bc85985d40e7c7177ed6072703ae0846d88b0b4599

cd "$ROOT/fastxt-ios"
stamp=.speech-libs
want="sherpa-onnx $sherpa_onnx $sherpa_onnx_sha256 onnxruntime $onnxruntime $onnxruntime_sha256"
if [ -f "$stamp" ] && [ "$(cat "$stamp")" = "$want" ] &&
  [ -d sherpa-onnx.xcframework ] && [ -d onnxruntime.xcframework ]; then
  echo "sherpa-onnx $sherpa_onnx and ONNX Runtime $onnxruntime are in fastxt-ios"
  exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
fetch() { # url sha256 -> unpacked into $tmp
  curl -fL --retry 3 -sS -o "$tmp/download.zip" "$1"
  if ! echo "$2  $tmp/download.zip" | shasum -a 256 -c - >/dev/null; then
    echo "error: $1 isn't the file this script expects" >&2
    exit 1
  fi
  unzip -q -o "$tmp/download.zip" -d "$tmp"
  rm "$tmp/download.zip"
}
fetch "https://github.com/k2-fsa/sherpa-onnx/releases/download/xcframework/sherpa-onnx-$sherpa_onnx-ios-static.xcframework.zip" \
  "$sherpa_onnx_sha256"
fetch "https://github.com/csukuangfj/onnxruntime-libs/releases/download/v$onnxruntime/onnxruntime-ios-static-xcframework-$onnxruntime.xcframework.zip" \
  "$onnxruntime_sha256"
rm -rf sherpa-onnx.xcframework onnxruntime.xcframework
mv "$tmp/sherpa-onnx.xcframework" "$tmp/onnxruntime.xcframework" .
echo "$want" >"$stamp"
echo "Fetched fastxt-ios/sherpa-onnx.xcframework ($sherpa_onnx) and onnxruntime.xcframework ($onnxruntime)"
