#!/usr/bin/env bash
#
# The install smoke test (#50): does a bundled Kettle stand on its own?
#
# Everything else about packaging is asserted against JSON or against a
# bundle sitting next to its own source tree, where "it found the packs"
# proves nothing — the checkout was right there. This runs the built app
# from a copy elsewhere, with a home directory it has never seen, and
# asks it where it is looking.
#
# Passing means: packs came out of the bundle, the writable directories
# are under this fresh home, no model is installed (the #73 first-run
# floor), nothing was taken from a source checkout, and the sidecar it
# shipped can actually start.
#
# That last one is here because it was missed once. The bundle carried a
# 33KB llama-server that loaded its engine out of /opt/homebrew: bundled,
# executable, Mach-O arm64, and unable to run anywhere but the machine
# that built it. A smoke test that stops at the first-run screen never
# touches the sidecar, so it passed.
#
# Usage: scripts/smoke-install.sh [path/to/Kettle.app]
# Run `bun run tauri build` in app/ first.

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app="${1:-$repo/app/src-tauri/target/release/bundle/macos/Kettle.app}"

if [[ ! -d "$app" ]]; then
  echo "no bundle at $app — run 'bun run tauri build' in app/ first" >&2
  exit 1
fi

# Not `mktemp -d`: that hands back /var/folders/…, and /var is a symlink
# to /private/var. Tauri refuses to resolve resources for an executable
# with a symlinked ancestor on macOS (tauri-utils' starting_binary guard,
# off unless process-relaunch-dangerous-allow-symlink-macos is set), so
# a bundle run from there falls back as if it were unbundled — a
# property of the temp directory, not of the app. /private/tmp is real.
work="$(mktemp -d /private/tmp/kettle-smoke.XXXXXX)"
trap 'rm -rf "$work"' EXIT

# Somewhere else entirely, and a home with no Kettle data in it: a
# machine that has just installed the app and never run it.
cp -R "$app" "$work/Kettle.app"
mkdir -p "$work/home"

report="$(HOME="$work/home" "$work/Kettle.app/Contents/MacOS/kettle-app" --where)"
echo "$report"

fail=0
check() { # check <jq-path> <predicate-description> <test-expression>
  if ! eval "$3"; then
    echo "FAIL: $2" >&2
    fail=1
  fi
}

value() { echo "$report" | sed -n "s/.*\"$1\": \"\{0,1\}\([^\",]*\)\"\{0,1\},\{0,1\}$/\1/p"; }

packs="$(value packs)"
runs="$(value runs)"
models="$(value models)"
model="$(value model)"
from_checkout="$(value from_checkout)"

# Packs are read from the writable copy, not the read-only bundle (#50)
# — and since this home has never seen Kettle, anything in it can only
# have come from the bundle on this launch. That is the first-launch
# install, asserted rather than assumed.
check packs "packs must be read from the fresh home, not $packs" \
  '[[ "$packs" == "$work/home"* ]]'
check packs "the bundled pack was not installed on first launch" \
  '[[ -f "$packs/app.kttl.subscription-audit/pack.json" ]]'
check packs "the installer kept no record of what it wrote" \
  '[[ -f "$packs/.installed.json" ]]'
# The record is what makes "never overwrite a pack you changed"
# enforceable; without a hash it is just a version number and the rule
# cannot be applied.
check packs "the record names no content hash" \
  'grep -q "blake3:" "$packs/.installed.json"'

# What the task grid would actually show. The checks above assert that a
# directory exists and has the right things in it; this one asks the
# command the screen calls. `list_packs` resolved its own path to the
# source checkout for four PRs while every other check here passed, so
# "the packs directory is correct" and "the person sees a task" are
# separate claims and both get made.
packs_found="$(value packs_found)"
check packs_found "the task grid would show no packs (got \"$packs_found\")" \
  '[[ "$packs_found" =~ ^[0-9]+$ && "$packs_found" -ge 1 ]]'

# Most UK banks send PDFs, so an installer that cannot read one is a
# Kettle that asks people for a format their bank does not offer. Two
# separate things have to be true — the feature compiled in and the
# native library shipped — and only "ready" means both.
check reads_pdf "the installed app cannot read PDFs: $(value reads_pdf)" \
  '[[ "$(value reads_pdf)" == "ready" ]]'
check runs "runs must live under the fresh home, not $runs" \
  '[[ "$runs" == "$work/home"* ]]'
check models "weights must be looked for under the fresh home, not $models" \
  '[[ "$models" == "$work/home"* ]]'
check model "a fresh install has no model; got \"$model\"" \
  '[[ "$model" == "none" ]]'
check from_checkout "the app reported taking something from a source checkout" \
  '[[ "$from_checkout" == "false" ]]'

sidecar_path="$(value path)"
sidecar_version="$(value version)"

check sidecar "the sidecar must come from inside the bundle, not $sidecar_path" \
  '[[ "$sidecar_path" == "$work/Kettle.app"* ]]'
# The behavioural half: a version string means dyld resolved every dylib
# llama-server loads, from inside the bundle. No weights are needed to
# ask, which is the point — this is the one engine check a fresh install
# can make.
check sidecar "the shipped sidecar could not start: $sidecar_version" \
  '[[ "$sidecar_version" != unavailable:* && -n "$sidecar_version" ]]'

# The structural half, which is what a developer machine cannot fake:
# nothing in the bundled sidecar loads anything but its own siblings and
# macOS. Homebrew's llama-server passes the check above on this machine
# and fails this one.
while read -r object; do
  [[ "$(file -b "$object")" == Mach-O* ]] || continue
  while read -r loaded; do
    # A dylib's own install name is the first entry otool -L prints, and
    # it is identity rather than dependency — libpdfium's is the
    # relative `./libpdfium.dylib`, which is harmless because Kettle
    # dlopens it by absolute path.
    [[ "$(basename "$loaded")" == "$(basename "$object")" ]] && continue
    case "$loaded" in
      @rpath/*|@loader_path/*|@executable_path/*|/usr/lib/*|/System/*) ;;
      *)
        echo "FAIL: $(basename "$object") loads $loaded, which is not in the bundle" >&2
        fail=1
        ;;
    esac
  done < <(otool -L "$object" | tail -n +2 | awk '{print $1}')
done < <(find "$work/Kettle.app/Contents/Resources/sidecars" -type f)

# The strongest form of the claim: nothing it named is in the checkout.
if grep -q "$repo" <<<"$report"; then
  echo "FAIL: the report names the source checkout at $repo" >&2
  fail=1
fi

if [[ $fail -eq 0 ]]; then
  echo "OK — the bundle stands on its own, and reached the no-model first-run state"
fi
exit $fail
