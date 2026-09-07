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

## Finding on 7 September 2026

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
site. The next mapping change must handle that ambiguity explicitly, retaining
ask locations and negative sites; it must not rewrite source truth or change
the fixture spacing merely to obtain a pass.

The corpus adapter now names merged boundaries when the text matches, and
rejects additional acquired prose outside the authored passages. It uses
`corpus-fields-v4`, since unscored acquisition changes the denominator. Pack
scoring remains 19. Missing or reordered pages and an extra rendered ask are
also exercised through the command; none is reported as an ordinary model
omission. The PDF integration test's vendored-reader skip is declared in
`quiet_skips.rs`; the actual PDF and Vision tests ran on this Mac.

Further coverage remains: independently authored layouts, genuinely long
documents under a suitable manifest, stronger degradations and missing-page
detection based on evidence available in the document. These two cases are
not a fresh challenge; see [CHALLENGE.md](CHALLENGE.md).
