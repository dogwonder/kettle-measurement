# fixtures/letter-01

**A recording, not a worked example.** One real run of the letter pack,
taken from the app on 18 August 2026 and copied here whole. Nothing in
it was authored, tidied or re-rendered: `results.json` is what the
runner wrote, `report.html` is the document that run produced with the
stylesheet it was born with, and `raw/` is the prompt as it was sent and
the answer as it came back.

That is the difference from `fixtures/run-01/`, which is a synthetic
example of a *shape* — its creating commit says so — and which the
public demo used to show while its footer had to admit it was not a
recording.

## What was run

| | |
|---|---|
| Pack | `app.kttl.letter-to-actions` 0.2.0 |
| Input | `packs/app.kttl.letter-to-actions/fixtures/generated-development-three_asks-linden-04.txt` |
| Input hash | `blake3:721a7ca15f49fdf4d3f77df2a81a7d6ad00d18a404fd7d0c44de1524f0b20dcf` |
| Model | `qwen3.5-4b-q4_k_m.gguf` (4B, Q4_K_M, context 8192) |
| Sidecar | llama-server 10145 (ad256ded3), Metal |
| Machine | Apple M1 Pro, macOS 26.5.2 |
| Recorded | 2026-08-18, 26 seconds wall clock |

The letter is a committed bed fixture, so it is wholly synthetic and its
expected answer is published beside it
(`generated-development-three_asks-linden-04.expected.json`). Anybody can
check this recording against what the bed says is right, which is the
whole reason the demo uses a bed letter rather than a prettier invented
one.

## What it shows

Three asks read out of one letter, all three dated correctly:

- **payment** — £480.00, "within 14 days", due 30 April 2026 (`worked_out`:
  16 April plus fourteen days, arithmetic Rust did);
- **response** — "by the end of the month", due 30 April 2026 (`worked_out`);
- **attendance** — 26 November 2026 (`read_and_verified`: that date was on
  the page, so nothing was counted).

Nothing in needs-review, and every claim quotes the passage it came from
verbatim. The two claim kinds visible here are #366's point, and are
exactly what the subscription report cannot yet render honestly.

## Consumers

- the public demo (`app/demo/`) replays this run in the browser;
- the frontend tests drive the screens from it.

`fixtures/run-01/` stays where it is. It is the committed example of the
`kettle/run-report@0` wire shape, the input to the actions emitter's
tests and the renderer's staleness guard, and the letter pack's report is
a different schema (`kettle/letter-report@0`) that replaces none of that.

## Re-recording it

Run the pack in the app on the same fixture with the same model, then
copy the run directory here. Do not hand-edit anything in it: a record
that was corrected afterwards is not a record. If the report needs to
change, the thing to change is the template, and then this has to be
re-run rather than re-rendered — reports keep the stylesheet they were
born with, deliberately.
