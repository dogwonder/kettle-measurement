# Running the study on yourself (#431, 27 August 2026)

The author is the only participant for the foreseeable future, so what
runs is a **pilot of the instrument**, not the pre-registered
measurement. This page is the playbook; `scripts/study-pilot.sh` is the
same steps as a script.

## What a pilot can and cannot say

A session sat by the person who wrote the seeds is the worst case for
learnability — you know the three operators and roughly what each does
to a card. So:

- It **can** establish that the task is doable in the time the consent
  form promises; how long a letter takes; whether the two-step gate and
  the four questions work; whether a seed is legible from the page
  (if the author cannot tell which card carries it without opening the
  evidence, nobody can); and whether the transcript floor, the
  signature step and the scorer run end to end.
- It **cannot** produce a detection rate anybody may quote. The primary
  comparison — invention against mis-relation, paired, within
  participant, n = 20, 30-point MDE — stays frozen and stays unmet. An
  `a`-numbered transcript is marked `author-pilot` in the file and is
  never pooled with the twenty.

The letters are the one thing that makes a self-run worth anything: the
corpus is drawn from a generator you have not read the output of, and
the seed is drawn per session from your id, so within a task you are
reading a letter for the first time even though you know the operators.

## Before the first session

1. **Corpus.** `scripts/study-pilot.sh corpus 14` runs the letter pack
   for real over fourteen synthetic letters from
   `../kettle-examples/out-bed` and writes `fixtures/study/letters/`.
   About a minute a letter on the M1 Pro. Regenerating after a
   transcript exists breaks that transcript's rebuild, and the script
   refuses.
2. **Audit.** The same command runs `fixtures/study/audit-letters.py`
   and writes `audit-letters.json`: every `extra` and `missed` is either
   a natural pipeline error or a fair reading the bed did not list, and
   a person decides which before any letter is a clean control, filling
   in `read_by` and `clean`. **The author does this after sitting, not
   before** — the next section says why.
3. **Tests.** `cd app && bun run test` — the letter session tests need
   ten letters and fail loudly with fewer.

## The author reads the audit last (28 August 2026)

Reading the audit means reading the letters, and an unread corpus is the
only thing a self-run has. So for the author the audit moves to after the
session and before the score, and the order costs nothing, because
**nothing gates on it**: no module reads `audit-letters.json`, and
`plan(participant, corpus)` draws its two clean controls from the corpus
directory rather than from the `clean` field. Its consumer is scoring —
the same job `audit.json` does for the statements, telling a participant
who caught a natural error from one who raised a false alarm — and
scoring happens in `study-pilot.sh file`, after the last screen.

What it costs is that the author audits knowing what they answered. That
is a real bias and it is recorded rather than avoided: `read_by` names
who read it, and the corpus, the bed's expectations and the transcript
are all published, so anybody may re-read the twelve flagged letters and
disagree. For anybody who is not the author, the order above stands —
there is no freshness to spend, so spend nothing.

## Sitting a session

1. `scripts/study-pilot.sh next` says which id to use (`a01`, `a02`, …).
2. `scripts/study-pilot.sh serve` starts the harness and opens it on
   that id — the link carries `?participant=`, so the box on the gate is
   already filled in and cannot be edited. Read the consent page as a
   participant would; it is the text in force and it stamps the file.
3. Ten letters, two steps each. Answer what you actually think — a
   pilot that games its own seeds measures nothing about the
   instrument.
4. On the last screen, copy the file and save it as
   `fixtures/study/transcripts/aNN.json`.

## Taking the file in

Audit first if you are the author: read `audit-letters.json` against the
letters now that the session cannot be spoiled, and fill in `read_by`
and `clean`. A flag on a natural error scores as a false alarm until
somebody has said which notes are errors.

`scripts/study-pilot.sh file fixtures/study/transcripts/a01.json`:

- reads the participant id out of the file and puts it in place;
- asks who read the free-text answers and signs for it in `READ.json`
  — the one check no floor can do, recorded rather than intended;
- runs the transcript floor (`study-transcripts.test.ts`), which
  refuses a key the consent form does not list, a stamp whose digest
  does not match the words in force, or an answer the harness could
  not have produced;
- scores it (`bun run study:score`), per class before any overall
  figure.

Commit the transcript and `READ.json` together. `fixtures/` is inside
the published boundary, so the file is public the moment it is
committed; the id form and the `role` key are what stop it being
mistaken for one of the twenty.

## What to write down afterwards

On #431, as a comment: elapsed time per task (the scorer prints it),
anything about the page that told you where the seed was, any question
you could not answer with the four offered, and any card the pipeline
got wrong on its own (the audit's job, but a session finds what an
audit does not). Those are the findings a pilot exists for. A UI change
gets its own issue and regression test, as the issue's "done when"
says.
