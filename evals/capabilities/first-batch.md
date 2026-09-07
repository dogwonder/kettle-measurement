# First implementation batch — 6 September 2026

The generator now authors the amount for each ask, including the council-tax
instalment, parking discount, service-charge demand and invoice total. Source
bindings are authored alongside the paragraphs, never recovered by Kettle's
parser. Fixed appointments carry absolute structure; invoice and tax references
carry the selected source reading for v19. The v18 adapter retains the older
pointing phrase. Existing uncommitted generator work was incorporated.

Every current annex paragraph and organisation footer has an explicit
adjudication. Conditional requests with an unsettled condition and general
advice are negative tasks under the current pack's rule, even where they are
instructions in ordinary language. Pagination adds those decisions as it adds
sections. Text includes the footer, and rendering refuses extra or missing
paragraph, postscript or footer prose. This is a parity check, not a semantic
oracle for arbitrary HTML. Inherited main-body conditional-ask interpretations
were not redesigned in this batch.

The generator requires an explicit scoring version. New Kettle main rejects
incompatible version tags at fixture validation; untagged historical beds keep
their existing validation. The unmodified local #628 loader at `96c4ce477`
accepts the v19 fixtures and rejects dated v18 fixtures lacking `when`. That
older loader does not enforce version tags generally: carry the main loader
change forward when integrating #628. No saved application format, scorer
identity, baseline, tier, prompt or schema was changed here.

`date-forms.json` preserves all 33 inherited forms and their expected dates or
refusals. Each has a stable ID, provenance, a proposed verifier mapping and a
synthetic model-reading example. The current v18 parser test consumes the data;
the separate inventory contract survives its retirement. These forms remain
internally sourced: independent provenance, v19 adapter coverage and model
measurements are explicitly pending. The last-day-of-month verifier gap is
visible rather than counted as a pass.

## Validation

| Check | Result |
|---|---|
| Generator unit/CLI/rendering tests | 11 passed |
| Nine kinds × six styles × two voices × two dateline forms | 216 fixtures accepted by each matching v18/v19 disk loader |
| Opposite-version bed through each loader | Refused before evaluation |
| Fresh local text beds | 36 fixtures accepted by each matching loader |
| Root formatting and Clippy, all targets/features | Passed |
| Root Rust tests, all features | 1,026 passed |
| Separate app formatting and Clippy, all targets/features | Passed |
| Separate app Rust tests, all features | 147 passed; one existing local-model test ignored |

The full suites initially hit sandbox restrictions binding mock-server ports;
they passed when rerun with that permission. No test was removed or quietened.
Different runner checkouts were verified with separate Cargo target directories
after a shared target reused a same-named build artifact. The final loader
checks and opposite-version refusals use the corrected, separate builds.

Frontend and public-projection checks were not run in this batch. No frontend
or report shape changed; the public-tree CI job still needs to validate the
committed projection when these currently uncommitted changes are submitted.
No model evaluation or new performance claim was made, and #628 remains paused.

## Re-asking the checks

From Kettle:

```sh
python3 scripts/capability-coverage.py
cargo test -p runner --test fixture_version --test reading_inventory --test reading_vocabulary
cargo run --offline -p runner --example check_fixture_contract -- ../kettle-examples/out-bed-plan-v18-20260906
```

From `kettle-examples`, using its installed environment:

```sh
python -m unittest discover -s tests
python scripts/check_runner_contract.py --runner ../kettle --scoring-version 18
```

The contract helper loads fixtures only. It starts no endpoint and needs no
weights. `kettle-examples/README.md` documents generation, the v19 adapter check
and its Python dependency lock. Use a separate Cargo target for the v19 checkout.

Fresh local output is in `kettle-examples/out-plan-v18-20260906` and
`out-plan-v19-20260906`, with corresponding `out-bed-plan-v18-20260906` and
`out-bed-plan-v19-20260906`. These disposable directories are ignored. Both
generations use seed 7, count 36, text only, one-page configuration, no injected
instructions and no repeated asks; the complete options are in `generation.json`.
Actual PDF page count and photo severity remain null for text-only output.
Old output and archived recordings were preserved.

Both generations record generator base revision
`d23e1365ec1ae01a0f0ea3729eebd922891c4412` plus the effective source digest
`sha256:0632e1ff12062a80048830ef25e973098db39dd77ee9031774d7aa86984e0898`.
Their bed digests are:

- v18: `sha256:87090ac4d00eecfcd536c81d2d0cf2b1a82fcc211c6bdab7b7913fa37aadc987`
- v19: `sha256:78b6111df26c2e880ec59a85aec359d5defce46ab56accdc4a3cc83d21eeabd5`

Disk replay and resume invalidation are the next work package. Shared field
scoring, independent challenge families, multi-page photo evaluation and
scheduled model comparisons remain subsequent work.
