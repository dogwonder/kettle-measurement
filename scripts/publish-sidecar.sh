#!/usr/bin/env bash
#
# Validate a prepared llama.cpp runtime and only then publish it into
# sidecars/<platform>. Kept separate from vendor-sidecar.sh so the
# failure boundary is executable in a test without a network or compiler.

set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo "usage: publish-sidecar.sh <platform> <source-dir> <destination> <build>" >&2
  exit 2
fi

platform="$1"
src="$2"
dest="$3"
build="$4"

case "$platform" in
  # linux-x86_64 is the measurement platform (see vendor-sidecar.sh): it
  # is never shipped to anyone, but it publishes through the same
  # staging and version check as the two that are, because "the bundle
  # starts and names its build" is worth asserting wherever a sidecar
  # comes from.
  macos-arm64 | linux-arm64 | linux-x86_64) ;;
  *)
    echo "refusing unknown sidecar platform: $platform" >&2
    exit 2
    ;;
esac

if [[ "$(basename "$dest")" != "$platform" || "$dest" == "/" ]]; then
  echo "destination must end in sidecars/$platform: $dest" >&2
  exit 2
fi
if [[ ! -f "$src/llama-server" ]]; then
  echo "no llama-server in prepared source: $src" >&2
  exit 1
fi

parent="$(dirname "$dest")"
mkdir -p "$parent"
stage="$(mktemp -d "$parent/.${platform}.staging.XXXXXX")"
backup=""

cleanup() {
  if [[ -n "$stage" && -d "$stage" ]]; then
    rm -rf -- "$stage"
  fi
  if [[ -n "$backup" && -d "$backup" && ! -e "$dest" ]]; then
    mv "$backup" "$dest"
  fi
}
trap cleanup EXIT

if [[ "$platform" == macos-* ]]; then
  # The @rpath closure, resolved through LC_RPATH = @loader_path.
  echo "→ resolving @rpath closure"
  closure=(llama-server)
  queue=(llama-server)
  while [[ ${#queue[@]} -gt 0 ]]; do
    current="${queue[0]}"
    queue=("${queue[@]:1}")
    while read -r loaded; do
      [[ "$loaded" == @rpath/* ]] || continue
      name="${loaded#@rpath/}"
      [[ -e "$src/$name" ]] || {
        echo "unresolved dependency $loaded" >&2
        exit 1
      }
      for seen in "${closure[@]}"; do
        [[ "$seen" == "$name" ]] && continue 2
      done
      closure+=("$name")
      queue+=("$name")
    done < <(otool -L "$src/$current" | tail -n +2 | awk '{print $1}')
  done
  # Dereference the release's dylib aliases: the bundle contains real
  # files rather than links whose targets may not have been selected.
  for name in "${closure[@]}"; do
    cp -L "$src/$name" "$stage/$name"
  done
else
  # ggml discovers CPU backends by scanning beside the executable, so
  # every lib*.so* is runtime material even when it is absent from NEEDED.
  libraries=("$src"/lib*.so*)
  if [[ ! -e "${libraries[0]}" ]]; then
    echo "no runtime libraries beside $src/llama-server" >&2
    exit 1
  fi
  cp -L "$src/llama-server" "$stage/llama-server"
  cp -a "${libraries[@]}" "$stage/"
fi

chmod +x "$stage/llama-server"
if [[ -f "$src/LICENSE" ]]; then
  cp "$src/LICENSE" "$stage/LICENSE"
elif [[ -f "$src/../LICENSE" ]]; then
  cp "$src/../LICENSE" "$stage/LICENSE"
else
  echo "the prepared sidecar has no llama.cpp licence" >&2
  exit 1
fi
printf '%s\n' "$build" > "$stage/BUILD"

echo "→ checking the prepared sidecar before publication"
if ! version="$("$stage/llama-server" --version 2>&1)"; then
  echo "$version" >&2
  echo "sidecar failed its pre-publication start check" >&2
  exit 1
fi

# Preserve a previously working bundle until the replacement has passed
# validation and is ready to rename on the same filesystem.
if [[ -e "$dest" ]]; then
  backup="$(mktemp -d "$parent/.${platform}.previous.XXXXXX")"
  rmdir "$backup"
  mv "$dest" "$backup"
fi
mv "$stage" "$dest"
stage=""

if [[ -n "$backup" ]]; then
  rm -rf -- "$backup"
  backup=""
fi

echo "→ vendored $(find "$dest" -type f | wc -l | tr -d ' ') files ($(du -sh "$dest" | cut -f1)) into sidecars/$platform"
echo "$version" | head -1
