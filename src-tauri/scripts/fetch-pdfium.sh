#!/usr/bin/env bash
# Downloads a prebuilt PDFium library (https://github.com/bblanchon/pdfium-binaries)
# into src-tauri/lib, where development builds find it and installers bundle it from.
#
# Usage: fetch-pdfium.sh [--if-missing]
set -euo pipefail

cd "$(dirname "$0")/.."

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  pkg=pdfium-mac-arm64;   lib=libpdfium.dylib ;;
  Darwin-x86_64) pkg=pdfium-mac-x64;     lib=libpdfium.dylib ;;
  Linux-x86_64)  pkg=pdfium-linux-x64;   lib=libpdfium.so ;;
  Linux-aarch64) pkg=pdfium-linux-arm64; lib=libpdfium.so ;;
  MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64) pkg=pdfium-win-x64; lib=pdfium.dll ;;
  *) echo "Unsupported platform: $(uname -s) $(uname -m)" >&2; exit 1 ;;
esac

if [ "${1:-}" = "--if-missing" ] && [ -f "lib/$lib" ]; then
  exit 0
fi

version="${PDFIUM_VERSION:-latest}"
if [ "$version" = latest ]; then
  url="https://github.com/bblanchon/pdfium-binaries/releases/latest/download/$pkg.tgz"
else
  url="https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F$version/$pkg.tgz"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $url"
curl -fL --progress-bar "$url" -o "$tmp/pdfium.tgz"
tar -xzf "$tmp/pdfium.tgz" -C "$tmp"

mkdir -p lib
src="$(find "$tmp" -name "$lib" -type f | head -n 1)"
[ -n "$src" ] || { echo "$lib not found in archive" >&2; exit 1; }
cp "$src" "lib/$lib"
echo "Installed lib/$lib"
