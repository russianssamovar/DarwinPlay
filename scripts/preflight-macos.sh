#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  exit 0
fi

if ! command -v xcode-select >/dev/null 2>&1; then
  echo "Xcode developer tools are not available." >&2
  exit 1
fi

DEVELOPER_DIR_CURRENT="$(xcode-select -p 2>/dev/null || true)"
if [[ -z "$DEVELOPER_DIR_CURRENT" || "$DEVELOPER_DIR_CURRENT" == "/Library/Developer/CommandLineTools" ]]; then
  echo "DarwinPlay requires full Xcode for command-line builds." >&2
  echo "Current developer directory: ${DEVELOPER_DIR_CURRENT:-not selected}" >&2
  CANDIDATE="$(find /Applications "$HOME/Applications" -maxdepth 1 -type d -name 'Xcode*.app' -print -quit 2>/dev/null || true)"
  if [[ -n "$CANDIDATE" ]]; then
    echo "Run:" >&2
    echo "  sudo xcode-select --switch "$CANDIDATE/Contents/Developer"" >&2
  else
    echo "Install Xcode, then select Xcode.app/Contents/Developer with xcode-select." >&2
  fi
  exit 1
fi

if ! command -v xcodebuild >/dev/null 2>&1; then
  echo "xcodebuild is unavailable from the selected developer directory." >&2
  exit 1
fi

if ! xcodebuild -version >/dev/null 2>&1; then
  echo "The selected developer directory is not a usable full Xcode installation." >&2
  echo "Current developer directory: $DEVELOPER_DIR_CURRENT" >&2
  exit 1
fi

if ! xcrun --sdk macosx --show-sdk-path >/dev/null 2>&1; then
  echo "The macOS SDK is unavailable from the selected Xcode installation." >&2
  exit 1
fi

if ! xcrun --sdk macosx swift --version >/dev/null 2>&1; then
  echo "The Xcode Swift toolchain is unavailable." >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust is required. Install it with rustup before building DarwinPlay." >&2
  exit 1
fi

printf 'Xcode: %s\n' "$(xcodebuild -version | tr '\n' ' ' | sed 's/[[:space:]]*$//')"
printf 'Developer directory: %s\n' "$DEVELOPER_DIR_CURRENT"
printf 'macOS SDK: %s\n' "$(xcrun --sdk macosx --show-sdk-path)"
printf 'Swift: %s\n' "$(xcrun --sdk macosx swift --version | head -n 1)"
printf 'Rust: %s\n' "$(rustc --version 2>/dev/null || cargo --version)"
