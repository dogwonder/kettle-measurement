# Independent challenge hand-off

Updated 8 September 2026. [The first external-wording challenge](challenge-01/README.md)
uses three UKHSA invitation templates, with authoring, annotation and rendering
relationships stated separately. Its plan is `measurement-04-challenge.json`;
the authoritative exposure state is `challenge-01/lifecycle.json`. The ordinary
diagnostic and format bundles remain exposed development material. The
lifecycle script records declarations and exposure; it does not certify
independence. [Private field adjudication and exports](FIELD-EVIDENCE.md) now
have a separate local workflow.

## Source inventory

These publication indexes were checked as candidates for separate wording
and layout families. Their example documents have not been imported or
adjudicated into challenge truth. A separate author should choose whole
families, record the specific document/version, invent all recipient details
and values, and author facts and asks without consulting Kettle's answers.
Review source reuse terms before copying any wording or artwork.

| Candidate family | Source and locator | Reason to consider it | Scope constraint |
|---|---|---|---|
| Benefit invitation and follow-up | [DWP example invitation letters for claiming PIP](https://www.gov.uk/government/publications/example-invitation-letters-for-claiming-pip), updated 22 December 2023; PIP.0201, .0185, .0202, .0204 | Several letter variants, choices, deadlines and consequences described by an outside author | The published examples are four or five pages. They exceed the shipped three-page letter limit; test refusal or use a separately declared longer-document scope. |
| Vaccination invitation | [UKHSA childhood vaccination invitation templates](https://www.gov.uk/government/publications/childhood-vaccination-invitation-letter-templates), updated 27 January 2026; GOV-20132 and gateway 2025522 | Different addressee/actor relationship and invitations for different age groups | Document-reading material only. Do not turn medical background into an ask or infer a scheduled appointment from an invitation to arrange one. |
| Complaint response | [PHSO central-government sample letters](https://www.ombudsman.org.uk/organisations-we-investigate/complaint-standards/uk-central-government-complaint-standards/ukcg-good-complaint-handling-toolkit/sample-letters) | Responses authored around different complaint-handling decisions | Select and adjudicate a specific template before claiming any capability coverage. A template collection is not a scored case. |

A public citation establishes where a family came from. Independence also
needs a separate authoring relationship and separation from development
feedback. If a family/result informs prompt, schema, scorer or architecture
changes, record exposure and use it as regression material thereafter.

## Freeze and record exposure

The separate author supplies the ordinary corpus facts/spans/asks shape with
`selection.purpose: "challenge"`, `selection.exposure: "unexposed"`, a fresh
selection id and the supported scoring fields. Its provenance includes:

```json
{
  "independent": true,
  "authoring": {
    "author": "Name or accountable author identifier",
    "relationship": "separate-author",
    "source_families": [
      {
        "id": "fresh-family-id",
        "locator": "Publisher, exact document, version and pages",
        "relationship": "external-source"
      }
    ]
  }
}
```

These are declarations requiring review, not values to add to a development
example to make it independent. The script refuses known development case
ids/documents and generator-family declarations; it cannot detect paraphrases,
prior human exposure or a false declaration. Keep the actual challenge outside
routine fixture discovery. Do not place real personal records in a corpus.

```sh
python3 scripts/challenge-selection.py freeze \
  --corpus /path/to/separate-challenge.json --record /path/to/lifecycle.json
python3 scripts/challenge-selection.py check \
  --corpus /path/to/separate-challenge.json --record /path/to/lifecycle.json
python3 scripts/challenge-selection.py expose --record /path/to/lifecycle.json \
  --reason 'Result informed the deadline prompt' --evidence 'recording/decision locator'
```

Freezing creates a new record and pins the complete corpus bytes, including
truth and selection. Checking refuses changes; after exposure it returns
`regression` and exit 2. Exposure appends a dated reason/evidence event; there
is no reset operation. Keep lifecycle records together: freezing refuses a
reused digest or selection id already in that ledger, even under a new filename. Retain the original record with any recording and
create a fresh selection before another held-out claim. The record contains
metadata and a digest, not the challenge's documents or answers.

## Explicit execution and replay

Ordinary `kettle corpus` calls and inventory coverage still refuse challenges.
The explicit `--challenge-record` path accepts the original frozen corpus,
validates its facts/spans/asks and authoring declaration, and consumes the
lifecycle before starting a sidecar or sending a request. For a separately
scheduled measurement, use:

```sh
target/debug/kettle corpus --corpus /path/to/separate-challenge.json \
  --challenge-record /path/to/lifecycle.json \
  --model /path/to/approved-model.gguf --out /path/to/new-challenge-run
```

Choose the model, runtime, question and run budget before scheduling this.
The first supplied external challenge and its execution identity are linked above.
The existing ordered `--bindings` contract is also available; acquisition
mismatches and ambiguous attribution remain explicitly unscored.

Execution takes a conservative one-attempt policy: reservation itself appends
the exposure event, recording the intended output directory and answer source.
Failed model startup and later failures also consume the selection; the output
directory may not exist if startup failed. A mock or `--no-model` attempt consumes
it too and never establishes model evidence. Use exposed synthetic contract
fixtures when testing the workflow. A second fresh attempt is refused.

The report retains the unchanged challenge selection and the consumed lifecycle
as `challenge`; `challenge-lifecycle.json` is saved beside the source snapshot
and exchanges. The source's frozen `exposure: unexposed` describes its authoring
state, while the lifecycle records that the attempt has consumed it. Existing
model, weights, runtime, machine, pipeline and executable identities accompany
the report. These results do not promote inventory coverage, tiers or ceilings.

```sh
target/debug/kettle corpus --corpus /path/to/new-challenge-run/corpus.json \
  --challenge-record /path/to/new-challenge-run/challenge-lifecycle.json \
  --replay /path/to/new-challenge-run --out /path/to/new-challenge-replay
```

Replay requires exact recorded requests and the original consumed lifecycle
and corpus identity. It retains that lifecycle and reports `answer_source:
replay`; it neither launches a model nor becomes a fresh challenge. Modified
truth, changed lifecycle metadata and legacy prompt-only recordings are refused.

Freezing serialises creation across the ledger with `.challenge-freeze.pending`.
Execution and the Python exposure command share an exclusive `.pending` file,
so competing launches cannot both reserve the same lifecycle. An interrupted
write blocks subsequent execution/checking; investigate the ledger and any
recordings before recovery rather than resetting a selection to unexposed.
Keep one authoritative ledger: these local records cannot prevent someone
restoring an old copy or making false authoring declarations.

No challenge score is owed to ordinary CI. The tests exercise consumption,
concurrent attempts, failed startup, metadata changes and exact replay using
visibly synthetic contract examples only. Once consumed, retain the selection
as regression evidence and obtain fresh separately authored cases before
another held-out claim.
