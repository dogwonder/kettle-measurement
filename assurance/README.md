# Assurance-claims registry

One reviewable place where a product-level claim names the evidence it
stands on and the changes that stop it applying (#434). The registry
points at evidence — baselines, owning tests, release checks, issues —
it never replaces it, and it never copies a mutable number. README, app
copy and essay figures cite a claim id; the claim cites the evidence.

- `claims.json` — the registry. Each entry carries a stable id, the
  exact wording, its scope (pack, version, model, eval set, machine,
  platform — declared fields constrain, undeclared ones don't), its
  evidence, the recorded status, invalidation triggers, the surfaces
  that quote it, and a review route. A claim standing on a baseline may
  also record what it was measured against (`recorded_against`: bed
  digest, model, sidecar) and the verdict it asserts (`expects`) —
  optional fields that turn those declared triggers into checks.
- `claims.schema.json` — the committed contract for that file. The
  enforcing parser is `crates/runner/src/assurance.rs`;
  `crates/runner/tests/assurance_claims.rs` holds the schema to be no
  looser than the parser and validates the committed registry against
  the real tree on every CI run, without executing any local-model or
  packaged-network measurement.

Render the registry with:

```sh
cargo run -p kettle -- claims
```

Exit 0: a sound record (downgrades and failures are states, not
errors). Exit 1: user-facing copy stands on a claim that is no longer
proven. Exit 2: the registry itself cannot be trusted.

## Status is derived, not trusted

`proven`, `unproven` and `failed` are three rendered states. The file
records what was believed when the entry was written; validation
re-derives what the evidence supports now:

- **Refusal** — the registry is broken as a record: evidence that
  doesn't exist, a proven claim with no invalidation triggers or no
  inspectable evidence, duplicate ids, both halves of a declared
  contradiction standing proven. CI fails.
- **Downgrade** — the world moved: a baseline from an earlier scoring
  or pack version, an expired release check. The claim's effective
  status becomes unproven with the mismatch named, and CI stays green —
  evidence goes stale on someone else's merge, and a registry that
  turned that into a red build would be deleted within a week.

`failed` is a finding, not an error state: "a quote is evidence only of
a value it contains" (#460) is one of the most useful records here.

An issue reference is context, never proof. A manual release check is
evidence — with a date and an expiry — because "not automatable" and
"not evidenced" are different things and the registry must say which.

## What validation checks, and what it cannot

Validation covers provenance freshness — scoring version, scope match,
expiry — plus the declared invalidation triggers that are mechanically
checkable (#489). A claim that records the bed digest, model or sidecar
it was measured against is downgraded when the baseline's value moves,
and a claim that states the verdict it asserts is downgraded when the
baseline contradicts it. A trigger the claim records no value for is
not checked, and validation never guesses one.

What no validation reads is the wording itself. Whether the sentence
still describes what the evidence says is a human review duty, owed
along the claim's `review_route` every time its evidence is
re-recorded. The worked example is #489: a baseline was re-recorded,
its verdict went fail → pass under a claim asserting the failure, and
`kettle claims` printed byte-identical output across the swap — the
wording had to be corrected by hand, and was. The checks above make
that particular silence impossible now; they do not make wording
review unnecessary.

The other thing validation does not read is **whether a cited test
passes**. It checks that the file exists and that the owning test's name
is in it — that the claim points at the test which would catch the thing
— and CI is what says whether that test is green today. So a claim can
stand `proven` over a failing test, and on 17 August 2026 one did:
`reference/design/tokens.css` was overwritten with a stale copy,
`tokens-sync.test.ts` went red, and `report-keeps-its-stylesheet` went on
rendering as proven until the frontend suite was read.

Recorded rather than fixed, because the two jobs are worth keeping apart.
A registry that re-ran the tests would be a second CI, slower and behind;
a registry that read CI's last status would make a claim's meaning depend
on which branch was built most recently. What the registry asserts is
that a claim names the evidence that governs it. Whether the tree is
currently green is a question with an existing, better answer.

The gap it leaves is real and worth stating plainly: **the claims page
can say "proven" while a cited test is failing on main.** If that becomes
worth closing, the honest shape is a status the registry reports rather
than derives — the run and conclusion, cited like any other evidence, not
a verdict recomputed here.

## `published` is the publication boundary, and it has two readers

The `published` array names the part of the tree that goes public
(#478), as repo-relative path prefixes. It is declared here rather than
in a workflow because two things read it and they must read the same
sentence:

- **Validation** refuses a *proven* claim that keeps no probative
  evidence inside it. Evidence a reader of kttl.app cannot open is not
  evidence to them, and a citation that dead-ends is the force-loss #477
  was reopened about.
- **`kettle project`** materialises the public tree from it: the tracked
  files inside the boundary, copied into a directory that holds nothing
  else. A second list in the workflow is how the published tree would
  come to disagree with the registry describing it.

So a change here is a publication decision, reviewed as one. Two
consequences are worth knowing before making one:

**Published paths are a stability commitment.** Moving the v13 baselines
into `evals/history/` already broke a claim that cites by path. A rename
inside the boundary is a public broken link.

**The boundary must carry a tree that builds.** `crates/` alone does
not: the workspace manifest, the lock file, the root `fixtures/` two
runner tests read, `scripts/publish-sidecar.sh` two more shell out to,
and the two `app/src-tauri/` manifests that `privacy-boundary.toml`
declares and `kettle scores` reads are all load-bearing. None of that is
visible in a list of prefixes, which is why CI's `public-tree` job
projects the tree and runs its whole suite there, in its own target
directory. A public repository that does not build is worse than none:
it invites a reader to check the claims and hands them a failure.

### A projection is validated slightly differently, and says so

A few claims cite surfaces or tests inside the half that stays closed —
`app/src/lib/components/ExportPanel.svelte`, `app/src/tokens-sync.test.ts`.
In this tree their absence means somebody deleted the copy a claim
depends on, and that is a refusal worth keeping. In the projection the
same absence is the boundary working.

`kettle project` therefore writes `PROJECTION.json`, and validation
reads it: **outside** the boundary, absent is expected; **inside** it,
absent is still broken. The narrowness is the point — a mode that
excused every absence would be an amnesty rather than a boundary. And it
costs no real check, because the rule above already guarantees every
proven claim cites something the projection carries, which is verified
there exactly as strictly as here.

The exam bed is published, decided 17 August 2026. It was never
withholdable: deleting all 180 `generated-exam-*` files from the renewal
pack and running `kettle bed` restores them byte-identically, because the
generator is in `crates/runner/src/eval/letters.rs` and the set
declarations are in the committed bed specs — both inside the boundary.
Withholding the files while publishing the command that regenerates them
is theatre a reader can discover, which is the same finding that settled
the prompts. #317's split still does its real job, which is stopping
prompt iteration from seeing the exam set; #428's independent challenge
set is the holdout that being public cannot weaken.

## The first entry is the reason the registry exists

The renewal pack's 8 August v12 FAIL verdict was a live, quotable claim
for about a day — until #457 showed it rested on the eval's own join,
not on the model. `renewal-v12-fail-verdict` records that withdrawal.
A sentence that outlives its measurement is the failure mode this
directory is for.
