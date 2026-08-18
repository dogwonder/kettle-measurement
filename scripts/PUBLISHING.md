# Publishing the measurement layer

Kettle is private; its measurement layer is public (#478). Two
repositories carry it, and neither is written by hand:

| Repository | What it holds | How it gets there |
|---|---|---|
| `dogwonder/kettle-measurement` | the projection — pipeline crates, task packs with their prompts and beds, committed baselines, the assurance registry, the build toolchain | `.github/workflows/public-tree-publish.yml`, on every merge to `main` |
| `dogwonder/kettle-runs` | the run recordings the baselines rest on, each with a MANIFEST | by hand, after a rented-GPU sitting (`evals/RENTED-GPU.md`) |

The boundary is declared once, as `published` in
`assurance/claims.json`, because validation and `kettle project` must
read the same sentence. It is not repeated here, and a second list is
how the published tree would come to disagree with the registry that
describes it. The same file's `published_at` says where the projection
goes, so the workflow's remote, every evidence link on kttl.app and the
registry itself name one repository; a test fails if the workflow and
the registry drift apart.

## The first publication is a decision, not a merge

The publish workflow runs on merge and pushes to a **private** remote
until somebody flips it. That is deliberate: a projection's whole value
is that it tracks `main`, so publishing it on a schedule somebody has to
remember would make it the hand-maintained mirror #269 forbids — but the
*first* push crossing into public view is a decision, and decisions do
not belong to CI.

Order matters only in one direction. `kettle-measurement` names the run
archive in three places (`evals/README.md`, `evals/RENTED-GPU.md`, and
one claim note), so a public projection beside a private archive owes
the reader an explanation. Publishing the archive first, or both
together, owes nothing.

## History starts at the flip

Decided 17 August 2026. Both repositories are published **as they
stand**, not as they were arrived at: their history is replaced with a
single commit at the moment of the flip, and accumulates normally from
there.

The reason is not that the history is dirty. It was audited first, over
every commit rather than the tip:

- every path ever added to `kettle-runs` is `.json`, `.txt`, `.md` or
  `.log` — no `*.private.*` file, no document format, ever
- the only deletions in its history are the 3,100 `.request.json` →
  `.request.txt` renames of PR #518
- no email address, IBAN or sort code appears anywhere in it
- all **1,699** distinct fixture-runs ever recorded trace to a fixture
  committed in this repository, every one from the development set bar
  the three declared `statement-*` files

So the choice is made from a known position. What it buys is that a
private product's working days do not become a public chronology, and
what it costs is close to nothing: the content is dated and
self-describing, so the commit messages restate what the files already
say. `kettle-measurement` had four commits, all from CI, on the day this
was decided.

The one thing this cannot be is a later idea. Rewriting the history of a
repository that has been public does not withdraw it — clones, forks and
mirrors keep what was pushed — so `scripts/reset-history.sh` refuses to
run against a repository that is not private, and refuses before it
clones anything.

## Doing it

```sh
scripts/reset-history.sh dogwonder/kettle-runs           # dry run
scripts/reset-history.sh dogwonder/kettle-runs --yes     # force-push
scripts/reset-history.sh dogwonder/kettle-measurement --yes
```

The script replaces history and nothing else. Git names a tree by the
hash of its contents, so it compares the orphan commit's tree hash with
the one it replaced and refuses to push if they differ — there is no
"close enough" available. A dry run prints both hashes and pushes
nothing.

Then, in GitHub's settings for each repository, flip it to public. The
script will not do this, and should not: a script that can make a
repository public is a script that can make one public by accident.

Afterwards, the next merge to `main` publishes the projection as usual
and the history grows from the reset point — which is what the workflow
argues for, since *when did this baseline change, and what moved with
it* is exactly what a reader checking a claim wants to see.

## What it does not publish

No model weights and no llama-server sidecar; neither is redistributable
from here. The honest wording is **inspectable, and re-runnable given
the weights** — `evals/README.md` names the weights each baseline was
recorded against.

Published paths are a stability commitment (#477). Moving the v13
baselines to `evals/history/` already broke a claim that cites by path;
after the flip, a rename inside the boundary is a public broken link.
