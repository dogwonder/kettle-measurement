# Writing a pack

`README.md` beside this file says what the builtins do. This page says
what building the second pack cost us, so the third one does not pay
again.

**Every rule below is enforced by a named test.** This page says which
test catches you and why it exists; it deliberately does not restate
the rule, because prose drifts from code and a failing test cannot.
Where a rule has no test, it says so — that is a gap worth seeing, not
worth hiding behind a paragraph.

## The order

1. Decide the typology and the harm model — what the pack asserts, and
   what it costs a person when it is wrong. That decides everything
   downstream, including how big the bed has to be.
2. Author the bed spec: composition in JSON, structure in Rust.
3. Declare ceilings **before** generating fixtures, and size the bed to
   carry them.
4. Declare the verdict shape, and check it fits the bed.
5. Measure the deterministic floor (`--no-model`).
6. Measure on development. Iterate the prompt there, and only there.
7. Spend exam once, when you are done changing the prompt.
8. Only then write thresholds — from the promise, never from the range
   you just measured.

Steps 3 and 4 come before any measurement on purpose. A ceiling or a
gate chosen after seeing a number is a number you chose, not a bar you
cleared.

## The traps, and what catches them

| trap | caught by |
|---|---|
| the sealed set is the development set with other names | `the_exam_set_is_not_the_development_set_wearing_other_names` (#299) |
| the two sets vary everything except the thing the pack is on trial for | `the_two_sets_do_not_share_a_decision` (#317) |
| a set repeats letters verbatim — every varying input is a cycle in the index, so a shape spending more families than their common period duplicates | `no_set_plants_the_same_letter_twice` (#300) |
| an expected answer the model cannot reach from what it is shown | `every_expected_party_is_named_somewhere_in_its_own_letter` |
| a ceiling the bed can never satisfy — `c` needs `n >= 3.84/c - 3.84` items with zero errors | `the_bed_carries_the_evidence_its_declared_ceilings_need` |
| a verdict rule the bed cannot support — a per-fixture bar `b` needs `n >= 1/(1-b)` decisions per fixture | `a_gate_that_cannot_read_this_bed_is_refused_rather_than_applied` (#301) |
| bars declared with no rule for reading them | `PackError::MissingEvalGate` (#301) |
| a scored step the pack sets no bar for | `a_step_nobody_set_a_bar_for_cannot_clear_it` |
| a bed that drifted from its generator | `regenerating_the_letter_bed_reproduces_it_byte_for_byte` |
| a scored item id reused after retirement | the pack-wide tombstone registry (#237) |
| an input role with nothing to show a person | `missing field 'label'` at load, and `a_role_says_what_to_call_it` (#334) |

Two of those deserve their reasoning stated rather than looked up.

**Why a sealed set has to be authored, not just labelled.** The letter
bed's exam half was, for a while, the development half with different
filenames — 355 letters, byte-identical. It read like independent
evidence and was none: the 7B scored 0.98 on both, and the wrong-answer
spreads matched down to the count of a single repeated mistake. A
sealed set that cannot disagree with the set you tuned on rubber-stamps
whatever tuning produced. Content must vary by set, and the two voices
must plant the **same difficulty in different words** — an exam voice
that quietly drops a shape's distractor is easier rather than
independent, which is the same defect wearing a different hat.

And varying content is not enough on its own: it has to vary on **the
axis the pack is on trial for**. The subscription bed's two sets differed
in statement shape, ordering, messiness and family names, and drew on
the *same 92 merchants* — so it caught a prompt overfitted to shape, and
could not catch one overfitted to the merchant list, which is the whole
of what #257 rebuilt the bed to measure. That failure is silent by
construction: a shared-merchant exam passes, and reads exactly like an
exam that generalised. Ask what your pack's claim rests on, and hold
*that* out; each pattern's brand list is now split in half, development
first and exam second, so the generator cannot spend one set's merchants
on the other however many families a set grows to.

**Why the verdict shape is a declaration.** One rule cannot serve both
packs here. `subscription-audit` carries ~10 decisions per fixture, so a
per-fixture score is a rate and gating on it is informative.
`letter-to-actions` carries one decision in 245 of its 355 fixtures, so
a per-fixture score can only be 0.0 or 1.0 — a 0.95 bar on those means
no errors at all, anywhere, and the verdict reads the same at 444/445 as
at 0/445. That is not a strict gate but a gate with no gradient, and a
gate with no gradient cannot tell improvement from disaster.

## The traps with no test

Nothing stops these, so they are the ones to hold in your head.

- **A threshold chosen to fit the number you just measured.** The whole
  point of a bar is that it precedes the result. The letter pack's 0.95
  came from `END_TO_END_BAR`, a promise already in the code; 0.93 would
  have passed on the day it was written. Say where a bar came from, in
  the manifest, next to the bar.
- **An exam voice easier than the development voice.** Varying the
  prose is not enough — see above.
- **A prompt that quotes the bed.** Naming a fixture's exact phrase
  teaches to the test and spends the sealed split without appearing to.
  Write the principle, not the example.
- **Iterating against exam.** It is spendable once per prompt version.
  Every look that then informs a prompt edit converts it into a second
  development set.
- **Run directories under `target/`.** `cargo clean` removes them, and
  with them the ability to re-score a measurement without re-running it
  (#293). A run directory is a recording: `Replay::from_run_dirs` reads
  one directly, so a scoring change costs seconds instead of hours.
- **A referral expectation on a phrase that has a correct reading.**
  `review: true` (#445) authors "the correct outcome here is a
  referral". That is honest where **no correct reading exists** — a
  bare `Excess:` in a document stating no voluntary excess anywhere,
  which no careful person could label either (#461). It is wrong where
  the phrase has an answer: `run.rs` deliberately keeps the #377
  refusal — Rust declining to compare a repeated `(term, basis)` — out
  of `needs_review`, so a *correct* reading produces no review key at
  all, and the expectation scores it as a miss on the expected side plus
  an unmatched extra on the read side. The bed then forbids the pack
  from being right, and marks down any future model that manages it.
  This is not a scoring nicety: on #462 it would have written into the
  bed that a commercial schedule's `Insurance amount:` can only ever be
  referred, when it plainly means the sum insured. Ask whether a careful
  person reading that passage alone could give the answer. If they
  could, the expectation is a term.
- **A structural property every fixture shares.** The renewal bed was
  all single-section personal policies, so the failure that met the
  first real document — values pairing across cover sections — was
  *unrepresentable* in the bed, not merely unmeasured, and no ceiling
  could see it (#378). A bed cannot test for its own monoculture, which
  is why this trap lives here and not in the table above. Before
  calling a bed done, ask what every fixture has in common, and whether
  a real document could lack it. Green evals are evidence about
  documents shaped like the bed, and nothing more.

## Measuring

- **A prompt edit is guarded.** Record before, edit, record after,
  compare with `--baseline`; exit code 1 on any drop. An unmeasured
  prompt edit is the one change this project cannot review.
- **A scoring change bumps `SCORING_VERSION`.** Baselines from another
  version are refused rather than silently compared. Changing what a
  *verdict* means counts, even when no score moves.
- **Resume is safe across a regenerated bed.** The resume key includes
  a digest of each fixture's bytes and its `expected.json`, so a
  regenerated bed cannot pass for the old one. It also includes the
  prompt digest and the scoring version — which is why editing either
  mid-run discards the work so far.

## Questions this raises, answered

**Why does the model never do arithmetic?** Because a wrong number
presented as arithmetic is worse than an honest phrase. Dates resolve
in `timeline`, money in `rust_decimal`, recurrence in
`recurrence-detect`. The model answers small closed questions; the
builtins do the sums.

And the sums being right is not the claim being right. The first real
comparison subtracted correctly all the way to a sevenfold-overstated
premium rise, because the values it paired were never comparable
(#377). A deterministic step is only as sound as the identity that fed
it — where identity is ambiguous, the step refuses rather than
chooses.

**Why does a bed author its expectations rather than deriving them?**
Deriving `due` with the same resolver the run uses would make bed and
run agree by construction, and a bed that cannot disagree measures
nothing. Each shape authors its own answer from its own composition.

**Does my pack need a `kinds` map?** Only if it has a `classify`-role
step. `kinds` says what a *recurring payment* of each category is —
rent monthly is a utility, Netflix monthly is a subscription — so it
exists to serve a derivation that only a pack sorting merchants
performs (#253). `validate_kinds` returns early when there is no
classify step, and tolerates a stray map rather than refusing it.
An extraction pack has no merchants and no cadence, so
`letter-to-actions` declares none: absent because there is nothing for
it to say, not because it was forgotten.

**Why does every input role need a `label`?** Because `role` is a
binding key and not copy. `previous` and `renewal` are what
`run_pack_bound` matches on; they are also, without a label, the only
words the app would have to put above a drop zone. The alternative is
the shell inventing them — prettifying `previous` into "Last year's
policy" in TypeScript, where the person who knows what the document is
called cannot see or change it, and where a translated pack could never
reach. It is required rather than optional for the same reason: an
optional label means the shell still needs that fallback, so the field
would buy nothing. Write it in British English, sentence case, and for
the person rather than the pipeline — "Last year's policy", not
"Previous policy document".

**How many files does a role take?** Declare `count` on the input:
`"count": 2` means exactly that many, `{"min": 2}` at least,
`{"max": 12}` up to, `{"min": 2, "max": 12}` between — so four hundred
dropped files are refused rather than read (#334 §1). The older
`multiple: bool` spelling is still accepted and normalised at load
(`true` is "at least one", `false` or absent is "exactly one"), but a
manifest that declares both is refused — two fields that can disagree
is one assertion nobody checks. Prefer `count` in new packs; the
shipped packs still carry the older spelling.

**How does a pack compare two documents?** Declare the two inputs in
the order they are compared — the *earlier* document first — a
`policy-terms` model step, and `builtin:term-diff` (#350). The model
reads each named value verbatim with the passage it came from; Rust
pairs them on `(term, basis)` and does every subtraction in `Decimal`.
Four things follow from that shape and are worth knowing before you
meet them: `basis` is part of the key, so a monthly instalment and an
annual premium never pair; `other` never reaches the diff, because it
is where the model says "this is a term you don't model" and that is a
routing answer; a value whose quote Rust cannot find in the passage
goes to a person rather than into the diff (#258), and so does a value
whose shape its term cannot hold (#380, below); and a `(term, basis)`
that either document states more than once does not pair at all —
every reading of it goes to a person with its quote (#377, below). The loader refuses a
term-diff pack that declares fewer than two inputs, or that lets either
compared input be several files — which one it compared would then be
unstated, and a comparison run the wrong way round reports a price cut
for a rise.

**Why do the two document packs' prompts and schemas look copied?**
Because they are (#394). `obligations.schema.json` and
`policy-terms.schema.json` share the passage envelope
(`{ id, segment, confidence, <items[]> }`, ~28 lines each), and the
two prompts are the same document with the nouns swapped — including
the load-bearing empty-list-discipline sentence, in two near-identical
spellings. **A fix to one does not reach the other**, so until they
share a source, edit both and measure both. They have not been merged
because prompt edits are guarded (record a baseline, change, compare —
see the eval section of `CLAUDE.md`): collapsing them is a prompt edit
to two packs at once, and it waits for someone at a machine with
weights, not for a tidy-minded refactor.

**Does my pack need a `value_kinds` map?** Yes, if it has a
`policy-terms` step — and the loader refuses it without one, complete
both ways against that step's term enum (#380). It says what kind of
value each term can hold: `"money"`, `"percentage"`, `"duration"`,
`"text"`, or a list where a term is honestly written either way
(`"no_claims_discount": ["money", "percentage"]`). A value that will
not parse as its declared kind is not a finding — it is a passage for a
person, carrying the quote, exactly as an unmodelled term is.

Pack data for the same reason `kinds` is: "a cover limit is money" is
pack policy, and a runner that knew it would be pack-specific runner
code (#51). It is required rather than optional because the missing map
is the silent case — every term would be checked against nothing while
the pack looked guarded. `"text"` is the deliberate escape hatch, and
it has to be written down.

This was found the hard way. The first real comparison report rendered
a `cover_limit` whose value was a policy period — "From <date> to
<date> both days inclusive" — paired against a monetary figure from the
other document, and nothing downstream could have caught it:
`Changed { delta: None }` is a legitimate state (a phrase change like
"14 days" to "21 days"), so a date range where money belongs is
indistinguishable from an honest phrase by the time it reaches the
diff. The check has to be against what the term says it can hold, at
the point the value is read.

**Why does a repeated term refuse to pair?** Because `(term, basis)`
is a sufficient key only while each modelled term occurs once, and
arbitrary the moment a schedule repeats the same heading under three
cover sections. The first real comparison resolved the repetition by
taking the first reading: an excess subtracted across sections, and one
section's premium compared against the whole schedule's total — both
rendered as Kettle's own arithmetic (#377). So the refusal is keyed on
the repetition itself, deliberately *not* on "scope could not be
derived" — scope derivation is a heuristic over text shape, and a
missed heading would silently restore the old behaviour. Keyed on
repetition, a future heading detector can only ever turn a referral
into a comparison, never a referral into a wrong number. Caught by
`a_term_stated_twice_in_one_document_does_not_pair` and, end to end on
a sectioned schedule, `a_repeated_term_does_not_pair_across_sections`.

**What goes in the `copy` block?** What a run says for itself on the
app's screens (#244): `time` (how long, honestly), `will` (what a run
will do, numbered, in the pack's own words) and `run_verb` (the words
on the Run button). Required — the missing declaration is the silent
case, exactly as `value_kinds` is, and the shell's fallback branch
retired when this became so. A pack without one is refused with a
message that says what to write
(`a_pack_without_copy_is_refused_and_says_what_to_write`). Two rules
inside it:

- A `will` entry may name the progress steps it covers in `steps`, and
  a name that resolves to no step label fails the load
  (`a_will_entry_naming_an_unknown_step_is_refused`). Coverage is not
  forced — prose stays free to group six steps into three sentences a
  person cares about.
- `time` carries a real figure only where a measurement exists;
  otherwise say plainly that the pack has not been timed yet. An
  invented "about 4 minutes" is #213's defect in a manifest.

The progress sequence itself is never authored: it derives from the
pipeline (`run::step_labels`, the same source a run emits from), so the
screen's prediction cannot drift from what the run reports. Write the
block in British English, in the plain-language register — it goes
straight to a person deciding whether to run you.

**Must a report template mark its claim kinds?** Yes. Every claim a
report renders says what kind of claim it is (#366, #367): read off
the page, worked out by Rust's arithmetic, or counted from something
the person themselves gave. The kind is derived in the runner
(`claim::Kind`, carried on the payload) — the template's only job is
to show it, each kind in its own words a person can act on, never to
decide it for itself. `claim_marks.rs` renders every kind in
`Kind::ALL` through every pack's template and fails a template whose
renderings do not differ — so a new pack meets this test the day its
template exists. A pack that deliberately does not mark its claims yet
is staged there with a reason and a date, never silently.

**How does a fixture carry two documents?** Its `expected.json` names
them, by role (#354):

```json
{ "fixture_id": "renewal-01",
  "inputs": { "previous": "renewal-01-previous.txt",
              "renewal":  "renewal-01-renewal.txt" } }
```

The fixture is then named after the expectations rather than after any
one document, and both documents are hashed into its digest — so two
fixtures differing only in their second file are different questions to
the resume cache. Omit `inputs` and nothing changes: the document beside
the file, bound to the pack's sole role, which is what every fixture
written before comparison packs is. A role the pack does not declare, or
a file that is not there, is refused at discovery — before a sidecar is
spawned and before the first fixtures are spent.

**Why is needs-review a cost and never a failure?** It is the appliance
working as designed — saying "I am not sure" is the correct answer to
an unclear input. It is reported (`eval_costs`) so the trade is visible,
and deliberately absent from the verdict so it can never be gamed into
one.

**Why is harm read by a Wilson upper bound and quality by a lower one?**
Both refuse to be flattered by a small sample, pointing opposite ways.
A harm rate must be demonstrably *below* its ceiling, so it is read at
its worst plausible value; a quality rate must be demonstrably *above*
its bar, so it is read at its worst plausible value too. One
consequence to know before meeting it as a bug: a quality rate sitting
exactly on its bar never clears it, at any sample size.
