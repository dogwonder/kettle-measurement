# crates/runner/examples

Measurement harnesses. Each one runs the shipped pipeline pieces against
something the eval bed cannot reach, and each is a real thing to copy
and run — `documented_commands.rs` (#418) checks every command on this
page names a package this workspace builds and an example file that
still exists.

They are examples rather than CLI subcommands because none of them is a
thing Kettle does for a person. The CLI's surface is the product's;
these are instruments.

All of them want `--all-features`, or the feature they name.

## `rung1_accounts` — the invention floor over filed accounts (#568)

The instrument behind a pathway decision. #474 made the summariser pack
conditional on a pre-registered gate: does the raw model invent on
filed charity accounts often enough for a containment bed to have
anything to measure? The floor was 5%. It read **1 invented claim in 470
judgeable ones — 0.21%, Wilson [0.04%, 1.20%]** — so the upper bound is
a quarter of the floor, the gate is excluded rather than merely unmet,
#568 closed and #474 took option 3.

```
cargo run -p runner --features pdf --example rung1_accounts -- \
  --corpus <dir of *.pdf> --model <path.gguf> --out <dir> [--arm a|b|both] [--plan]
```

`--plan` prints the call count and stops, which is how to find out what
a corpus costs before spending it: the recorded run was 270 calls in 86
minutes on an M1 Pro.

Two arms over the same corpus, both declared before the corpus was
fetched:

- **A, closed** — the summariser's closed questions, schema-valid, with
  nothing in Rust checking the answer. The rung the 5% floor is read
  against.
- **B, prose** — the same documents explained in plain English, quotes
  unverified. Diagnostic only: it says whether this material can produce
  invention at all, which is what tells "the accounts are easy" apart
  from "closed questions already removed it".

**The corpus and the outputs never enter this repository, and never
enter `kettle-runs`.** They are real filed documents carrying trustee
names, so they stay on the machine that ran them under the same rule
that keeps `*.private.*` out of the tree. What travels is the write-up
on #568 and this harness, which reproduces the corpus from the register
and the judging from a run directory without the GPU.

`examples/rung1/` holds the half that needs no weights — the sampling
rule, the batching, the id join — so `tests/rung1_sampling.rs` can hold
it to the properties the rate depends on. Every one of those functions
can fail silently: a dropped passage shrinks the denominator, a missed
join drops a claim from numerator and denominator together, and neither
would raise anything during eighty-six minutes of GPU time.

Two things the run found about itself, kept here because they are about
the instrument rather than the result:

- **A proximity check is not a correctness check.** Arm B's presence
  rule counted 21.6% of figures as absent from the chunk given, and 106
  of those 118 were within 0.5% of a real figure — restatements in
  millions, not fabrications. The same proximity that exonerates them
  hides real errors, because in a document dense with figures every
  wrong number has a near neighbour. Anything judging figures against a
  financial table needs exact placement. Trusting nearness here would
  have reported a clean 4.5%.
- **Containment was in the prompt, not the guards.** Every Rust check
  was off; what was on was the instruction to copy figures "exactly as
  printed". That is the mechanism behind #432's flat ablation ladder and
  its `prevented` = 0 — the scorecard measures Rust checks over
  candidates the prompt has already made faithful.

## `study_corpus` — the ten reports behind the seeded-error study (#431)

Runs the subscription pack for real over `fixtures/study/statement-*.csv`
and writes the reports beside them, through the same `run_pack` the
desktop app calls.

```
cargo run -p runner --example study_corpus
```

The reports the study shows are genuine pipeline output, not authored: a
report Kettle *would* produce is a different artefact from one Kettle
*did* produce, and any error the pipeline makes on its own tells the
study more than a corpus with none. `fixtures/study/audit.py` is what
makes a "clean" control honestly clean.

## `read_photo` and `compare_readings` — the photographed letter (#399)

macOS only, behind `--features vision`.

```
cargo run -p runner --features vision --example read_photo -- <path>
cargo run -p runner --features vision --example compare_readings -- <path>
```

`read_photo` reads one image through the Vision text reader;
`compare_readings` reads it twice and shows where the two disagree,
which is the signal behind the disputed-reading mark. Vision's own
confidence is not that signal — it reported 1.000 on a line where `5QT`
had been read as `50T`.
