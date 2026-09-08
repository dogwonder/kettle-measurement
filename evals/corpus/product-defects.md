# Amount selection and list attribution — local patch, 8 September 2026

The letter prompt now explicitly selects the amount required by the particular
payment, including an instalment beside an annual charge. It also attributes
list actions to their own passages, with a shared deadline read at the
introduction. A deadline-only introduction receives no combined task; an
introduction that itself requests an action retains it. Several actions in
one acquired passage are each requested once. Headings and footers are not
blanket exclusions.

This was measured on 8 September 2026 on a rented RTX 4090; see [the results](measurement-02-pod-results.md). It fixes the prose instalment selection and the split-list duplicate, leaves the table-row selection and the heading duplicate, and once drops "within" from a copied deadline. The full bed then found it costs eleven points of pooled recall against 0.3.0; this patch is **withdrawn as a merge candidate**, and the pack on `main` was restored to 0.3.0 the same evening. The prompt and examples described below exist only in history (`0da1fa4e` for 0.3.1, `71646aa8` for 0.3.2) and in the archived recordings. It was an **unmeasured prompt fix** when written, motivated by `money-form-013-letter` and
`format-form-002-letter` in [measurement 1](measurement-01.md). Containment
cannot establish that one of two faithfully copied sums belongs to the ask,
and the current schema does not attest the relationship between free-form
task wording and a heading. Adding a money-label finder or a punctuation-based
task filter would obscure that limitation and violate the current method.
Rust, source-truth scoring and semantic-action status are unchanged.

Pack version is 0.3.1, invalidating version-keyed product results for the new
prompt/examples. Pack scoring remains 19 and corpus scoring remains v6. The
existing failing v19 baseline/tier records remain historical evidence for
their recorded identities; they neither validate this prompt nor become passes.
No pack strata, ceilings, source inventory or old corpus facts were changed.

## Regression material

[`product-regressions-01.json`](product-regressions-01.json) holds ten wholly
synthetic, exposed documents, with twelve positive slots and five explicit
negative sites. Their source facts were authored explicitly before editing
the prompt, without importing a Kettle parser or deriving truth from answers.
The same assistant authored this regression material and the fix: it is not
an independently authored challenge or evidence of generalisation. The new
worked examples use different wording, organisations, figures and dates.

| Case | Required distinction |
|---|---|
| `instalment-after` | Select £81.05 beside an earlier £972.60 annual charge. |
| `instalment-before` | Select £137.40 beside a later £1,648.80 annual charge. |
| `instalment-table` | Read €200.00 at its separate table row; €1,200.00 and both table rows create no further task. |
| `annual-total-due` | Select £960.00 genuinely requested in full, not the old £80.00 instalment. |
| `instalment-not-printed` | Keep the payment ask with an absent amount; neither substitute nor divide £726.00. |
| `already-paid-no-task` | An unsettled already-paid condition creates no current task. |
| `split-list` | Two response bullets, deadline attached to the introduction, no introduction task. |
| `active-heading` | A payment requested in the heading survives beside the response bullet. |
| `merged-list` | Payment and response remain separate actions within one paragraph. |
| `footer-task` | Keep an explicit return request in a footer. |

The first no-model acquisition check caught a table and a multiline list
whose authored passages the text reader split. Before any recording, the
table annotations were separated by row and the merged-list whitespace was
made inline to actually exercise one acquired passage. No fact, wording or
positive action was changed to match an answer. This text selection makes
no PDF/OCR acquisition claim.

The command regression exercises faithful controlled readings, then deliberately
substitutes all three annual-total distractors and adds a combined task at the
list introduction. It requires twelve matched slots for faithful readings,
three wrong amount fields on both sides for the substitutions, and one extra
raw/verified candidate for the heading. It also requires exact replay of both
outcomes and keeps semantic action coverage `not-assessed`. Controlled task
wording is not semantically scored. These are instrument checks, not capability
passes or proof that the model follows the edited instructions.

