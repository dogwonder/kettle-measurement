# app/study — the participant harness (#431)

The instrument for the seeded-error study. Separate from the product,
because #431 says so in as many words and because a study needs
conditions the product must never grow — starting with a report shown
without its evidence.

Run it with `bun run study:dev` (port 5174) or build it with
`bun run study:build`. It is tested by the app's own vitest suite
(`study/study.test.ts`), so a change to a product component that breaks
the harness goes red in one place.

## 27 August 2026: letters, and the author as the only tester

Two amendments, recorded on #431 the same day.

**The corpus is synthetic letters.** The `kettle-examples` generator
produces UK letters that are invented by construction and that nobody
has read before a session; `crates/runner/examples/study_letters.rs`
runs the letter pack over them for real and writes
`fixtures/study/letters/`. The letter pack is where the product's harm
model actually lives — a missed obligation is the unrecoverable error —
and #431's own seed list (wrong deadline, phantom obligation, missing
obligation) was letter-shaped from the start; the statement corpus
could seed none of them. The statement track stays built and tested,
selectable with `?material=statements`; letters are the default.

**The author sits the sessions, and that makes them a pilot.** An
`a`-numbered id (`a01`) marks the transcript `author-pilot`; it is
never one of the twenty and never pooled. The frozen primary is
unchanged and unmet until twenty people are recruited. What a pilot
can establish, and how to run one, is in `PILOT.md`;
`scripts/study-pilot.sh` is the same steps as a script.

The letter track's three operators are separated by what checking
does, as the statement track's are: `misquoted-deadline` (the card
quotes words the letter does not say; opening the passage refutes it),
`misresolved-deadline` (the quote is genuine and the date worked out
from it is wrong; opening the passage confirms every word), and
`dropped-obligation` (the ask is not on the page; only the letter shows
it). See `src/lib/study-letter.ts`.

## 28 August 2026: what the first sitting found

The author sat ten letters under the instrument as built. Four things
were wrong with it, and none of them could have been found by reading
the code, because every one of them was green.

**The scorer could not see a detection.** The check step asked *"Which
one? Name it"* into a free-text box, and both scorers attribute a
rejection by comparing that answer to the claim's title exactly. A
person writes "the 6th June date is wrong". No prose matches a title,
so every rejection scored `false-alarm` — ten out of ten on the first
sitting, on seeded and clean reports alike, and the primary measure
would have come out zero from twenty participants who did nothing
wrong. The harness's own acceptance test passed because it *typed the
gold answer into the box*: it asserted the contract rather than the
behaviour.

So the question is now a list of the report's own claims, picked rather
than typed, with one option that is not a claim — `ABSENT`, in
`study-response.ts`. The omission operators drop a claim, so their gold
answer names something that is not on the page and no list of what is
there could offer it. Attribution is exact both ways: pointing at a
card on an omission is a false alarm, and so is calling an invention an
absence. **Declared cost:** the list shows that a missing ask is a
thing one could report, which the box did not. That is a cue, it is
identical across arms, and it lands on the exploratory omission class
rather than on the frozen invention-versus-mis-relation primary.

**The floor refused every letters transcript.** `corpus` holds the pool
a session was drawn from, so `rebuild` can refuse a corpus that is not
the one the participant saw. The floor required exactly ten ids, which
was true of the statements by coincidence — there are ten reports — and
false of all eighteen letters.

**The consent form described a file it does not get.** It said the
transcript records "which ten documents you were shown"; it records the
eighteen the ten were drawn from, so it named eight documents nobody
saw. The key-list mechanism could not catch this, because the key was
right and only its meaning was wrong — which is the limit of that
mechanism, now written down. Version 2026-08-28 fixes the wording.

