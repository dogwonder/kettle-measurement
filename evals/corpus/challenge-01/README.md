# External invitation wording challenge (#428)

Three public UK Health Security Agency invitation templates supply the wording
for this first external challenge. They were written outside Kettle and use
none of its development/exam generator templates. The originals are pinned
under `sources/`; source URLs and SHA-256 values are in
`fixtures/generation.json`. The publication index is
[Childhood vaccination invitation letter templates](https://www.gov.uk/government/publications/childhood-vaccination-invitation-letter-templates),
updated 27 January 2026.

UKHSA Crown copyright material is reused under the
[Open Government Licence v3.0](https://www.nationalarchives.gov.uk/doc/open-government-licence/version/3/).
No NHS or other logo is reproduced in the generated inputs, and no endorsement
is implied. The original public templates contain placeholders, not a person's
clinical record. The generated names, practice, telephone and dateline are
fictitious. Source vaccination prose is document-reading test material, not
medical guidance supplied by Kettle.

`generate.py` extracts the specified original paragraphs, fills placeholders,
omits instructions to the template editor and normalises the blank appointment
reminder. Original wording defects remain. It uses a new plain ReportLab
layout, independent of `kettle-examples`, and PDFium rasterisation for the image
arm. That image is a clean scan, **not** a camera photograph. Original DOCX
layout is not preserved; this measures an external wording family in three
input formats, not fidelity to UKHSA's Word layout.

Truth was annotated before seeing model answers: the current ask is to phone
to book. Conditional advice depends on preferences or past appointments the
letter cannot settle. A blank reminder is not an appointment already made.
No payment amount or definite appointment date is authored. Every remaining
passage has an explicit negative site, with the source and reasoning retained.

The letter author is external; the adapting developer still chooses the cases,
annotates truth and renders the inputs. This is **not independently adjudicated**
and its three related templates are one family, not three independent samples
or proof of generalisation. No prompts, schema, scorer or thresholds changed
in response to this challenge.

The selection is frozen in `lifecycle.json`, outside routine fixture discovery.
Ordinary diagnostics refuse it. Its explicit first execution consumes the
selection before model startup; retain a failed attempt too, and never reset
the ledger. See [the challenge workflow](../CHALLENGE.md) and the pinned
`../measurement-04-challenge.json` plan. Regenerate assets only into a new
directory; regeneration does not create a fresh holdout of the same wording.
