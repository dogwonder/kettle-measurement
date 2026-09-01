# The study's answers (#431)

One file per participant, published alongside the findings they support.
Empty until somebody is recruited — but an `a`-numbered file (`a01.json`)
is the study's author sitting the instrument themself, marked
`author-pilot` inside, and never counted among the twenty
(`app/study/PILOT.md`).

They live here, inside `fixtures/`, because `fixtures/` is already
declared in `assurance/claims.json`'s `published` boundary — so these
travel through the projection that publishes the rest of the measurement
layer, and there is no second repository, second boundary or second
README to come apart from this one. `kettle project` materialises from
`git ls-files`, so a transcript is public the moment it is committed.
That is the intended behaviour and it is worth saying out loud, because
it means **the check happens before the commit and there is no second
chance at it.**

## Why they are published at all

The study's result is a rate: how often a person catches a seeded error
of each class. A rate is a claim, and this project's rule is that a
claim can be re-asked by somebody who was not in the room. Publishing
the answers means a finding can be re-read rather than re-gathered —
which matters more here than anywhere else in the repo, because
re-gathering costs twenty people half an hour each.

The corpus they were given is already here: `../report-*.json`,
`../statement-*.csv` and `../audit.json`. The instrument that drew a
session and scored an answer is published too (`app/src/lib/study-*.ts`
and `app/study/`), so a transcript can be re-scored rather than taken on
trust.

## Why they can be published safely

Because nothing connects a participant number to a person. Not "held
securely" — never written down, which is the arrangement the consent
text describes and the reason it also says a file cannot be withdrawn
once handed over. A file here holds a number, ten answers, two
confidence ratings each, some timings, the claim they pointed at where
they rejected one, and whatever they typed into the one free-text box.

## What is checked, and what cannot be

`app/src/lib/study-transcripts.ts` is a **floor**, in the same sense
`scripts/check-boundary.sh` is one in the recordings archive: it reads
shape and never meaning. It refuses a file that holds any key the
consent form does not list, a participant number that is not of the form
`p01`, a consent stamp whose digest does not match the words in force
under that version, an answer the harness could not have produced, and a
free-text answer past 300 characters.

That last one is the only rule designed to *stop* the machine deciding.
A box asking what a claim should have said instead does not get three
hundred characters by accident, and the one that does deserves a
person's eye. ("Which one?" is no longer a box at all — it is a list of
the report's own claims, because a prose answer to it could never be
scored; see `app/study/README.md`.)

**What the floor cannot check is the thing that matters.** A sentence
naming somebody's employer is a perfectly well-formed string. So the
human read is a recorded step rather than a good intention: every file
here must have an entry in `READ.json` naming who read its free-text
answers and when, and `study-transcripts.test.ts` refuses a transcript
nobody has signed for — and a signature for a file that is not here.

Two hundred free-text answers across twenty files is a short job. It is
the last moment at which anything can be changed.

## A retired number is never issued again

`RETIRED.json` names participant numbers that were sat and whose
transcript is not published, with the reason and the commit the file is
in. `scripts/study-pilot.sh next` reads it and skips them.

It exists because `a01` was sat and then superseded within the hour, by
a change to the draw it had itself provoked. Counting the files on disk
would have offered `a01` again, and two different sessions under one
number is a published record that cannot be read.

## Adding a session's file

1. Take the file the participant handed over. Do not edit it.
2. Read the free-text answer in all ten responses.
3. Add it here as `p07.json`, and add its entry to `READ.json`.
4. `cd app && bun run test` — the floor and the manifest are checked
   there, so a fault is a red test rather than a discovery later.
