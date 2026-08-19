# See what changed in a renewal

Last year's policy and this year's renewal, and what changed between
them. #66.

`AUTHORING.md` step 1 is "decide the typology and the harm model — what
the pack asserts, and what it costs a person when it is wrong". This is
that, written down **before any fixture exists**, so the ceilings that
follow can be read against a claim rather than against a measurement.
Dated 3 August 2026.

## What it asserts

One thing, per named value: **this document states this value, on this
basis, and here are the words it comes from.** The comparison follows
from those readings and is deterministic Rust (`terms.rs`, #350) — it
does no reading of its own.

So this pack sits on the **attention** side of the knowledge/attention
triage. It needs no knowledge about insurers, products or what is a
good price; it needs a document read carefully. That is the difference
between it and the subscription audit, which was sidelined in #348
because its central claim rests on inferred merchant identity, and
identity is never given by the source.

Every claim here carries its own proof. "Your compulsory excess rose
from £250 to £500" traces to two quoted passages that Rust has verified
exist in the two documents (#258). Verification is per-claim and local,
which is why completeness over the input space is not what has to hold.

## What it costs when it is wrong

Three harms, deliberately not equivalent:

1. **A value read from the wrong year.** The worst of the three, and
   unique to a comparison: it does not fail, it reverses. A rise
   reported as a cut is worse than no answer, because a person acts on
   it and has no way to see the mistake. Rust holds the attribution —
   role comes from the segment's document (#330), never from the model
   — so this is a Rust bug with a test rather than a model error, and
   the bed scores it (#356).
2. **A value missed.** A term stated in both documents that the run
   never reads produces no row. The person believes nothing changed
   about it. Recoverable only by reading both documents themselves,
   which is the job they came here to avoid.
3. **A value invented.** A number attached to a term the passage never
   stated. It costs worry and a phone call, and it is the most visible
   of the three: the quote is on the page beside it, so it can be
   checked. #258's guardrail makes it checkable *before* it is shown —
   a quote Rust cannot find in the source never becomes a finding.

The ordering matters for the ceilings: a miss is held tighter than an
invention, as it is in `letter-to-actions`, because an invention comes
with the evidence that refutes it and a miss comes with nothing at all.

## Intended ceilings, and why they are not here yet

`AUTHORING.md` step 3 says declare ceilings **before** generating
fixtures, and size the bed to carry them. Those two land together, in
the bed's own change, because `every_declared_ceiling_has_the_distinct_
decisions_it_needs` (#310) rightly fails a ceiling no bed can support.
What this file fixes now is the *claim*, so the numbers cannot be
chosen to fit a measurement later:

- **`value_stated`** — passages stating a named value, where a miss is
  measured. The tighter class, per the harm ordering above.
- **`states_nothing`** — passages stating none, where inventions are
  measured.

Both read by a Wilson upper bound, as every harm ceiling here is: a
harm rate must be demonstrably below its ceiling, so it is read at its
worst plausible value.

## What this pack cannot do yet

Said here rather than promised in the manifest:

- ~~**It produces no report.**~~ Done 4 August 2026. The Comparison
  typology has its own document (`comparison_report.rs`,
  `kettle/comparison-report@0`) and this pack has a template, for the
  reason the other two have their own: a shared shape with half its
  fields empty makes every reader guess which half meant anything
  (#238). The page leads with what moved, names which document is
  which before comparing anything — getting the two the wrong way round
  reverses the answer rather than failing — and marks each row as read
  or worked out (#367). Rendering it from the CLI still needs a run to
  render, which is the bullet below.
- **A person cannot run it.** The app and the CLI flatten every role's
  `accept` into one union (#334 §3), so there is one file picker and no
  way to say which document is last year's.
- **Exclusions are out of v1** (#66's scoping, 3 August). An exclusion
  is a paragraph, not a named value: no enum, so no pairing, and
  comparing two prose exclusions for equivalence is a knowledge
  question — the shape that sank the subscription pack.
