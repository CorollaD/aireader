#!/usr/bin/env bash
# Downloads a prebuilt PDFium library (https://github.com/bblanchon/pdfium-binaries)
# into src-tauri/lib, where development builds find it and installers bundle it from.
#
# Usage: fetch-pdfium.sh [--if-missing]
#
# The library matches the Rust target being built: PDFIUM_TARGET, else TAURI_ENV_TARGET_TRIPLE
# (set by the Tauri CLI for build hooks), else this machine. PDFIUM_VERSION picks a release
# (e.g. 8111); the default is the version the release builds use, whose archives are checked
# against the SHA-256 sums below. Other versions are not checked.
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

# SHA-256 of the chromium/8111 archives.
sha256=""
if [ "$version" = 8111 ]; then
  case "$pkg" in
    pdfium-mac-arm64) sha256=46c674298c60123f1195c2a03c65ee08c4783c84874ce4f5b3a054abb3ecaf30 ;;
    pdfium-mac-x64) sha256=e7fece4693f65a7e47f84fa38099068d573d4dcbd46b1a3c8d2c26581c308466 ;;
    pdfium-linux-x64) sha256=bdc60126399f39fe768566a18d4be055a7b3681bc9e248e8a6656b4f6cae0aa2 ;;
    pdfium-linux-arm64) sha256=042de247138f96341534cccf24ed21c2550a7245b936bb4a60d2340120000f05 ;;
    pdfium-win-x64) sha256=3452d11031050ec65e57f180215b57b9e6d465e3fbedafa8a77ad90885c7d23e ;;
  esac
fi
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
if [ -n "$sha256" ]; then
  if command -v sha256sum >/dev/null; then
    actual="$(sha256sum "$tmp/pdfium.tgz" | cut -d' ' -f1)"
  else
    actual="$(shasum -a 256 "$tmp/pdfium.tgz" | cut -d' ' -f1)"
  fi
  if [ "$actual" != "$sha256" ]; then
    echo "Checksum mismatch for $pkg.tgz: expected $sha256, got $actual" >&2
    exit 1
  fi
fi
tar -xzf "$tmp/pdfium.tgz" -C "$tmp"

mkdir -p lib
src="$(find "$tmp" -name "$lib" -type f | head -n 1)"
[ -n "$src" ] || { echo "$lib not found in archive" >&2; exit 1; }
cp "$src" "lib/$lib"
echo "$pkg@$version" > "$stamp"
echo "Installed lib/$lib ($pkg, chromium/$version)"
