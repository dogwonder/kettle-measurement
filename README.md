# Kettle

A desktop app that runs read-only "task packs" against local files using a
local model, producing reports and proposed actions. Task data stays on
this machine by design; nothing is written automatically. Three packs
ship: a subscription & recurring-spend audit of a bank statement, a
letter-to-actions extraction (what a letter asks of you, and by when),
and an insurance renewal diff (what moved between last year's policy
and this year's).

- **Working conventions:** [`CLAUDE.md`](CLAUDE.md) — the engineering
  decisions that are locked, and why
- **App decisions:** [`app/DECISIONS.md`](app/DECISIONS.md) ·
  **release checks:** [`app/RELEASE-CHECKS.md`](app/RELEASE-CHECKS.md) ·
  **evals:** [`evals/README.md`](evals/README.md)
- **Stack:** Tauri 2 · Rust (`runner` lib + `cli` +
  `privacy-audit`, the build-time guard that holds source to the
  network boundary declared in `privacy-boundary.toml`) · llama-server
  sidecar · Svelte 5 + Vite ·
  [GOV.UK Frontend](https://frontend.design-system.service.gov.uk)
  v6, skinned (see [#168](https://github.com/dogwonder/kettle/issues/168))

## Product constraints and their evidence

`CLAUDE.md` records the rules the product is built to; the
[`assurance/claims.json`](assurance/claims.json) registry records what
the current evidence can prove. Implemented and proven are deliberately
not treated as synonyms:

- **Implemented; whole-tree proof still open — exact money types.** Every
  amount-carrying path uses `rust_decimal::Decimal`, and a test pins an
  exact sum that a float would lose. The broader “never held as a
  float” claim remains unproven until a scoped source guard owns every
  future amount path (`money-never-floated` in the registry).
- **Implemented — constrained reading.** Model steps answer small,
  schema-constrained questions at temperature 0 and Rust re-validates
  every answer. Statements stay decomposed; a one-page letter may stay
  whole while the model answers a closed question about it. Free-form
  prose is allowed only where prose is itself the deliverable, and may
  never change a finding, a number or an action.
- **Proven — read-only pack capabilities and proposed actions.** A pack
  declaring anything beyond `["read"]` is refused. Actions leave Kettle
  only as `.ics` or copyable text for a person to use; the registry
  proves that contract, not the broader runtime negative that Kettle
  can never write to a calendar.
- **Implemented; packaged capture still open — the task network
  boundary.** Task model calls are configured for `llama-server` on
  `127.0.0.1`, and the source audit refuses undeclared network paths.
  Downloading model weights is a separate, explicit network operation.
  No packaged-build capture yet proves that a run makes no non-loopback
  connection, so that claim remains unproven pending
  [#233](https://github.com/dogwonder/kettle/issues/233).

## Status

**M1 — runner and CLI · M2 — eval harness · M3 — desktop app ·
M4 — model manager and packaging · M5 — GOV.UK Frontend rebase: all
complete.** Every issue in those five milestones is closed.

A statement goes end to end: CSV and text-layer PDF parsing with
skipped-row warnings, deterministic merchant cleanup, per-bank column
mappings, grammar-constrained model steps with retry-once-then-review,
recurrence and price-rise detection, totals and annualisation, an actions
emitter, and a self-contained HTML report. The desktop app drives that
pipeline drop-to-report with progress, cancellation, save-a-copy, export
and deletion.

**PDF parses the layouts it recognises, by their exact headings.**
Text-layer PDFs reach a full report where the table is headed Date /
Description / Paid Out / Paid In / Balance, or Date / Description /
Amount / Balance where a running balance settles the direction
([#218](https://github.com/dogwonder/kettle/issues/218)). What is proved
is those *headings*, not those layouts: the match is literal, so four
real HSBC Advance statements — text-layer, that exact column order —
are refused, because their details column is headed "Payment type and
details" and the parser requires the word "Description"
([#343](https://github.com/dogwonder/kettle/issues/343)). Scanned,
image-only PDFs still need OCR
([#71](https://github.com/dogwonder/kettle/issues/71)), and a layout
Kettle does not recognise fails closed, which is the right failure:
guessing which column is money is how you silently invert someone's
spending.

Nothing holds that to account yet. No PDF is scored anywhere in the eval
bed ([#256](https://github.com/dogwonder/kettle/issues/256)) — the three
PDF fixtures on disk are parsed by unit tests and scored by nothing — so
the strongest honest claim is that the fixtures parse. This paragraph
said "PDF reads real statements" until 4 August 2026, which the bank's
own statement contradicts.

**The model no longer decides what a payment is — and the pack's
numbers are being re-established on that basis.** The Stage 3 eval bed
([#237](https://github.com/dogwonder/kettle/issues/237)) showed the
classify step being scored on a question its input cannot answer:
whether a merchant *name* is a subscription, when subscription-ness is
a fact about cadence the model was never shown. So kind is now derived
in Rust from the payments themselves and the pack's own category→kind
policy, and the model answers only what a name can carry — category,
with "unknown" as a legitimate answer
([#253](https://github.com/dogwonder/kettle/issues/253), scoring v5).
The committed `evals/baseline.json` remains the version-4 historical
record; the harness refuses to compare it. The fabrication that floor
run caught — the deterministic pipeline inventing series on
processor-split merchants
([#261](https://github.com/dogwonder/kettle/issues/261)) — is fixed:
`STRIPE*` is cleaned like every other processor, merchants group on
their words rather than on string similarity (which had read
`ALDERRENT`, `ALDERGROCER` and `ALDERMARKET` as one merchant), and a
cadence now needs a majority of a series' intervals to agree with it
rather than just the median. The deterministic floor moved from 0.39
to **1.00 e2e mean** over the bed's 861 items, which is what CLAUDE.md
has always said it should be: recurring below 100% on fixtures is a
Rust bug. A baseline, model margin, and threshold re-derivation
([#234](https://github.com/dogwonder/kettle/issues/234)) can now be
measured over series that are real; they are the next step, and until
they exist `pack.json` still declares `min_tier: "7b"` (#213), staged
as an explicitly stale floor.

**M4 delivered the model manager and weight-free installer.**
([#47](https://github.com/dogwonder/kettle/issues/47)) The first-run
screen recommends from measured tiers, downloads with visible progress,
allows a stopped transfer to resume, verifies every part, and records
which installed model a run will use. `ModelDownload` supports one-file
and split GGUF releases as one all-or-nothing installation
([#49](https://github.com/dogwonder/kettle/issues/49)). The installer
builds and signs without shipping weights.

**The app is not distributable.** It is signed with an Apple *Development*
certificate, which Gatekeeper does not accept and Apple will not notarise
— `spctl` says `rejected`, correctly. Building locally works; handing the
`.dmg` to someone else does not. That stays true until there is a
Developer ID certificate and a notarisation run.

## Development

### Runner and CLI

```sh
cargo test                        # unit tests against synthetic fixtures
cargo run -p kettle -- doctor        # is everything in place to run?
cargo run -p kettle -- parse <file>  # show the transactions Kettle reads
cargo run -p kettle -- bed --pack-dir packs/<pack> --check
                                  # what would regenerating this pack's
                                  # eval bed change? (omit --check to write)
```

PDF support is behind a feature flag, since it needs the vendored pdfium:

```sh
cargo run -p kettle --features pdf -- parse <file.pdf>
```

`app/src-tauri` is a separate Cargo workspace, so the root `cargo test`
does not reach it — but its tests cover runner behaviour against canned
mocks. Root `cargo fmt` and `cargo clippy` miss it too, and CI's
`app-rust` job runs all three. Run the whole set before pushing a runner
change:

```sh
cd app/src-tauri && cargo fmt --check \
  && cargo clippy --all-targets --all-features -- -D warnings \
  && cargo test --all-features
```

Running one of the three is what makes this bite: a one-line call-site
edit once passed root `cargo fmt`, passed `cargo test` here, and failed
CI on `cargo fmt --check` in this directory alone.

### The public measurement tree

Kettle's measurement layer is published as a projection of this tree
(#478) — the pipeline crates, the packs with their prompts and beds, the
committed baselines, the assurance registry and the build toolchain,
under Apache-2.0. The product surface is not. The boundary is the
`published` list in [`assurance/claims.json`](assurance/claims.json), so
changing what is public is a reviewed change to the registry rather than
to a workflow; `assurance/README.md` has the reasoning.

```sh
cargo run -p kettle -- project --check   # what would be published, by prefix
cargo run -p kettle -- project --out ../public-tree
```

The selection is `git ls-files` filtered by the boundary — tracked only,
so the data rules' guarantee that nothing `*.private.*` is committed is
inherited rather than restated. The projection writes `PROJECTION.json`
and `PROJECTION.md` saying what revision it came from and what it omits.

A prefix list cannot tell you whether the result builds, and twice it did
not: the first projection compiled nothing (`fixtures/` unpublished) and
the second failed two sidecar tests (`scripts/publish-sidecar.sh`). So
CI's `public-tree` job projects the tree and runs its whole suite there,
in its own target directory, before the publish workflow pushes it. To
check the same thing locally:

```sh
cargo run -p kettle -- project --out ../public-tree
cd ../public-tree && cargo test --workspace --all-features
```

### Desktop app

```sh
cd app
bun install
bun run tauri dev   # the app, against the real runner
bun run test        # vitest — component and styling guardrails
bun run check       # svelte-check
```

A real run needs the two gitignored prerequisites: a `llama-server`
binary in `sidecars/` (see its README) and `.gguf` model weights —
[`app/src-tauri/models.md`](app/src-tauri/models.md) documents the
pinned list and where the digests come from. `cargo run -p kettle -- doctor`
reports what is missing.

### Evals

Prompt edits are guarded. Measure before and after, and say what moved —
an unmeasured prompt edit is the one change this project cannot review.

```sh
cargo run -p kettle -- eval app.kttl.letter-to-actions --model models/<weights>.gguf \
  --write-baseline /tmp/kettle-letter-before.json
cargo run -p kettle -- eval app.kttl.letter-to-actions --model ... \
  --baseline /tmp/kettle-letter-before.json
```

Exit code 1 on any drop. Baselines carry their own provenance and are
refused across scoring versions rather than silently compared — see
[`evals/README.md`](evals/README.md). CI downloads no weights; evals run
locally only.

### Styling

All styling compiles from `app/src/styles/`. Nothing under it is edited
in place downstream — every consumer is generated:

```sh
cd app
bun run tokens:build   # runs under node, not bun
```

That one command rewrites every copy from the compiled SCSS —
`reference/design/tokens.css`, `app/src/tokens.css`, and the `<style>`
block of **every** pack's `report.html.tera`. The generator finds the
templates rather than naming them: naming one pack is how the letter
template's copy drifted from the compiled stylesheet, unnoticed until
a second pack made the gap visible. Never edit a generated copy by
hand — the sources are the only place a style change belongs.

**A rendered report keeps the stylesheet it was born with.** Reports are
self-contained: the CSS is inlined at render time, so rebuilding the
SCSS changes what *future* reports look like and leaves every report
already on disk exactly as it was. That is deliberate — a report is a
record of one run, and a record that restyled itself later would be a
worse record. But it means a styling change you cannot see is usually a
report that predates it: re-run the pack, or re-render, before assuming
the change did not land.

### Printing a report

There is no Print button, deliberately
([#222](https://github.com/dogwonder/kettle/issues/222)). The report is
served to a sandboxed frame from a different origin, so the webview
cannot ask it to print — the button that used to try failed in complete
silence. Use **Save a copy** and print the saved HTML from a browser;
the report's print stylesheet is written for exactly that.

### A bare `kettle` from any directory

Add a wrapper to your shell profile — it rebuilds from source each time,
so it never goes stale while you are working on the CLI:

```sh
kettle() {
  cargo run -q --manifest-path ~/path/to/kettle/crates/cli/Cargo.toml --bin kettle -- "$@"
}
```

## Data

A real person's records must never enter the repo — synthetic fixtures
only. Real documents for local testing are named `*.private.<ext>`
(gitignored) and used via `--fixture-dir`. That covers every form one
reaches this tree in — the file you were given, a conversion of it, and
any text extracted from it.

A run keeps everything it needed in one per-run directory: input hashes,
raw model exchanges, findings and the report. That is what makes any
figure traceable back to the payments behind it, and it is what "delete
everything" removes.