**The id was typed, not issued.** `scripts/study-pilot.sh next` printed
`a01` to a terminal; the gate suggested `p01` in a placeholder; the
author typed what was in front of him, and the file came out claiming
to be one of the twenty. A transcript cannot be renamed after the fact
— the session's whole plan is drawn from the id — so that sitting
cannot be published at all. The id now travels in the link
(`?participant=`) and the box is read-only when it does.

The sitting itself is kept as an instrument shakedown, not a
transcript: it is not in `fixtures/study/transcripts/`.

`a01` was then sat under the repaired harness, and it found a fifth
thing — a clean control that was not clean. The pipeline had dropped an
appointment from it, the answer named the missing obligation in full,
and it scored `false-alarm`. Two changes came out of that (#577): the
scorer is told, from the signed audit, which documents the pipeline got
wrong on its own, and `caught-a-natural-error` says so where a false
alarm would otherwise be recorded; and `planLetters` now draws its two
**clean controls only from letters the audit calls clean**, so the cell
the false-alarm rate is measured on is not contaminated at source.

That second change moves the draw, and a session rebuilds from its
participant id — so `a01`'s own answers to its two clean tasks would be
scored against letters it never showed. Its transcript is therefore
retired to the same shakedown status as the first sitting (it is in the
history at `417edf7`, and its numbers are on #431), and the next
session is `a02`. A pilot cannot produce a quotable rate, so what a
pilot is worth is exactly what these two sittings have already paid
out.

`a02` was then sat under the repaired instrument and **detected eight
of eight seeds** — invention 3/3, mis-relation 3/3, omission 2/2 — in
5.8 minutes, median 33 seconds a task, against 6.6 minutes when the
answer was typed. Picking is faster and cost nothing in accuracy, and
the detection is discrimination rather than rejection: the scorer wants
the exact card, and with two to four cards a report, chance is well
under half.

It found two more things.

**Nobody has ever accepted a report.** Thirty tasks across three
sittings, thirty rejections. `correct-acceptance` and `false-acceptance`
are empty in every session sat so far. That is expected of the author,
who knows errors are seeded, and it is why a pilot cannot produce a
rate — but it is also a hole in the design for the twenty: nothing here
would catch a participant who rejects everything, and two clean controls
each is thin cover. A reject-everything strategy scores 100% detection
under the current scorer.

**The audit is blind to a deadline's wording.** `letter-10`'s bed
expects "on 11 April 2026 between 8am and 6pm" and the pipeline proposed
"on 11 April 2026" — the visit's time window dropped. `audit-letters.py`
matches on party and due date, so it saw nothing; a participant saw it
immediately. That is the third real error the audit's shape check has
missed (the others: an invented anchor filed as an "extra", and a
dropped appointment). The audit is a floor, not a reading.

**A session now records the ten documents it drew.** Marking `letter-10`
unclean moves the draw, and that is how `a01` came to be scored against
letters it never showed — silently, in a table that looked perfectly
ordinary. The corpus list catches a corpus that changed; it cannot catch
a *judgement* that changed. So `drawn` joins the transcript, `rebuild`
refuses a plan that disagrees with it, and the consent form lists it.
The same principle as the consent stamp: record what was shown, do not
recompute it. `a02` is retired for the reason it discovered, and is the
last one this can happen to quietly.

**`unclean` travels on `StudyCorpus`**, not beside it, because two
things need the same answer: the draw, which must not use an unclean
letter as a control, and the scorer, which must not call a catch on one
a false alarm. One declaration, so a harness and a scorer cannot come
to different views of the same session.

## 28 August 2026, later: the answer is a set

Two notes from sitting it, both about the answer rather than the
letters.

**"Which one?" asks a question the reports do not always have one
answer to.** Three of the author's first thirty answers put a second
complaint in the correction box, having nowhere else to say it — *"Wrong
date: 6 May 2026. And missing payment …"*. Worse for the measure, a
participant who saw the seed *and* something else had to choose, and
choosing the other one scored a false alarm on somebody who had seen the
seed. So the claims are tick boxes, `ABSENT` may be ticked alongside one,
and leaving them all unticked is the "I can't say" it replaces.

**That needs a scoring rule or it is a downgrade**, because ticking
everything would then detect every seed. Each ticked claim carrying no
seed is scored as a false alarm of its own, against `claims_offered` —
the claims the report actually put in front of them, recorded in the
answer for the same reason `drawn` is recorded. Tick everything and the
table shows a hit rate of one beside a false-alarm rate of one, which is
visibly uninformative where `correct-detection` alone read as perfect.

It is also a better-powered instrument: each participant goes from ten
binary decisions to thirty or forty claim-level ones, which is the thin
false-alarm cell the same sitting exposed.

**The correction box is a textarea**, and **length no longer refuses a
file.** The floor rejected any free-text answer past 300 characters,
while describing that answer as one that "deserves a person's eye" — a
routing decision built as a validity one. Since a participant's file may
not be edited after they hand it over, somebody who explained themselves
at length produced an unpublishable transcript having done nothing
wrong. `closeReading()` now points at those answers and
`scripts/study-pilot.sh` prints them before asking who read them.

## What a session is

Ten reports, drawn from `fixtures/study/` by `plan(participant, corpus)`:
three carrying a seeded invention, three a seeded mis-relation, two an
omission, two clean. Order, seed assignment and the emphasised claim are
drawn from a generator seeded by the participant's own id, so a finished
session rebuilds from the id alone. The mix, the sample size and the
primary comparison are frozen on #431 — read the pre-registration
comments there before changing anything in this directory.

**Every participant is in condition 3.** #431's frozen pre-registration
promised the primary sixty pairs *and* split twenty participants across
two arms at ten each, and both cannot hold: ten participants in
condition 3 give thirty pairs, and the minimum detectable difference
moves from thirty points to about forty. Condition 4 was already
exploratory, predicted to show no movement, and able to see only a
~40-point difference, so it is not run here (26 August 2026). The
emphasised presentation stays built and tested, ready to be a study of
its own.

Each task is two steps, and the gate is deliberate:

1. **Read.** The report's figures with nothing behind them, and the
   question *how confident are you that this report is right?*
2. **Check.** The evidence disclosures and the source statement arrive,
   and the answer is given, followed by the same question again.

Without the gate, "before checking" would mean whatever each participant
happened to do first. The gate is identical for every arm and every
error class, so it cannot bias the primary comparison; it costs realism
and buys a measure that means one thing.

The statement is part of the task, not a convenience. An invention and a
mis-relation can both be checked inside the report — a wrong amount
disagrees with its own transaction chips, a wrong period with the median
interval printed beside it. An omission cannot, so without the source
document its detection rate would be zero by construction, and a zero
the harness produced looks exactly like a finding about people.

## Where the pieces live

| Piece | Where |
|---|---|
| Seeding one named error, and its gold answer | `src/lib/study-fixtures.ts` (statements), `src/lib/study-letter.ts` (letters) |
| What one participant sees | `src/lib/study-session.ts`, `src/lib/study-letter-session.ts` |
| Scoring one answer | `src/lib/study-response.ts`, `src/lib/study-letter-response.ts` |
| Scoring from the command line | `scripts/study-score.ts` (`bun run study:score <transcript>`) |
| The transcript | `src/lib/study-record.ts` |
| The consent text, and the stamp it leaves | `src/lib/study-consent.ts` |
| The corpus and the statements | `study/corpus.ts` |
| The report, the statement, the letter, one task, the session | `study/*.svelte` |
| Running a pilot on yourself | `study/PILOT.md`, `scripts/study-pilot.sh` |

The scoring, session and transcript layers are under `src/lib` rather
than here because they are pure and belong with the tests that drive
them; the harness is the four Svelte files and the corpus loader.

## The consent text

`src/lib/study-consent.ts` **is** the form. The gate renders it and the
transcript stamps it, from one object, because the version used to be a
constant in `study-record.ts` while the words were paragraphs in
`StudyApp.svelte` — editing the copy moved neither, so a transcript
could name a document that said something else.

The stamp carries the version *and* a digest of every word rendered
under it, so two participants a fortnight apart cannot come out of the
analysis looking like they read the same page. `study-consent.test.ts`
pins that digest: **if it fails, you edited the text**, and the fix is to
set `version` to the day the new wording comes into force and paste the
new digest in the same commit.

Two of its promises are held by something rather than intended. The
"what gets written down" list is the transcript's own key list in a
participant's words, so the form cannot describe a file the harness does
not write; and a source scan over the harness and every module it
imports fails on `fetch`, `XMLHttpRequest`, `sendBeacon`, `WebSocket` or
`EventSource`, which is the small version of what
`crates/privacy-audit` does for the product.

### The arrangement, settled 27 August 2026

- **DGW Ltd holds the answers**, for now.
- **No list connects a participant number to a person.** Not "kept
  securely" — never written down. So there is nothing to look anybody up
  in, and the form says plainly that this also means a file cannot be
  taken back once handed over: with nothing to search, there is nothing
  to delete. The right that remains is real and sufficient at this scale
  — do not hand it over. The 26 August wording promised deletion by
  participant number, and that promise had to be withdrawn rather than
  softened, which is what the version bump records.
- **All twenty transcripts are published** alongside the findings, so a
  result can be re-read rather than re-gathered. Hence the one line
  beside the free-text box in `TaskScreen.svelte`: a warning read once
  on the consent page, ten reports earlier, is not where the risk is.
- **Contact:** Rich Holman.

The transcripts go to `fixtures/study/transcripts/`, which is inside the
published boundary — see that directory's README for the floor every
file passes and the read every file is signed for. This harness and its
scoring modules are published with them, because a rate nobody can
recompute is decoration. The Svelte files here import the product's
report components, which stay closed: the public tree can be read and
re-scored, not built.

**A gap in the text refuses a session**, and the mechanism stays now the
two original gaps are filled. A `[UNSETTLED: field]` marker with a
declared reason and date renders in place, raises a notice, and refuses
Start in the words of the gap — and `study-consent.test.ts` fails until
somebody writes the words, because declaring a hole is no longer enough
once people are being asked to agree to this. `study.test.ts`
demonstrates the refusal on a document with a gap, since the shipped one
has none.

## Two things to know before running it

**The report here is assembled from the app's components, not from the
runner's `report.html`.** Three of #431's four conditions are the same
report shown differently, and a rendered document can only be one of
them — a re-renderable report is what the conditions require. Today the
desktop app shows `report.html` in an iframe, so this assembly is not
yet the artefact a Kettle user reads. `study.test.ts` holds the two to
the same figures for the same run, which bounds the gap to presentation
rather than content. It closes for good when the app's own report screen
is built on these components.

**The emphasised claim is drawn, not ranked.** It used to be the claim
that costs most if it is wrong — the largest annualised figure, ranked
on the clean report so the seed could not choose it. That stopped the
seed choosing the emphasis but not from changing which row *looks*
biggest: an invention multiplies its row by ten, so on those tasks the
emphasised row was no longer the largest on the page while on the other
seven it was, which over ten reports is learnable and marks one of the
two classes the primary compares. Drawing removes the rule rather than
muffling it. The cost is "high-risk", and at ten per arm the question
condition 4 could answer was never which claim deserves the emphasis but
whether one already-open disclosure changes what a person does.

## Nothing is sent anywhere

The transcript is written by the participant's browser and shown to them
at the end to hand over. It carries what they said and nothing the
product produced — no findings, no input hash, no run. That separation
is a "done when" clause on #431, and `study-record.test.ts` asserts it
on the serialised form, which is the artefact that leaves the machine.

`noindex` is on the page because a study instrument left where it can be
found is a study instrument somebody has practised on.
