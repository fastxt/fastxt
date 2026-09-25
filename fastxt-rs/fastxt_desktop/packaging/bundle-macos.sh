#!/usr/bin/env bash
# Bundle the Fastxt desktop binary into Fastxt.app and a .dmg.
#
# Usage: bundle-macos.sh <version> <output-dir> <binary> [more-binaries...]
# Pass one binary per architecture; they are lipo'd into a universal slice
# (a single binary works too — lipo -create with one input is a copy).
set -euo pipefail

VERSION=$1
OUTDIR=$2
shift 2

CRATE_DIR=$(cd "$(dirname "$0")/.." && pwd)
APP="$OUTDIR/Fastxt.app"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$OUTDIR"

lipo -create -output "$APP/Contents/MacOS/fastxt" "$@"
chmod +x "$APP/Contents/MacOS/fastxt"
sed "s/@VERSION@/$VERSION/g" "$CRATE_DIR/packaging/Info.plist" > "$APP/Contents/Info.plist"
cp "$CRATE_DIR/icons/Fastxt.icns" "$APP/Contents/Resources/"

# Ad-hoc signature: gives the app a stable identity so the firewall prompt and
# Gatekeeper dialog behave consistently. This is NOT notarization — until a
# Developer ID certificate is configured, users must right-click > Open the
# first time. See https://fastxt.app for details.
codesign --force --sign - "$APP"

hdiutil create -volname "Fastxt" -srcfolder "$APP" -ov -format UDZO \
    "$OUTDIR/Fastxt-$VERSION.dmg"

echo "created: $OUTDIR/Fastxt-$VERSION.dmg"
