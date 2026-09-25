#!/usr/bin/env bash
# Build llama.cpp's llama-server as a single static binary and place it where Tauri expects
# a sidecar: desktop/src-tauri/binaries/llama-server-<target-triple>[.exe]
# macOS: Metal (shaders embedded). Windows (Git Bash on CI): Vulkan — needs the Vulkan SDK.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
TAG="${LLAMA_CPP_TAG:-b10964}"
# Follows CARGO_TARGET_DIR: on Windows a short one avoids MSBuild's 260-char path limit
# (ggml's nested vulkan-shaders-gen build is deep).
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
command -v cygpath >/dev/null && TARGET_DIR="$(cygpath -u "$TARGET_DIR")"
SRC="$TARGET_DIR/llama.cpp-$TAG"
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
  # No OpenMP: MSVC's links VCOMP140.DLL, which clean PCs without the VC++ redistributable lack
  # (ggml's own thread pool is used instead, as on macOS and in whisper-rs).
  *windows*)     FLAGS+=(-DGGML_VULKAN=ON -DGGML_OPENMP=OFF -DCMAKE_POLICY_DEFAULT_CMP0091=NEW -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded) ;;
  *linux*)       FLAGS+=(-DGGML_VULKAN=OFF) ;;
esac
LOG="$SRC/build.log"
if ! { cmake -S "$SRC" -B "$SRC/build" "${FLAGS[@]}" && cmake --build "$SRC/build" --config Release --target llama-server -j; } >"$LOG" 2>&1; then
  grep -E "error|Error" "$LOG" | tail -20 || true
  tail -20 "$LOG"
  echo "✗ llama-server build failed (full log: $LOG)" >&2
  exit 1
fi
BIN=$(find "$SRC/build" -name "llama-server$EXT" -type f | head -1)
mkdir -p "$(dirname "$OUT")"
cp "$BIN" "$OUT"
echo "✓ $OUT ($(du -h "$OUT" | cut -f1))"
