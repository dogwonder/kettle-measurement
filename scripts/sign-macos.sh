#!/usr/bin/env bash
#
# Sign the vendored sidecar, so a signed Kettle.app is coherent (#50).
#
# Run this BEFORE `tauri build`, and the ordering is the whole point.
# Tauri signs the .app and seals Contents/Resources by hash; signing the
# sidecar afterwards would invalidate that seal. Signing it first means
# the bytes Tauri seals are already signed bytes.
#
# It has to be done at all because Tauri does not do it. Tauri signs the
# app bundle with the hardened runtime and leaves every Mach-O under
# Contents/Resources as the linker left it — ad-hoc, no team, no runtime
# flag. Kettle ships eleven of those.
#
# And `codesign --verify --deep --strict` passes on that bundle. Mach-O
# files under Resources/ are sealed as resources by content hash rather
# than treated as nested code, so the check that looks authoritative
# reports valid. Notarisation would not: it requires every Mach-O in the
# bundle to carry a Developer ID signature and the hardened runtime. The
# verification that catches it is `codesign -dv` on each file, which is
# what this script does after signing.
#
# Usage:
#   APPLE_SIGNING_IDENTITY="<identity>" scripts/sign-macos.sh
#   scripts/sign-macos.sh "<identity>"
#
# Then build with the same identity, and with /usr/bin ahead of
# Homebrew on PATH — see PATH note below:
#   cd app && PATH="/usr/bin:$PATH" APPLE_SIGNING_IDENTITY="<identity>" \
#     bun run tauri build
#
# Nothing here contacts Apple. A secure timestamp does (it sends a hash
# to timestamp.apple.com, never the binary) and notarisation uploads the
# binary itself — set KETTLE_SECURE_TIMESTAMP=1 for the first when
# preparing a real release; the second is a separate, deliberate step
# that this script does not do.

set -euo pipefail

identity="${1:-${APPLE_SIGNING_IDENTITY:-}}"
if [[ -z "$identity" ]]; then
  echo "no signing identity — set APPLE_SIGNING_IDENTITY or pass one" >&2
  echo >&2
  echo "available:" >&2
  security find-identity -v -p codesigning >&2
  echo >&2
  echo "Distribution outside the App Store needs a 'Developer ID Application'" >&2
  echo "certificate. An 'Apple Development' one signs and verifies locally but" >&2
  echo "Gatekeeper will not accept it and it cannot be notarised." >&2
  exit 1
fi

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dir="$repo/sidecars/macos-arm64"
# The PDF reader is a bundled Mach-O too, and notarisation does not care
# that it is loaded by dlopen rather than spawned.
pdfium="$repo/sidecars/libpdfium.dylib"

if [[ ! -d "$dir" ]]; then
  echo "nothing vendored at sidecars/macos-arm64 — run scripts/vendor-sidecar.sh first" >&2
  exit 1
fi

timestamp="--timestamp=none"
if [[ -n "${KETTLE_SECURE_TIMESTAMP:-}" ]]; then
  timestamp="--timestamp"
  echo "note: a secure timestamp contacts timestamp.apple.com (a hash, not the binary)"
fi

# Inside-out: dylibs before the executable that loads them, because
# signing a Mach-O seals what it depends on.
echo "→ signing dylibs"
for lib in "$dir"/*.dylib; do
  codesign --force $timestamp --options runtime --sign "$identity" "$lib" 2>&1 |
    grep -v "replacing existing signature" || true
done

echo "→ signing llama-server"
codesign --force $timestamp --options runtime --sign "$identity" "$dir/llama-server" 2>&1 |
  grep -v "replacing existing signature" || true

if [[ -f "$pdfium" ]]; then
  echo "→ signing libpdfium"
  codesign --force $timestamp --options runtime --sign "$identity" "$pdfium" 2>&1 |
    grep -v "replacing existing signature" || true
else
  echo "note: no libpdfium to sign — PDF reading will be absent from this bundle"
fi

# Per file, because the bundle-level check does not look here.
echo "→ verifying"
fail=0
while read -r object; do
  [[ "$(file -b "$object")" == Mach-O* ]] || continue
  if ! codesign --verify --strict "$object" 2>/dev/null; then
    echo "FAIL: $(basename "$object") does not verify" >&2
    fail=1
    continue
  fi
  details="$(codesign -dv "$object" 2>&1)"
  if grep -q "Signature=adhoc" <<<"$details"; then
    echo "FAIL: $(basename "$object") is still ad-hoc signed" >&2
    fail=1
  fi
  if ! grep -q "flags=.*runtime" <<<"$details"; then
    echo "FAIL: $(basename "$object") lacks the hardened runtime" >&2
    fail=1
  fi
done < <(find "$dir" -type f; [[ -f "$pdfium" ]] && echo "$pdfium")

if [[ $fail -ne 0 ]]; then
  exit 1
fi

# The sidecar has to survive being signed, not merely satisfy codesign:
# the hardened runtime is a restriction on what a process may do, and
# llama-server wants Metal.
echo "→ checking it still runs"
"$dir/llama-server" --version 2>&1 | head -1

echo "OK — the vendored sidecar is signed with the hardened runtime."
echo "Now build with the same identity; see the header for the PATH caveat."
