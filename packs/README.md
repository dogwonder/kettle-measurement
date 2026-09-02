# Packs, and the builtins they compose

A pack is data: a manifest, prompts, schemas, fixtures and a template.
What a pack may *do* is limited to composing the builtin steps below —
the runner's standard library, each one generic, tested and versioned
with the runner (`reference/packs/kettle-plugin-architecture.md`, tier
2). A pack needing a step that is not here is a runner change with unit
tests, never pack-specific runner code (#51's bar).

This is the patterns note the design philosophy doc asks for (Part 5.3):
what each builtin does, and when to reach for it. The contract of each
is held by the named tests; this page is the index, not the authority.

Writing a new pack rather than reading about one? **`AUTHORING.md`**
beside this file is the order to do things in and the traps that cost
us a day on the second pack — sealed sets that were copies, ceilings a
bed could not carry, gates that could not read the bed they judged.

## Preprocess builtins

### `builtin:statement-parse`

Reads bank-statement files (CSV, and text-layer PDF via the pdfium
sidecar) into dated, signed transactions, then groups them into
merchants deterministically (`cleanup::group_transactions`, #261).
Row-level oddities become warnings, never crashes; a file with no
readable payments stops the run honestly. Contract tests:
`crates/runner/tests/parse.rs`, `cleanup` coverage in
`tests/recurrence.rs`.

- **In**: the run's input files.
- **Out**: transactions and merchant groups for model steps to ask
  about; `input.rows` counts payments.
- **Reach for it** in any pack whose subject is structured money
  records.

### `builtin:document-text`

Reads a document (text, Markdown, text-layer PDF via the same pdfium
path, or on macOS a full-size JPEG/HEIC photograph) into ordered
paragraph `Segment`s, each carrying its page and position as a person
would cite it (#239, #240, #399). A role declaring
`file_semantics: "pages"` joins several chosen files as ordered parts
of one document. Segmentation is
measured against the page's own line spacing, so letters and agreements
set at different sizes both split at paragraphs. Contract tests:
`crates/runner/tests/document.rs`.

- **In**: the run's input files.
- **Out**: `Segment`s for an `obligations`- or `policy-terms`-role step
  to ask about; `input.rows` counts segments.
- **Reach for it** in any pack whose subject is prose a model will be
  asked small closed questions about.

## Aggregate builtins

### `builtin:recurrence-detect`

Finds the repeating series in each merchant's payments — exact-amount
clustering, cadence banding with a majority rule (#261), price-rise
merging (#28), refund netting (#253) — and derives each finding's kind
from cadence plus the pack's `kinds` map (#253). Declines what it
cannot certify rather than inventing a cadence, and says why (#271).
Deterministic by contract: below 100% on fixtures is a Rust bug.
Contract tests: `crates/runner/tests/recurrence.rs`.

- **In**: merchant groups with model answers applied.
- **Out**: the Audit payload — findings, income, regular spending.
- **Reach for it** after classify in any Audit-typology pack.

### `builtin:timeline-sort`

Resolves extracted deadline phrases against the document's own date —
"within 14 days of the date of this letter", "by the end of the month",
an absolute "by 12 August 2026" — merges duplicate obligations from
overlapping segments (keeping every passage as evidence, and the least
confident reading's confidence), and orders the result soonest first
(#241). The arithmetic is Rust's, tested at month ends and across leap
years; the model never computes a date (CLAUDE.md). A phrase outside
the tested set is not guessed: the obligation keeps its phrase, stays
undated and sorts last, in front of a person. Contract tests:
`crates/runner/tests/timeline.rs`.

- **In**: extracted obligations and the document's segments.
- **Out**: the Extraction payload's obligations, dated, merged and
  ordered.
- **Reach for it** after an `obligations`-role step in any
  Extraction-typology pack; generalised on purpose — housing
  complaints (#92) and warranty monitoring want the same step.

### `builtin:term-diff`

Pairs named values read out of two documents and says what moved
(#350, for #66). Pairing is an identity check on the closed
`(term, basis)` key from the pack's schema, never a string-similarity
guess; every delta is `Decimal` arithmetic Rust does, because the model
neither computes nor compares. A term the model routes to `other`
never pairs and never reaches the diff — the passage goes to a person.
Generic, not renewal-specific: payslips (#67) or a tariff comparison
diff the same way. Contract tests: `crates/runner/tests/terms.rs`.

- **In**: terms read by a `policy-terms`-role step, each carrying its
  quote and which document it came from.
- **Out**: the Comparison payload — changed, unchanged and one-sided
  terms, plus what could not be compared.
- **Reach for it** after a `policy-terms` step in any pack comparing
  named values across two documents.

## Packs in this repo

| Pack | Typology | Pipeline |
|---|---|---|
| `app.kttl.letter-to-actions` | Extraction | `document-text` → `obligations` → `timeline-sort` → render |
| `app.kttl.renewal-diff` | Comparison | `document-text` → `policy-terms` → `term-diff` → render |
| `app.kttl.subscription-audit` — **withdrawn** 30 August 2026 (#545): measured, never offered | Audit | `statement-parse` → `normalise` → `classify` → `recurrence-detect` → render |

The subscription audit carries a `withdrawn { on, why, record }` block
in its manifest, so `kettle eval` still runs it and the app never shows
it; it stays as the one bed on which 4B, 9B and 27B differ. All three
are data only: every step above is a builtin or a declared
model role, and `the_pack_directory_contains_no_executable_code` holds
them to it. The letter and renewal packs' bed generators live in
`crates/runner` beside the statement one — a generator is development
tooling, not something the pack needs in order to run.

### Three beds, and their harm models

The subscription bed's ceilings are symmetric: confidently calling a
subscription something else, and the reverse, are each capped at 5%.
The letter bed's are **not**. Missing an obligation is capped tighter
than inventing one at 5%, because a missed deadline is often
unrecoverable — the fine escalates, the hearing passes — while an
invented one costs a person a phone call.

That asymmetry decides the bed's size, not taste. Wilson's upper bound
at zero errors is `3.84/(n+3.84)`, so a 1% ceiling needs 381 obligations
with zero misses before even a perfect run can clear it, and a 2%
ceiling needs 189 — which is why the letter bed is 355 letters per
selection rather than the 40 first sketched.
`the_bed_carries_the_evidence_its_declared_ceilings_need` asserts it, so
a ceiling can never be declared against a bed too small to satisfy it —
a gate that fails for want of evidence reads exactly like one that fails
for being wrong.

The renewal bed is generated, like the others, from a committed spec
(`crates/runner/src/eval/renewals.rs`), and `tests/renewal_bed.rs`
holds the bed on disk to be byte-for-byte what the spec emits. Two
identical passages are one decision, not two (#310). What it scores is
the **reading** step — did the model read each named value off the
page, verbatim, with its quote — not the pairing or the diff, which
are deterministic Rust downstream of it (#378): there is no diff-level
expectation in the bed, so a green bed says the reading holds, and
says nothing about a layout the segmenter has never met (#377).

**A declared ceiling is the strongest claim the bed can support, not the
ambition.** The letter pack declares 2% and holds 1% as its goal (#315).
That is not a revised view of the harm: the model made zero errors on
207 development and 219 exam decisions, and 2% is simply what that
evidence entitles Kettle to say, where 1% needs a bed roughly twice the
size in both sets. Read a ceiling as a bound on what has been shown, and
the reason field for why it sits where it does — including that the
asymmetry against 5% narrows to 2.5× while the 2% holds, which is a
consequence of the evidence rather than a judgement about deadlines.

## Model roles

The closed set a schema-bearing model step may declare (#120):
`normalise` and `classify` ask about merchant groups; `obligations`
asks what each document segment obliges someone to do, and by when
(#240); `policy-terms` asks each segment for the named values it
states, verbatim with their quotes (#350). Load-time validation
refuses anything else, and `role_names_match_the_runner` holds the
list to the runner's arms.

A pipeline with a `policy-terms` step must also declare `value_kinds`,
mapping every term its schema names to the shape Rust parses the value
as — `money`, `percentage`, `duration`, declared-unchecked `text`, or
a list of those where a term legitimately holds more than one — and
load-time validation holds the map complete against the schema's term
enum. Without the basis and shape being declared, a £45 monthly
instalment pairs with a £520 annual premium and reads as a 1000% rise.
