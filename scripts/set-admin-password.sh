#!/usr/bin/env bash
# Set the admin-mode password (owner only). Stored in the git-ignored .env.local; build.rs bakes
# only a verifier and — for personal builds — your API keys encrypted with it. Rebuild afterwards.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
F="$ROOT/.env.local"
read -r -s -p "New admin password: " P1; echo
read -r -s -p "Repeat: " P2; echo
[ "$P1" = "$P2" ] || { echo "✕ Passwords don't match"; exit 1; }
[ ${#P1} -ge 10 ] || { echo "✕ Use at least 10 characters (it protects your API keys)"; exit 1; }
case "$P1" in *$'\n'*) echo "✕ No line breaks"; exit 1 ;; esac
touch "$F"; chmod 600 "$F"
{ grep -v '^SORI_ADMIN_PASSWORD=' "$F" || true; printf 'SORI_ADMIN_PASSWORD=%s\n' "$P1"; } > "$F.tmp"
mv "$F.tmp" "$F"; chmod 600 "$F"
echo "✓ Saved to .env.local"
echo "  Rebuild your app:        ./scripts/build-mac.sh"
echo "  Admin mode in CI builds: gh secret set SORI_ADMIN_PASSWORD   (paste the same password)"
