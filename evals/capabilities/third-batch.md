# Broader capability inventory — 6 September 2026

Work package 3 of [the plan](../../plan.md) beyond dates. Six new
families join the inherited date forms: times (15), money and
quantities (17), obligations (14), parties, places and references (13),
relationships (10) and formats (10) — 79 new cases, 112 in all. Each
carries a stable id, an authored source truth or an explicit absence,
ambiguity or unsupported outcome, provenance, consumer packs, the
verifier's reach with a named gap where it falls short, and a synthetic
model-reading document that contains the phrase.

The form distinctions were adjudicated against public sources where one
exists: the GOV.UK style guide for date, time and money forms, GOV.UK's
invoice requirements, RFC 5545 for date-time, duration and period
forms, and the GOV.UK Design System table component for how a cell
relates to its headers. Twenty cases cite a rule; the rest say
`pending`. Citations carry a locator and the quoted sentence and no
fetchable address, because the privacy boundary permits a printed
address only under `assurance/`; the first draft carried URLs and the
boundary test refused them. The VAT invoice table was not retrievable
and is noted as such. Every document is synthetic; organisations are
invented.

Consumer packs are the two shipped packs and the rows of #480 by an
`idea:` prefix, so a capability no pack uses yet is still listed against
the pack that will.

## What the inventory shows

| family | main-check | pending-v19 | model-judgement | unsupported | none |
|---|---:|---:|---:|---:|---:|
| dates and periods | 0 | 33 | 0 | 0 | 0 |
| times | 0 | 0 | 0 | 0 | 15 |
| money and quantities | 6 | 0 | 1 | 10 | 0 |
| obligations | 1 | 0 | 11 | 0 | 2 |
| parties, places, references | 3 | 0 | 1 | 2 | 7 |
| relationships | 4 | 1 | 2 | 0 | 3 |
| formats | 5 | 0 | 1 | 3 | 1 |

Nineteen of 112 forms have a deterministic check on main; 51 carry a
named verifier gap; none has been read by a model. The largest holes
are the ones #595 named: no time is read at all, no place or reference
is carried, money qualifiers (VAT basis, credits, periods, ranges) are
not read, and cancellation and supersession are not represented. Four
money forms are documented misreads the verifier passes today: a sum in
parentheses, a `CR` balance, an ex-VAT subtotal and a monthly premium
all verify as positive, unqualified sums.

## Validation

The verifier column is executable. `inventory_verifier.rs` runs
`reading::check` over the 22 money and party cases that carry a check
and asserts the authored outcome, including the four misreads, so a
widening or narrowing of what counts as a sum or a name fails against
the inventory. `reading_inventory.rs` holds the contract for every
family: unique ids across files, fixed vocabularies, a named gap on
every `unsupported` or `none` row, a document that contains its phrase,
and the presence of all seven planned families.

| Check | Result |
|---|---|
| `python3 scripts/capability-coverage.py` | 112 cases; exits 0 |
| `cargo test -p runner --test reading_inventory --test inventory_verifier --test fixture_version` | 6 passed |
| `cargo test -p privacy-audit` | passed — provenance carries locators, never fetchable addresses |
| Root formatting and Clippy, all targets/features | Passed |
| Root Rust tests, all features | 1,041 passed |

No pack prompt, schema, scoring version, baseline or tier changed. No
model was run; #628 remains paused. The app and frontend were not
touched and their suites were not rerun. The new files sit under
`evals/` and `scripts/`, both inside the published boundary, and are
synthetic; the public-tree CI job validates the committed projection.

## Limits

This is an inventory, not evidence. A `main-check` row says a
deterministic check exists for the form as written in the inventory's
own document; it does not say a model chooses the right passage, and
the relationship rows make that explicit. The obligation family records
the letter pack's rule on conditional asks and general advice as the
pack's adjudication, not a universal one. The `pending-v19-test` rows
map to #628's boundary, which is not on main. Independent sourcing
covers forms, not the interpretation a pack gives them.

Work package 4, the shared corpus with separate raw-reading and
verified-output scoring, is the next slice and was not started here.
