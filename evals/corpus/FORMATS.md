# Paired acquisition diagnostics

`formats-01/` contains two wholly synthetic, exposed development cases,
each rendered as ordered text files, a PDF and ordered photographed JPEGs.
The first places an authored return-form ask in the footer. The second has
three explicit pages with account information in the middle and both asks
on page three. A cancelled appointment is an explicit negative site.

`corpus.json` authors the same facts and asks for every format. The binding
files choose the actual files in page order. `generation.json` pins every
asset's bytes, page plans, rendering options, dependency versions and the
generator/photo source hashes. It is generated in `kettle-examples` with
`python -m synth_letters formats --out NEW_DIRECTORY`; existing output is
never overwritten. Review a regenerated bundle before replacing committed
assets. Rendering bytes can differ with fonts or dependency versions.

Validate the committed assets without any reader or model:

```sh
python3 scripts/corpus-formats.py
```

Exercise all three actual acquisition routes without weights:

```sh
cargo build -p kettle --all-features
python3 scripts/corpus-formats.py --out evals/runs/formats-reader-01
```

The PDF route requires the vendored libpdfium reader; photo reading requires
macOS Vision. The wrapper runs `kettle corpus --no-model` into a new directory
per format and writes `formats-report.json` beside the full reports. Missing
readers and acquisition failures remain explicit. Exit 2 means at least one
case was unscored; it is not a model verdict. No result increments model
coverage. Run outside a sandbox that prevents Vision accessing its system
services; a failed system call is not evidence of poor photograph quality.

For a separately scheduled model diagnostic, use the existing corpus command
with the same `--corpus` and the appropriate `--bindings` file. Record a fresh
answer where the acquired request changes; text responses cannot be presumed
compatible with PDF/photo segmentation. Keep the exact generation directory
and bundle identity with the recording. This two-case check does not establish
photograph reliability or the full-pack ceilings.

## Initial finding on 7 September 2026 (corpus scoring v4)

The local PDF and Vision readers recovered every authored word in both
fixtures, including the final footer. They merged several short source
passages into one acquired segment. Ordered text inputs preserved those
boundaries and both cases were scorable; PDF and photo inputs each left two
cases unscored because per-ask attribution was unavailable. No model ran.

The combined report and underlying reader reports are retained locally at
`evals/runs/formats-reader-2026-09-07/`; archive them before cleanup if this
finding is cited in durable measurement evidence.

This is an acquisition/scorer integration limitation, not a reading-accuracy
score. The pipeline asks about acquired segments. A merged segment can contain
both the payment and a cancelled appointment, so assigning it to both authored
sites would falsely count a valid payment as an invention at the cancelled
site. That required an attribution rule retaining ask locations and negative sites,
without rewriting source truth or changing fixture spacing to obtain a pass.

The v4 corpus adapter named merged boundaries when the text matched, and
rejected additional acquired prose outside the authored passages. It used
`corpus-fields-v4`, since unscored acquisition changes the denominator. Pack
scoring remains 19. Missing or reordered pages and an extra rendered ask are
also exercised through the command; none is reported as an ordinary model
omission. The PDF integration test's vendored-reader skip is declared in
`quiet_skips.rs`; the actual PDF and Vision tests ran on this Mac.

Further coverage remains: independently authored layouts, genuinely long
documents under a suitable manifest, stronger degradations and missing-page
detection based on evidence available in the document. These two cases are
not a fresh challenge; see [CHALLENGE.md](CHALLENGE.md).

## Merged-passage attribution (corpus scoring v5)

The adapter now aligns the complete ordered source and acquired text after
whitespace normalisation. It records every source passage's acquired segment
ids, allowing several source passages to share a segment. Each field span
is located within its own authored passage before its acquired coordinate
is assigned. A repeated value elsewhere cannot supply that coordinate.
Missing, reordered or additional prose remains an acquisition error. A fact
or ask crossing an unsupported segment boundary still cannot be scored.

Where multiple source ask sites share a segment, distinct declared action
kinds provide the pairing: payment, response and attendance, for example.
The report names these in `kind_matched_segments`. The scorer does not choose
a pairing by whichever date, amount or party would produce a better score.
A valid payment is consumed by its payment slot and is not counted again as
an invention at the cancelled-attendance site. A lone attendance proposal
leaves the payment missing and counts as an invention at the negative site.
Reordering distinct kinds does not change the score.

This is typed-slot attribution, not independent scoring of the semantic
substance of free-form ask prose. If merged sites have overlapping kinds,
a negative site has an unspecified kind, or candidates repeat a kind or
name an undeclared kind, the case stays unscored with `attribution_errors`.
Raw answers and verified output remain in the report. The report separates attribution errors from acquisition errors and identifies
raw or verified candidate conflicts.
Single-source-site scoring retains its existing wrong-kind behaviour.

Both fixtures are now scorable in all three actual-reader arms, with four
positive asks and two negative sites per arm. The no-model rerun is retained
at `evals/runs/formats-reader-2026-09-07-attribution-v5/`; the v4 report and
all authored/rendered asset bytes remain unchanged. These are scorer/reader
checks, not new model measurements. Controlled endpoint tests additionally
exercise wrong totals, omissions, cancelled-event inventions, multiple kinds
in one segment, duplicate candidates, unknown kinds, source-kind collisions
and exact disk replay. Pack scoring remains 19.
