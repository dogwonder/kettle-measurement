# Publishing Kettle

Kettle is private; three things it produces are public (#269, #478).
None of the three is written by hand, and none of them is a copy: each
is generated from this repository, so there is never a second version of
a screen, a prompt, a baseline or a fixture to drift.

| Repository | What it holds | How it gets there | Visibility |
|---|---|---|---|
| `dogwonder/kettle` | the product and everything else — this repository | by hand; it is the source of the other three | private |
| `dogwonder/kettle-measurement` | the projection — pipeline crates, task packs with their prompts and beds, committed baselines, the assurance registry, the build toolchain | `.github/workflows/public-tree-publish.yml`, on every merge to `main` | public |
| `dogwonder/kettle-runs` | the run recordings the baselines rest on, each with a MANIFEST | by hand, after a rented-GPU sitting (`evals/RENTED-GPU.md`) | public |
| `dogwonder/kettle-demo` | build output only: `app/dist-demo/`, force-pushed to `gh-pages` and served at kttl.app | `.github/workflows/demo-deploy.yml`, manually dispatched | public |

Three different kinds of thing cross the line, and the difference is
worth holding: `kettle-measurement` publishes **source**, `kettle-runs`
publishes **data**, and `kettle-demo` publishes **build output**. Only
the first is something anyone would send a patch to. The product
surface, `app/src/`, crosses no line at all — the demo publishes what it
compiles to, never what it is.

The boundary is declared once, as `published` in
`assurance/claims.json`, because validation and `kettle project` must
read the same sentence. It is not repeated here, and a second list is
how the published tree would come to disagree with the registry that
describes it. The same file's `published_at` and `recordings_at` say
where the projection and the archive went, so the workflow's remote,
every evidence link on kttl.app and the registry itself name one
repository each; a test fails if the workflow and the registry drift
apart.

---

# Part one: the measurement layer

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

## Checking the projection before it goes

CI's `public-tree` job does this on every PR, and it is worth knowing
how to run by hand when a change touches the boundary:

```sh
cargo run -p kettle -- project --out /tmp/public-tree
```

Then build and test **inside** that directory, with `CARGO_TARGET_DIR`
pointed outside it, because what a prefix list cannot show is whether
the result builds. It twice did not. A projection that compiles here and
not there is the failure a public repository cannot afford: it invites a
reader to check the claims and hands them a stack trace.

## What it does not publish

No model weights and no llama-server sidecar; neither is redistributable
from here. The honest wording is **inspectable, and re-runnable given
the weights** — `evals/README.md` names the weights each baseline was
recorded against.

Published paths are a stability commitment (#477). Moving the v13
baselines to `evals/history/` already broke a claim that cites by path;
after the flip, a rename inside the boundary is a public broken link.

---

# Part two: the demo and kttl.app

The public site is the real product running on a recorded run, plus the
pages that back its claims. It builds from this working tree rather than
from a copy — see `app/demo/README.md` for why that is the whole design.

## Building it locally

```sh
cd app
bun run demo:dev      # the site, hot-reloading, on the dev server
bun run demo:build    # what actually gets published, into app/dist-demo/
```

The build shells out to `cargo run -p kettle -- claims --json`,
`packs list --json` and `scores`, so the pages' numbers are the
validated projections rather than figures typed into a template. That is
also why the build is slow the first time and why a Rust error fails it:
a page that cannot get a real number does not get to invent one.

**Run all four check surfaces before pushing anything that touches this**
— each has caught something the others could not:

```sh
cargo fmt --all --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test
cd app/src-tauri && cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features
cd app && bun run check && bun run test
cd app && bun run demo:build
```

## Deploying it

```sh
gh workflow run demo-deploy.yml -f ref=main
```

`workflow_dispatch` only, and not `push`, deliberately: a private
product's UI should not become public as a side effect of merging.
Adding `push: branches: [main]` makes the demo track main automatically,
and is a decision to take once, on purpose.

The workflow builds `app/dist-demo/`, adds `.nojekyll`, and force-pushes
the result to `gh-pages` on `dogwonder/kettle-demo`. Built output
crosses the line; source does not.

## When Actions cannot run

`workflow_dispatch` being the only trigger has a consequence worth
stating plainly: it is also the only CI route to kttl.app. On 18 August
2026 the dispatch was refused before the job started — *"the job was not
started because recent account payments have failed"* — and with it the
site had no way to be published at all. The site was then a build behind
main for three hours, serving copy that a merged commit had already
corrected.

So the fallback is the workflow's own steps, run by hand:

```sh
cd app && bun run demo:build
git clone --branch gh-pages git@github.com:dogwonder/kettle-demo.git /tmp/kettle-demo
cd /tmp/kettle-demo
find . -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +
rsync -a ~/Localhost/app/kettle/app/dist-demo/ .
touch .nojekyll
git add -A && git commit -m "Demo build from $(git -C ~/Localhost/app/kettle describe --always)"
git push origin gh-pages
```

**Clone over SSH, not HTTPS.** The workflow pushes with
`https://x-access-token:$TOKEN@github.com/...`, and that form works only
inside CI, where `DEMO_DEPLOY_TOKEN` exists. Locally it prompts for a
password and fails: `gh config get git_protocol` reports `https`, but the
git remotes here are SSH, so the HTTPS credential path has never held
anything. Substituting `$(gh auth token)` into the URL looks like the
obvious fix and is not one — it fails as an interactive password prompt
rather than as a permissions error, which sends you looking for a scope
problem that does not exist.

Two differences from the workflow, neither of which matters: the branch
gains a commit instead of being replaced, and `.nojekyll` is written by
hand rather than by the publish step. `CNAME` still arrives from
`app/demo/public/`, as it does in CI.

Then verify, and verify the *bundle* — see "Verifying a deploy" below.
Every page returns 200 whichever build is answering, so an HTTP check
cannot tell you the deploy landed. The one on 18 August was caught only
by grepping the hashed JS for a string the new build introduced.

## The published branch must carry everything Pages keeps

The push is a **force**-push over a tree with no history worth keeping,
so anything GitHub Pages stores *on that branch* has to be something the
build produces. Two things qualify:

- `.nojekyll`, or Pages drops the hashed asset directories Vite emits.
  The workflow writes it.
- `CNAME`, or Pages unsets the custom domain and the site falls back to
  `*.github.io`. `app/demo/public/CNAME` holds `kttl.app` and the build
  copies it, so every deploy restores the domain rather than removing
  it. `demo.test.ts` fails if that file goes missing while the workflow
  still force-pushes.

The second one is the trap: an outage caused by a *successful* build is
the kind nobody attributes correctly.

## First-time setup, as it was actually done

Recorded because it is the part nobody remembers, and it was done on
18 August 2026 in this order — which matters, since the demo builds for
a root domain (no Vite `base`), so it cannot be verified at a
`github.io/kettle-demo/` subpath first. DNS has to be right before the
first deploy.

1. **The target repository**: `dogwonder/kettle-demo`, public, empty. No
   README and no default branch — the force-push creates `gh-pages`.
2. **The token**: a fine-grained PAT at
   `github.com/settings/personal-access-tokens/new`, resource owner
   `dogwonder`, only `kettle-demo` selected, one permission —
   **Contents: read and write**. Named for its job
   ("kettle-demo deploy (kttl.app)"), because a token called `kettle` and
   another called `kettle 2` is how the wrong one gets revoked. Stored
   as the `DEMO_DEPLOY_TOKEN` secret on **`dogwonder/kettle`**, the
   repository that runs the workflow — not on the one being pushed to.
3. **DNS**, at the registrar: four apex `A` records to
   `185.199.108.153`, `.109.153`, `.110.153` and `.111.153`, and `www`
   as a `CNAME` to `dogwonder.github.io`.
4. **Dispatch the deploy**, which creates `gh-pages`.
5. **Pages**: source `gh-pages` / root. The pushed `CNAME` sets the
   custom domain by itself. Tick **Enforce HTTPS** once the certificate
   is issued — a few minutes after DNS resolves, not instantly.

## Verifying a deploy

The pages are client-rendered, so fetching the HTML proves the site
responds and nothing more. To check the content actually shipped, read
the built bundle:

```sh
curl -sS -o /dev/null -w "%{http_code}\n" -L https://kttl.app/evidence/
curl -sS https://kttl.app/evidence/ | grep -o '/assets/[A-Za-z0-9_.-]*\.js'
```

Then fetch the registry bundle among those assets and count the
addresses in it — 19 of the registry's 26 evidence references resolve,
and the seven that do not are the six issue citations and one `app/`
test. Following one of them for real is the check that matters: a
citation that 404s on the public tree is the failure the whole assurance
case rests on not having.
