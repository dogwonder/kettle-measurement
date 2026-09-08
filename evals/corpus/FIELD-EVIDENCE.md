# Private field evidence (#428)

Use a real document through the packaged app and retain its ordinary run
directory. This workflow adds local adjudication to that evidence; it does
not run a second model, infer correctness from approval or publish a document.
The observation and discovery registry are `*.private.json`, ignored by git
and created with owner-only permissions. Keep them outside the run directory
so that annotating an observation does not change its evidence.

## Record an observation

Write a local `provenance.private.json` with four strings:

```json
{
  "model": "the installed model and quant used for this run",
  "pack": "the pack id and version used for this run",
  "runtime": "the actual sidecar build/backend, or explicitly unknown",
  "basis": "where these identities came from: retained run receipt or installed build"
}
```

The desktop's existing receipt does not contain a complete runtime digest.
Keep that limitation explicit: this declaration supplements the retained
report/manifest and does not certify a runtime pin. Do not insert today's
model or sidecar identity into an old observation.

```sh
python3 scripts/field-evidence.py init \
  --run-dir /path/to/completed/app/run \
  --provenance /path/to/provenance.private.json \
  --format pdf --structure table --coverage amount-selection \
  --out evals/local/field/observation.private.json
```

The draft records hashes of every file in the run, its original report
provenance/manifest, and an unreviewed entry for each obligation/finding.
Exports refuse a changed run. Retain this run with the observation; deleting
the source evidence makes the observation unavailable for export.
When deleting this field observation, remove its private draft and any private
registry snapshots as well as the app run; the app cannot delete separately
created developer notes for you.

Read the original document and the actual report. Edit the private draft:

- `correct`: the important assertion is supported and complete.
- `wrong`: an asserted value or action contradicts the source.
- `incomplete`: an important part of an assertion or an entire claim is missing.
- `unsupported`: the system asserted something without supporting evidence.
- `uncheckable`: the available source does not permit a judgement.
- `null`: nobody has adjudicated it. Approval, silence and accepting an action
  never replace this with `correct`.

Use `note` for private reasoning, including source quotes if useful. To record
a missing claim, append a distinct id with `target: "missing"`, outcome
`incomplete` (or `uncheckable` if it cannot be established), a note, and null
`reproducer`/`issue` links. A missing claim cannot be a correct assertion.
The draft is developer adjudication, not a blinded independent user study.

## Track failures and export counts

Every non-correct adjudication is a discovery. Its coverage dimension is the
observation's closed `shape.coverage` category. Create a wholly synthetic
reproducer with different people, values and wording; set its local locator
in `reproducer`, and the tracking issue locator in `issue`. Never paste the
private observation or its note into that issue. A locator is a developer's
declaration, not an automated proof that the reproducer covers the failure.

```sh
python3 scripts/field-evidence.py registry \
  --record evals/local/field/observation.private.json \
  --out evals/local/field/registry.private.json

python3 scripts/field-evidence.py export \
  --record evals/local/field/observation.private.json \
  > /tmp/field-summary.json
```

Repeat `--record` to aggregate distinct runs. The registry is private and
lists each discovery, its observation, coverage, links and missing follow-ups.
Use a new output filename when refreshing it; existing records are not
overwritten. The public export is a newly constructed object constrained by
[field-summary.schema.json](field-summary.schema.json): outcome counts,
unreviewed and missing-claim counts, counts of discoveries without reproducers
or issues, and fixed format/structure/coverage categories. It contains no
filenames, paths, source hashes, dates, values, prompts, answers, notes,
model/runtime identifiers or per-observation linkage. Invalid private input
also cannot echo its content in an export error.

These are privacy-minimised aggregate observations, not an anonymity guarantee
or a population accuracy estimate. No baseline, tier or public claim is updated
by these commands. Actual private documents, adjudications and discoveries stay
local; only a deliberately reviewed aggregate may be shared.

## Repeat after the next real document

Run it normally, create a fresh private draft, adjudicate important claims and
missing claims, then inspect the registry's missing follow-ups. Existing
challenge lifecycle rules in [CHALLENGE.md](CHALLENGE.md) remain separate:
once challenge results have influenced development, retain them as regression
evidence and obtain fresh external material before another held-out claim.
