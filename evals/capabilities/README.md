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

- `main-check` — an owning test executes this inventory case at its declared boundary;
- `not-exercised` — a boundary exists, but no adapter executes this inventory case;
- `model-judgement` — only the model's closed answer decides it;
- `unsupported` — the verifier reaches it and refuses or misreads;
- `none` — no field carries it.

`unsupported` and `none` must name a `known_gap`. Where a case carries
`verifier.check`, `crates/runner/tests/inventory_verifier.rs` runs
`reading::check` on main and asserts the authored outcome —
`supported`, `not-a-sum`, `absent`, or `accepted-misread`, the last
being a documented gap the verifier passes today. An accepted misread
remains `unsupported` even when its regression test passes. A change to what a
sum or a name is fails there against the inventory instead of drifting
under it.

Each executable case names its owning `verifier.test` (file, function
and scope). The 33 date cases run through `timeline::resolve_structured`
with authored structures and base dates under scoring 19. Their check
compares a derived date or no date; it does not exercise passage
containment, the reason for refusing a computation, or model discovery.
The money/name checks exercise containment and parseability of a supplied
reading, not semantic selection or every typed source fact. A general
test of a related boundary does not count as executing an inventory case.

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
source outcomes, verifier coverage, owning checks and their expected
outcomes, model coverage, independent sourcing and known gaps. It refuses
unknown vocabulary, duplicate ids, stale check claims and misreads labelled
supported. This describes executable checks; it does not run them or report
capability passes. Run the owning Rust tests to validate those expectations.

After reconciling #628 on 7 September: **53 cases have executable checks**
(33 date and 20 money/name cases), **59 do not**. The checks expect 27
resolved dates, six cases deriving no date, eight parsable readings, one
absence, six unparsed money readings and five accepted misreads. The last
two categories retain their known limitations; a passing regression test
does not remove them. All 112 cases remain unmeasured by a model.

Ten formerly `main-check` cases had no inventory-consuming test and now
say `not-exercised`, as does the previously pending wrong-event relationship.
This corrects a coverage claim; related production tests remain in place.

Validate this report's claims with
`python3 -m unittest discover -s scripts -p 'test_capability_coverage.py'`.
CI runs these checks beside the owning Rust tests.

Further families, pack-specific selections and the shared corpus that
would put a model in front of these documents belong to subsequent
slices of #595/#432.

The shared corpus that reuses these capabilities across document kinds lives in `evals/corpus/` (work package 4); `fourth-batch.md` records its first slice.
