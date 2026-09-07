# Reading capability inventory

One file per capability family, each a list of cases with a stable id.
A case names a source form, what the document states — or that the
value is absent, ambiguous or unsupported — where the distinction came
from, which packs consume it, how far the deterministic verifier
reaches, and a synthetic document a model could be asked to read. Those
three answers are kept apart on purpose: a form the verifier cannot
check, or nobody has measured, is listed as exactly that, never as a
pass (#595, plan work package 3).

| file | family | cases | origin |
|---|---|---|---|
| `date-forms.json` | dates and periods | 33 | inherited from the v18 vocabulary test at `0b7127cc` |
| `time-forms.json` | times | 15 | third batch, 6 September 2026 |
| `money-forms.json` | money and quantities | 17 | third batch |
| `obligation-forms.json` | obligations | 14 | third batch |
| `party-place-reference-forms.json` | parties, places and references | 13 | third batch |
| `relationship-forms.json` | relationships | 10 | third batch |
| `format-forms.json` | formats | 10 | third batch |

## Three answers, three vocabularies

`source_truth.status` says what the document states: a `day`, `time`,
`sum`, `obligation`, `party`, `place`, `reference`, `relation`; or
`absent`, `ambiguous`, `refused`, `no-obligation`, `unsupported`. A
typed value travels with it where there is one. Truth is authored, never
recovered by the production parser.

`verifier.coverage` says how far the deterministic boundary on `main`
reaches, from a fixed vocabulary:

- `main-check` — a deterministic check on main exercises this form;
- `pending-v19-test` — the proposed #628 boundary maps it, no test yet;
- `model-judgement` — only the model's closed answer decides it;
- `unsupported` — the verifier reaches it and refuses or misreads;
- `none` — no field carries it.

`unsupported` and `none` must name a `known_gap`. Where a case carries
`verifier.check`, `crates/runner/tests/inventory_verifier.rs` runs
`reading::check` on main and asserts the authored outcome —
`supported`, `not-a-sum`, `absent`, or `accepted-misread`, the last
being a documented gap the verifier passes today. A change to what a
sum or a name is fails there against the inventory instead of drifting
under it.

`model_reading.coverage` is `not-measured` for every case. No model has
read any of these documents; a synthetic example beside a case is a
question that can be asked, not an answer.

## Provenance

`provenance.independent_source` is either `pending` or a public rule
with a locator a reader can follow — the site, section and entry, or
the RFC clause — and the sentence relied on. No fetchable address
appears: the privacy boundary (`privacy-boundary.toml`) permits a
printed address only under `assurance/`, and widening that surface is a
decision of its own, not a side effect of an inventory. Sourced so far:

- the GOV.UK style guide A to Z — date form `4 June 2017`, ranges with
  `to`, the 12-hour clock (`5:30pm`, `midday`, `midnight`, `10am to
  11am`), money (`£75`, `£75.50`, `4 pence`, `£1.5 million`), numbers
  with commas and `%`;
- GOV.UK, invoices — what an invoice must include (unique number, supply
  date, invoice date, amounts charged, VAT amount, total owed);
- RFC 5545 — floating, UTC and zoned date-times, `DURATION`, `PERIOD`,
  and the all-day event as a `DATE`;
- the GOV.UK Design System table component — row and column headers
  with scope, numeric cells right-aligned.

Not yet sourced: the full/simplified VAT invoice table (the page did not
return it), any public authority on the `3.50pm` dot form, on organisation
name suffixes, or on the pack's conditional-ask rule — those carry the
pack's own adjudication and say so. The inherited date forms remain
internally sourced.

## Reading the report

`python3 scripts/capability-coverage.py` prints, per file and in total,
source outcomes, verifier coverage, model coverage, how many cases are
independently sourced and which carry a known gap. It exits non-zero on
a value outside the vocabularies or a duplicate id. It counts nothing as
a pass that is not `main-check`.

Further families, pack-specific selections and the shared corpus that
would put a model in front of these documents belong to subsequent
slices of #595/#432.

The shared corpus that reuses these capabilities across document kinds lives in `evals/corpus/` (work package 4); `fourth-batch.md` records its first slice.
