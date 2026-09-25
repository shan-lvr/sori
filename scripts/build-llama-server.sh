#!/usr/bin/env bash
# Build llama.cpp's llama-server as a single static binary and place it where Tauri expects
# a sidecar: desktop/src-tauri/binaries/llama-server-<target-triple>[.exe]
# macOS: Metal (shaders embedded). Windows (Git Bash on CI): Vulkan — needs the Vulkan SDK.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
TAG="${LLAMA_CPP_TAG:-b10964}"
SRC="$ROOT/target/llama.cpp-$TAG"
TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
EXT=""; [[ "$TRIPLE" == *windows* ]] && EXT=".exe"
OUT="$ROOT/desktop/src-tauri/binaries/llama-server-$TRIPLE$EXT"
if [ -x "$OUT" ] && [ "${FORCE:-0}" != "1" ]; then echo "✓ $OUT (cached)"; exit 0; fi

# CI caches restore `target/` partially: only trust a checkout that is complete.
if [ ! -f "$SRC/CMakeLists.txt" ]; then
  rm -rf "$SRC"
  git clone --quiet --depth 1 --branch "$TAG" https://github.com/ggml-org/llama.cpp "$SRC"
fi
FLAGS=(-DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DLLAMA_CURL=OFF -DLLAMA_OPENSSL=OFF
       -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_SERVER=ON -DLLAMA_BUILD_TOOLS=ON -DGGML_NATIVE=OFF)
case "$TRIPLE" in
  *apple-darwin) FLAGS+=(-DGGML_METAL=ON -DGGML_METAL_EMBED_LIBRARY=ON -DCMAKE_OSX_DEPLOYMENT_TARGET=13.0) ;;
  *windows*)     FLAGS+=(-DGGML_VULKAN=ON -DCMAKE_POLICY_DEFAULT_CMP0091=NEW -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded) ;;
  *linux*)       FLAGS+=(-DGGML_VULKAN=OFF) ;;
esac
cmake -S "$SRC" -B "$SRC/build" "${FLAGS[@]}" >/dev/null
cmake --build "$SRC/build" --config Release --target llama-server -j >/dev/null
BIN=$(find "$SRC/build" -name "llama-server$EXT" -type f | head -1)
mkdir -p "$(dirname "$OUT")"
cp "$BIN" "$OUT"
echo "✓ $OUT ($(du -h "$OUT" | cut -f1))"
