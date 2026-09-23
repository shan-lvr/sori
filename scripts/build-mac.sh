#!/usr/bin/env bash
# Build, sign (stable "Sori Dev" identity) and install Sori to /Applications, then launch it.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
"$ROOT/scripts/signing-identity.sh"
cd "$ROOT/desktop"
[ -d node_modules ] || npm install
npx tauri build --bundles app --no-sign
APP="$ROOT/target/release/bundle/macos/Sori.app"
codesign --force --deep --options runtime \
  --entitlements "$ROOT/desktop/src-tauri/Entitlements.plist" \
  --sign "Sori Dev" "$APP"
codesign --verify --verbose=1 "$APP"
if [ "${1:-}" != "--no-install" ]; then
  osascript -e 'tell application id "com.seyoon.sori" to quit' 2>/dev/null || true
  sleep 1
  pkill -x Sori 2>/dev/null || true
  rm -rf /Applications/Sori.app
  cp -R "$APP" /Applications/Sori.app
  open /Applications/Sori.app
  echo "installed → /Applications/Sori.app"
fi
