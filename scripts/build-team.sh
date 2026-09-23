#!/usr/bin/env bash
# Build a shareable Sori for teammates → dist/Sori-<version>-arm64.dmg
#
# Differences from build-mac.sh (the owner's personal build):
#   - No API keys baked in (SORI_TEAM_BUILD=1). Fresh installs are fully on-device (Whisper
#     speech recognition + a small local text model), so nobody needs a key.
#   - Refuses to package if a key from .env.local somehow ended up in the binary.
#   - Doesn't install anything locally.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
"$ROOT/scripts/signing-identity.sh"
"$ROOT/scripts/build-llama-server.sh"
cd "$ROOT/desktop"
[ -d node_modules ] || npm install
SORI_TEAM_BUILD=1 npx tauri build --bundles app --no-sign
APP="$ROOT/target/release/bundle/macos/Sori.app"

# Safety net: none of the owner's keys may be inside the team build.
if [ -f "$ROOT/.env.local" ]; then
  while IFS='=' read -r k v; do
    v="$(echo "${v:-}" | tr -d '[:space:]')"
    [ -z "$v" ] && continue
    if grep -rqF -- "$v" "$APP"; then
      echo "✕ $k is embedded in the team build — aborting" >&2
      exit 1
    fi
  done < <(grep -E '^[A-Z_]+_KEY=' "$ROOT/.env.local")
fi

codesign --force --deep --options runtime \
  --entitlements "$ROOT/desktop/src-tauri/Entitlements.plist" \
  --sign "Sori Dev" "$APP"
codesign --verify --verbose=1 "$APP"

VERSION="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$APP/Contents/Info.plist")"
OUT="$ROOT/dist/Sori-$VERSION-arm64.dmg"
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/Sori.app"
ln -s /Applications "$STAGE/Applications"
mkdir -p "$ROOT/dist"
hdiutil create -quiet -volname Sori -srcfolder "$STAGE" -ov -format UDZO "$OUT"
rm -rf "$STAGE"
echo "✓ $OUT ($(du -h "$OUT" | cut -f1))"
echo "  Next personal build: ./scripts/build-mac.sh (rebuilds with your keys)"
