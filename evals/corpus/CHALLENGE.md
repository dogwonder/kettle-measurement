# Independent challenge hand-off

Updated 7 September 2026. No independent challenge has been authored, frozen
or measured yet. The diagnostic and format bundles are exposed development
material. The lifecycle script below records authoring declarations and
exposure; it does not certify independence or run a model.

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

`kettle corpus` and the inventory coverage command deliberately refuse a
challenge. A dedicated scheduled challenge execution/review path remains to
be connected before measurement. Do not bypass that boundary by relabelling
an unexposed challenge as a diagnostic. No challenge score is owed to ordinary
CI; its tests use visibly synthetic contract examples only.
