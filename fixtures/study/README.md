# fixtures/study

The corpus for the seeded-error study (#431): ten synthetic statements,
and the ten reports Kettle produced from them.

## Why ten

Each participant reads ten reports — three carrying a seeded invention,
three a seeded mis-relation, two a seeded omission, two clean. Ten seeds
spread over one document would show the same five merchants ten times,
and by the fourth report the participant is studying the harness rather
than reading a report. `study-session.ts` refuses a corpus smaller than
ten for that reason.

## The statements

`make-statements.py` generates `statement-01.csv` … `statement-10.csv`.
Ten distinguishable financial lives — a flat share, a family, a
freelancer, a student, a retiree, a small trader, a new parent, a
commuter, a musician, a carer — each with at least four recurring
commitments, at least one price rise no earlier than February, at least
one non-yearly commitment, a long tail of one-off spending and an income
line.

Everything is invented: invented amounts, invented dates, invented
sequences, no row traceable to anybody. Real public brands appear as
descriptor text only, per the 30 July 2026 amendment — the word
"Netflix" discloses nothing about anyone — beside a long tail of
invented merchants nobody could recognise, because *"I don't recognise
this, surface it"* is a correct answer a report has to be able to give.

Three statements carry deliberate descriptor noise (`STRIPE* ADOBE` /
`SQ *ADOBE CC` / `ADOBE CREATIVE CLOUD` for one commitment), which is
the normalise step's actual job.

## The reports

`report-01.json` … `report-10.json`, produced by running the
subscription pack **for real** — the same `run_pack` the desktop app
calls, on Qwen3.5-4B Q4_K_M, one sidecar for all ten:

```sh
cargo run -p runner --features pdf --example study_corpus -- \
  --model ~/Library/Application\ Support/app.kttl.kettle/models/qwen3.5-4b-q4_k_m.gguf
```

Genuine rather than authored, decided 25 August 2026: a report Kettle
*would* produce is a different artefact from one Kettle *did* produce,
and any error the pipeline makes on its own tells the study more than a
corpus with none.

## The audit, and why it is not optional

`audit.py` reads every report against the statement that produced it.
A "clean" report is only clean once somebody has checked it — a
participant who catches a natural error in a clean control would
otherwise be scored as a false alarm while being right, which inverts
the measure the clean pair exists to give.

As recorded on 25 August 2026, at pack 1.5.0:

```
matched 44  wrong 0  missed 0  unexpected 0  not-a-series 6  label 7
```

- **44 series found, none missed, none invented, no wrong amount, no
  wrong period, no wrong price rise.** Every rise landed on the right
  month, including through the descriptor noise.
- **Six commitments were not series to find** — a single yearly payment
  is a fact about the year, not a recurrence, and its absence is
  correct.
- **Seven labels are wrong**, and they are the corpus's natural errors.
  Six use `other` where the classify schema's own enum offered a plainly
  better member (four housing costs, a sport membership, card fees). One
  is positively wrong — a pharmacy delivery labelled `food_drink` — and
  it is the only one shown at **high** confidence. The other six are
  `medium` and were routed to "check yourself", which is the pipeline
  hedging correctly on the ones it got wrong.

That last group is the interesting shape: **every figure right, the
label wrong** — the same class #568's rung 1 found in prose, arriving
here without anybody seeding it.

`audit.json` carries the per-report notes so scoring can tell a
participant who flagged a natural label error from one who raised a
false alarm.

## Regenerating

```sh
python3 fixtures/study/make-statements.py   # statements
cargo run -p runner --features pdf --example study_corpus -- --model <path.gguf>
python3 fixtures/study/audit.py             # audit.json, and the counts above
```

Re-running the pipeline re-measures: a different model, a different pack
version or a different scoring version can move what the reports say,
and the audit's counts are true of the run recorded above and of nothing
else.