## Compatible replay

The v6 archive was replayed into a new scratch directory using the original
pack extracted from `53124ce8`. All 33 exact requests, every per-case report,
the summary and generation identities matched; zero legacy matches were used.
The archive and sibling repositories were not modified. Reproduction from
the root, choosing new output directories:

```sh
mkdir -p /tmp/kettle-original-pack
git archive 53124ce8 packs/app.kttl.letter-to-actions | tar -x -C /tmp/kettle-original-pack
cargo run -p kettle --all-features -- corpus \
  --corpus evals/corpus/diagnostic-01.json \
  --pack-dir /tmp/kettle-original-pack/packs/app.kttl.letter-to-actions \
  --replay ../kettle-runs/2026-09-08-corpus-diagnostic01-contracts-v6-replay/replay \
  --out /tmp/kettle-original-pack-replay
```

The command tests additionally change a prompt, require exact replay to refuse
its requests, then restore that prompt and require the original reports back.
Old recordings must never be presented as answers to the new instructions.

## Local validation

Passed: 1,072 root Rust tests; 148 app Rust tests with one existing local-model
ignore; 20 Python checks; formatting and all-target/all-feature Clippy in both workspaces. The
full suites required loopback permission for their mock servers. The pack
version bump correctly made the old tier unavailable as current evidence;
the existing dated floor-stage tests now allow that absence while still
failing when a new passing row makes the stage obsolete. No tier data changed.

Replay and controlled-answer results above establish instrument behaviour
only. The original annual-total failure and heading duplicate remain the
latest real model observations.

## Separate measurement proposal — awaiting authorisation

The [prepared comparison](measurement-02.md) and its [pinned inputs](measurement-02.json)
now make this proposal reviewable. No model run is scheduled to execute automatically. The next proposed sitting
compares the original `53124ce8` pack with this patch on the installed
Qwen3.5-4B Q4_K_M and existing b10145 Metal runtime. Before execution, commit
or freeze both pack identities, both corpus digests, executable/weights/sidecar
digests and the exact request settings in a new measurement plan. Do not
reuse measurement 1's frozen plan for a different prompt.

- Each arm: the unchanged 33-case `diagnostic-01` plus the ten new cases,
  one pass each, 43 initial generation exchanges per arm (86 overall).
- Same machine/sitting, context 8192, temperature 0, reasoning off verified
  from logs, parallel 1, answer limit 4096. Retain and count every exchange.
  Review correction: the existing executor permits one schema retry per
  batch attempt, a targeted pairing re-ask and recursive truncation splits;
  it does **not** guarantee at most one retry per case. The corrected proposal
  retains that shipped policy within the wall-clock cap. The 86 initial
  exchanges are not a total-exchange ceiling. Execution of this corrected
  proposal still requires explicit authorisation.
- Hard stop: 1200 seconds per arm including both selections; two arms only,
  no automatic rerun, paid compute, download or additional model. A timeout
  leaves an incomplete comparison, not permission to extend it.
- Read amount selection, negative-site assertions, unmatched slots/candidates,
  deadline reading/structure/resolution and evidence attachment separately.
  Inspect the heading/bullet action text directly for duplication and the
  combined-action case for coverage; do not reinterpret slot counts as
  semantic omissions. Retain all failures and unsupported scopes.
- Report correctness before runtime; runtime/memory comparisons belong only
  to this sitting. Preserve exact recordings and replay each arm with its
  own pack. Archive synthetic findings only with separate write authorisation
  for `kettle-runs` if required by the filesystem boundary.

This diagnostic cannot clear full-pack ceilings, #625's old/new schema-token
comparison, #552's exam requirement, photograph accuracy, or tier promotion.
Independent challenges, PDF/photo and broader-document model runs, candidate
comparisons and the weekly-versus-pod merge-policy decision remain separate.
