#!/usr/bin/env bash
# Downloads a prebuilt PDFium library (https://github.com/bblanchon/pdfium-binaries)
# into src-tauri/lib, where development builds find it and installers bundle it from.
#
# Usage: fetch-pdfium.sh [--if-missing]
#
# The library matches the Rust target being built: PDFIUM_TARGET, else TAURI_ENV_TARGET_TRIPLE
# (set by the Tauri CLI for build hooks), else this machine. PDFIUM_VERSION picks a release
# (e.g. 8111); the default is the version the release builds use.
set -euo pipefail

cd "$(dirname "$0")/.."

target="${PDFIUM_TARGET:-${TAURI_ENV_TARGET_TRIPLE:-}}"
if [ -z "$target" ]; then
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) target=aarch64-apple-darwin ;;
    Darwin-x86_64) target=x86_64-apple-darwin ;;
    Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
    Linux-aarch64) target=aarch64-unknown-linux-gnu ;;
    MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64) target=x86_64-pc-windows-msvc ;;
    *) echo "Unsupported platform: $(uname -s) $(uname -m)" >&2; exit 1 ;;
  esac
fi

case "$target" in
  aarch64-apple-darwin) pkg=pdfium-mac-arm64; lib=libpdfium.dylib ;;
  x86_64-apple-darwin) pkg=pdfium-mac-x64; lib=libpdfium.dylib ;;
  x86_64-unknown-linux-gnu) pkg=pdfium-linux-x64; lib=libpdfium.so ;;
  aarch64-unknown-linux-gnu) pkg=pdfium-linux-arm64; lib=libpdfium.so ;;
  x86_64-pc-windows-msvc) pkg=pdfium-win-x64; lib=pdfium.dll ;;
  *) echo "Unsupported target: $target" >&2; exit 1 ;;
esac

version="${PDFIUM_VERSION:-8111}"
# Records which package is in lib/, so switching targets fetches the right one.
stamp="lib/.pdfium"
if [ "${1:-}" = "--if-missing" ] && [ -f "lib/$lib" ] && [ "$(cat "$stamp" 2>/dev/null)" = "$pkg@$version" ]; then
  exit 0
fi

if [ "$version" = latest ]; then
  url="https://github.com/bblanchon/pdfium-binaries/releases/latest/download/$pkg.tgz"
else
  url="https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F$version/$pkg.tgz"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $url"
curl -fL --retry 3 --progress-bar "$url" -o "$tmp/pdfium.tgz"
tar -xzf "$tmp/pdfium.tgz" -C "$tmp"

mkdir -p lib
src="$(find "$tmp" -name "$lib" -type f | head -n 1)"
[ -n "$src" ] || { echo "$lib not found in archive" >&2; exit 1; }
cp "$src" "lib/$lib"
echo "$pkg@$version" > "$stamp"
echo "Installed lib/$lib ($pkg, chromium/$version)"
