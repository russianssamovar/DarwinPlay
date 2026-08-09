#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist"
APP="$DIST/DarwinPlay.app"
CONTENTS="$APP/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"

"$ROOT/scripts/preflight-macos.sh"

cargo build --manifest-path "$ROOT/runtime/Cargo.toml" --release
xcrun --sdk macosx swift build --package-path "$ROOT/app" -c release --product DarwinPlay
SWIFT_BIN_PATH="$(xcrun --sdk macosx swift build --package-path "$ROOT/app" -c release --show-bin-path)"

rm -rf "$APP"
mkdir -p "$MACOS" "$RESOURCES"
cp "$SWIFT_BIN_PATH/DarwinPlay" "$MACOS/DarwinPlay"
cp "$ROOT/runtime/target/release/darwinplay-runtime" "$MACOS/darwinplay-runtime"
cp "$ROOT/assets/DarwinPlay.icns" "$RESOURCES/DarwinPlay.icns"
cp "$ROOT/THIRD_PARTY-NOTICES.md" "$RESOURCES/THIRD_PARTY-NOTICES.md"
cp "$ROOT/scripts/Info.plist" "$CONTENTS/Info.plist"
chmod +x "$MACOS/DarwinPlay" "$MACOS/darwinplay-runtime"
codesign --force --deep --sign - "$APP"

echo "$APP"
