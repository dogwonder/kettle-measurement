#!/usr/bin/env bash
# Replace a repository's history with a single commit carrying exactly
# the tree it has now (#478).
#
# One job, and the verification is what makes it safe: git names a tree
# by the hash of its contents, so the orphan commit's tree hash must
# equal the tree hash it replaced. If those match, every byte survived
# and only the history went. If they differ, the script refuses to push
# and says so — there is no version of "close enough" here.
#
# Why this exists at all: the measurement layer and the run archive are
# published as they stand rather than as they were arrived at. Their
# content is dated and self-describing — every run directory carries a
# MANIFEST, every baseline names its scoring version and machine — so
# the commit history restates what the files already say, while
# committing every future reader to a chronology of a private product's
# working days. The audit of 17 August 2026 found the archive clean in
# history and not only at its tip, so this is a choice made from a known
# position rather than a cleanup.
#
# It does NOT change repository visibility. Flipping a repository to
# public is a decision, taken by a person in GitHub's settings, and a
# script that could do it is a script that could do it by accident.
#
# Dry run by default:
#   scripts/reset-history.sh dogwonder/kettle-runs
#   scripts/reset-history.sh dogwonder/kettle-runs --yes
set -euo pipefail

REPO="${1:-}"
CONFIRM="${2:-}"

if [ -z "$REPO" ]; then
  echo "usage: scripts/reset-history.sh <owner/repo> [--yes]" >&2
  exit 2
fi

command -v gh >/dev/null || { echo "gh is required" >&2; exit 2; }

# The one irreversible mistake available here is rewriting history that
# somebody has already cloned. A public repository has been fetchable,
# forkable and mirrored; its history is not ours to withdraw. So this
# runs before the flip or not at all.
# REST rather than `gh repo view --json`, which goes through GraphQL:
# the safety check should not be the first thing to fail when one of
# GitHub's two APIs is having a bad afternoon. It 503'd on the first
# run of this script.
VISIBILITY=$(gh api "repos/$REPO" --jq .visibility)
if [ "$VISIBILITY" != "private" ]; then
  cat >&2 <<REFUSED
$REPO is $VISIBILITY.

Rewriting the history of a repository that has been public does not
withdraw it: clones, forks and mirrors keep what was pushed. Reset the
history before the flip, never after.
REFUSED
  exit 1
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

echo "Cloning $REPO …"
# Same reason as the visibility check: `gh repo clone` resolves the
# remote through GraphQL. The URL comes from REST and git does the rest.
CLONE_URL=$(gh api "repos/$REPO" --jq .ssh_url)
git clone -q "$CLONE_URL" "$WORK/repo"
cd "$WORK/repo"

BEFORE_TREE=$(git rev-parse HEAD^{tree})
BEFORE_COMMITS=$(git rev-list --count HEAD)
BRANCH=$(git rev-parse --abbrev-ref HEAD)

echo "  $BEFORE_COMMITS commit(s) on $BRANCH, tree $BEFORE_TREE"

# An orphan commit takes the index as it stands, so the working tree is
# never touched and nothing can be lost in a copy.
git checkout -q --orphan reset-history
git add -A
git -c user.name="kettle" -c user.email="ci@kttl.app" \
  commit -q -m "$(cat <<'MSG'
The measurement layer as it stands

Published as it is rather than as it was arrived at. The content is
dated and self-describing — every run directory names its pack, model,
scoring version and machine; every baseline names what it backs — so a
history of a private product's working days would restate what the
files already say.

This tree is byte-identical to the one this repository held before its
history was reset; git's own tree hash was compared to prove it.
MSG
)"

AFTER_TREE=$(git rev-parse HEAD^{tree})
echo "  after: 1 commit, tree $AFTER_TREE"

if [ "$BEFORE_TREE" != "$AFTER_TREE" ]; then
  echo >&2
  echo "REFUSED: the tree changed." >&2
  echo "  before $BEFORE_TREE" >&2
  echo "  after  $AFTER_TREE" >&2
  echo "Nothing has been pushed. A reset that alters content is a bug here." >&2
  exit 1
fi

echo "Tree hash unchanged: every byte survives, only the history goes."

if [ "$CONFIRM" != "--yes" ]; then
  cat <<DRY

Dry run. Nothing was pushed.

  $REPO: $BEFORE_COMMITS commit(s) → 1, content identical.

Re-run with --yes to force-push. Flipping the repository to public
stays a separate, human decision in GitHub's settings.
DRY
  exit 0
fi

echo "Force-pushing $BRANCH …"
git push -q --force origin "HEAD:$BRANCH"
echo "Done. $REPO now carries one commit; its tree is unchanged at $AFTER_TREE."
