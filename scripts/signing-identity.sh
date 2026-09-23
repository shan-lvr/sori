#!/usr/bin/env bash
# Creates a self-signed code-signing identity "Sori Dev" in the login keychain (once).
# Signing every build with the same identity keeps macOS privacy grants (Accessibility,
# Microphone) across rebuilds. No trust settings are changed. Remove it any time in
# Keychain Access → login → My Certificates → "Sori Dev".
set -euo pipefail
NAME="Sori Dev"
if security find-identity -p codesigning 2>/dev/null | grep -q "\"$NAME\""; then
  echo "identity '$NAME' already exists"; exit 0
fi
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cat > "$tmp/cfg" <<CFG
[req]
distinguished_name=dn
x509_extensions=ext
prompt=no
[dn]
CN=$NAME
[ext]
basicConstraints=critical,CA:false
keyUsage=critical,digitalSignature
extendedKeyUsage=critical,codeSigning
CFG
/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -keyout "$tmp/key.pem" -out "$tmp/cert.pem" -days 3650 -config "$tmp/cfg" 2>/dev/null
/usr/bin/openssl pkcs12 -export -inkey "$tmp/key.pem" -in "$tmp/cert.pem" -out "$tmp/id.p12" -passout pass:sori -name "$NAME"
security import "$tmp/id.p12" -k "$HOME/Library/Keychains/login.keychain-db" -P sori -T /usr/bin/codesign >/dev/null
echo "created identity '$NAME'"
