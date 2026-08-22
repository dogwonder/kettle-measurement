#!/usr/bin/env bash
#
# Reclaim build-artefact disk, safely and repeatably.
#
# ## What actually grows, measured 21 August 2026
#
# Not the eval runs and not worktrees, which is where the suspicion
# naturally lands: `evals/runs` was 5.8MB and `git worktree list` showed
# one. It is Cargo, and it is structural — `crates/runner/tests/` holds
# 64 integration test files, and each one is its own crate, so each gets
# its own link, its own binary and its own `.dSYM`. `app/src-tauri` is a
# second workspace, so the dependency tree underneath it is built twice.
#
#   target/debug/deps           8.0G   (~4.0G of it .dSYM, 166 bundles)
#   target/debug/incremental    3.2G   rebuild cache, nothing else
#   app/src-tauri/target        3.2G   the second workspace
#   target/release              421M
#
# 8.6G of artefacts for a suite that size is the ordinary cost, not a
# leak. Two things about it are worth knowing before reaching for a
# bigger hammer.
#
# ## Two settings that look like waste and are not
#
# **`split-debuginfo = "packed"` stays.** macOS defaults dev builds to
# unpacked DWARF in loose `.o` files that cargo never collects: 1,067,978
# of them, 88GiB, 88% orphaned, until c0d1b53 (20 August) set both
# workspaces to packed. One `.dSYM` per target that `cargo clean` can
# actually remove is the *fix*, and the 4.0G it costs is what replaced
# 88GiB. Reverting it would look like a saving for about a week.
#
# **`debug = "line-tables-only"` stays, including for tests.**
# `[profile.test] debug = 0` would drop the per-test-binary `.dSYM`, and
# was considered and refused on 21 August: it removes file and line from
# a panic backtrace, and the test binaries are exactly where that is
# worth paying for — a test failing somewhere unexpected is the case you
# cannot plan for. The trade is lopsided in the other direction anyway.
# Disk here is recoverable on demand, which is what this script is; a
# backtrace nobody recorded is not.
#
# So the answer to "the checkout keeps growing" is a sweep you can run
# whenever, not a setting that buys gigabytes by making failures harder
# to read.
#
# ## `df` will not move, and that is not this script failing
#
# Time Machine keeps local snapshots — 13 of them on the day this was
# written — and they pin the blocks of anything deleted until they
# expire. `du` drops immediately, `df` lags by hours. Freed bytes are
# reported from `du` for that reason. `tmutil listlocalsnapshots /`
# shows them if the wait matters.
#
# Usage: scripts/housekeeping.sh [--days N]
#
#   --days N   also sweep artefacts untouched for N days (default 7),
#              which is what catches churn as branches come and go.
#              Needs cargo-sweep; skipped with a note if absent.

set -euo pipefail

DAYS=7
while [[ $# -gt 0 ]]; do
  case "$1" in
    --days) DAYS="${2:?--days needs a number}"; shift 2 ;;
    --days=*) DAYS="${1#*=}"; shift ;;
    -h|--help) sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//;$d'; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 1 ;;
  esac
done

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"

# Deleting a target directory out from under a running build does not
# fail loudly — it produces a corrupted-looking rebuild, or two suites
# racing on fixed temp paths, which reads exactly like flaky tests. So
# this refuses rather than races. Same reason `cargo test` in two
# terminals is a known way to lose an afternoon.
if pgrep -f "[c]argo (build|test|run|clippy|check)" >/dev/null \
  || pgrep -f "[k]ettle eval" >/dev/null \
  || pgrep -f "[s]idecars/.*llama-server" >/dev/null; then
  echo "a build, eval or sidecar is running — refusing to sweep under it" >&2
  echo "what is running:" >&2
  pgrep -fl "[c]argo (build|test|run|clippy|check)|[k]ettle eval|[s]idecars/.*llama-server" >&2
  exit 1
fi

# Kilobytes, so the arithmetic below stays integer.
used() {
  local total=0 dir
  for dir in "$@"; do
    [[ -d "$dir" ]] && total=$(( total + $(du -sk "$dir" | awk '{print $1}') ))
  done
  echo "$total"
}

TARGETS=(target app/src-tauri/target)
before="$(used "${TARGETS[@]}")"

echo "→ clearing incremental caches (rebuild speed only, no artefact)"
for dir in "${TARGETS[@]}"; do
  rm -rf "${dir:?}/debug/incremental"
done

echo "→ clearing eval logs"
rm -rf target/eval-logs

# Asked through cargo, not `command -v cargo-sweep`: cargo finds its own
# subcommands in ~/.cargo/bin whether or not that directory is on PATH,
# and here it is not — so the direct check reports "not installed" about
# a binary sitting right there.
if cargo sweep --version >/dev/null 2>&1; then
  echo "→ sweeping artefacts untouched for ${DAYS} days"
  # Each workspace is swept in its own right: `app/src-tauri` has its
  # own Cargo.lock and its own target, and a sweep run at the root does
  # not reach it. That separation is the same one that makes it a test
  # blind spot, and it catches people the same way.
  for dir in "$repo" "$repo/app/src-tauri"; do
    (cd "$dir" && cargo sweep --time "$DAYS" 2>&1 | sed 's/^/   /') || true
  done
else
  echo "→ cargo-sweep not installed, skipping the stale-artefact pass"
  echo "   cargo install cargo-sweep"
fi

after="$(used "${TARGETS[@]}")"
freed=$(( (before - after) / 1024 ))

echo
echo "freed ${freed}MB — $(( before / 1024 ))MB → $(( after / 1024 ))MB across both workspaces"

if [[ "$(uname)" == "Darwin" ]]; then
  snapshots="$(tmutil listlocalsnapshots / 2>/dev/null | grep -c 'com.apple' || true)"
  if [[ "${snapshots:-0}" -gt 0 ]]; then
    echo
    echo "note: ${snapshots} Time Machine local snapshots are pinning the freed blocks."
    echo "      du reflects this now; df will not until they expire."
  fi
fi
