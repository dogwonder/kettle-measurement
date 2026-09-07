# Shared corpus, first slice — 6 September 2026

Work package 4 of [the plan](../../plan.md): the smallest useful shared
corpus, with raw-reading and verified-output scoring kept apart.
`evals/corpus/README.md` describes the records; this note records what
was built, what it found and how it was validated.

## Built

- `kettle-examples/synth_letters/corpus.py` authors twelve typed facts,
  three asks and three selected-over-distractor relations, renders them
  as a letter and as a statement, and writes `evals/corpus/slice-01.json`
  with provenance (generator revision, content digest, synthetic).
  Everything is a fact with an id; no value is recovered from rendered
  text. `tests/test_corpus.py` holds the generator's contract and checks
  the committed kettle copy against a fresh build.
- `crates/runner/src/eval/corpus.rs` loads and validates a slice and
  scores a case: per-field outcomes for the raw proposal and for the
  verified output, a pack `Selection` that turns unselected fields into
  `Unsupported`, missed and invented asks, evidence attachment,
  declared uncertainty and whole-item correctness, with a summary over
  cases. `VerifiedAsk` is built from the run's own `Obligation`.
- `crates/runner/tests/corpus_slice.rs` fixes the scorer's behaviour on
  the plan's acceptance examples with mock proposals and no weights.
  Each proposal is verified through the real `reading::check` and
  timeline, so the verified column is the pipeline's answer and not a
  second mock.

## What the scorer says

| proposal | raw | verified |
|---|---|---|
| faithful, either kind | correct on every selected field | correct, bar one cell below |
| annual total instead of the instalment | wrong | wrong — the sum is on the page, so verification cannot catch a wrong choice between two true figures |
| the earlier letter's date copied faithfully as the appointment deadline | wrong, evidence misattached | wrong (resolves to 3 March) |
| attendance ask not proposed | missing on every field, missed once | missing |
| an ask asserted on the conditional passage | invented, the true asks untouched | invented |
| the right amount at low confidence | uncertain, not a whole-item pass | correct |
| the right time under the letter pack's selection | unsupported | unsupported |
| the right time with time selected | correct | unsupported — no field carries it |
| a time given for the payment, which states none | wrong | — |

The one cell a faithful proposal does not pass: the statement writes
the appointment as `02/04/2026`, which the resolver refuses on purpose
because both fields could be the month. The raw reading is correct and
the verified deadline is `Missing` — not `Wrong`, never `Correct`. The
payment row's `24/03/2026` is unambiguous and resolves. This is the
first thing the corpus found, and it is exactly the shape the two
columns exist to show: pooled, it would read as a reading error.

## Validation

| Check | Result |
|---|---|
| `cargo test -p runner --test corpus_slice` | 10 passed |
| kettle-examples `python -m unittest discover -s tests` | 16 passed |
| Root formatting and Clippy, all targets/features | passed |
| Root Rust tests, all features | 1,051 passed |
| `cargo test -p privacy-audit` | passed |

No pack prompt, schema, scoring version, baseline or tier changed. No
model was run; #628 remains paused. The app and frontend were not
touched and their suites were not rerun. `evals/corpus/` sits inside
the published boundary and is synthetic throughout.

## Limits

Two document kinds and two obligations establish the scorer, not a
measurement. Raw readings are judged as copies of the document's own
words for the fact in that kind, so a paraphrase that means the same
thing scores wrong by design. Verified time, place and reference are
`Unsupported` until a field carries them. Growing the slice toward the
inventory, and any bounded pilot that reads it with a model, are later
work and were not started here.
