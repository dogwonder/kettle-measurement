Review before ordered-fixture integration, 7 September 2026, at `8cf2babf`.

Scope: the recent ordered-input and capability-coverage commits, and the
fixture, resume and corpus boundaries the next implementation will use.

1. **P2 — model measurement can be claimed with no evidence.**
   `scripts/capability-coverage.py` counts a `model_reading.coverage` label
   of `measured` without a recording or compatibility check. Reproduced
   in memory: change one inventory label and `model_measured` becomes 1.
   Current committed labels are all unmeasured, so current totals do not
   claim a run that never happened. Before slice 2b publishes model
   coverage, derive it from a recording tied to the case, request and
   model identity. A missing or incompatible record must not promote it.
   Tracking: #595/#432, slice 2b prerequisite.

2. **P2 — the corpus's verified-amount score discards currency.**
   `eval/corpus.rs::judge_verified` uses `money_digits` and ignores
   `Value::Money.currency`. Equal digits with the wrong currency can
   score Correct on the verified side; stripping the displayed sign can
   also accept a negative value against positive truth or reject a correctly
   negative value against signed truth. This is code-inspection evidence,
   not a newly measured model failure. Before broader money cases or
   comparisons, add wrong-currency and signed-value scoring cases and
   compare an explicitly interpreted amount/currency, preserving unknown
   or unsupported interpretation. Tracking: #595/#432, slice 2b prerequisite.

3. **P2 — ordered page input has no corresponding fixture binding.**
   `Expected.inputs` accepts one filename per role while the runner and
   desktop accept ordered page groups. The current fixture loader cannot
   express two photographed pages as one letter. This blocks measuring
   the shipped route and is the first implementation task in #256/#595.
   Retain filename compatibility, role order, page order, input identity
   and the product's separate file/page limits.

The current runner combines page parts and preserves physical-page
offsets; its existing tests cover cross-page anchoring and a blank
trailing page. The new limit test uses two text files against a one-page
limit, leaving the shipped three-page boundary and PDFs needing direct
checks. The fixture digest already hashes files in binding order and
resume identity includes the effective manifest; reuse those contracts.

No packaged native-dialog, photograph accuracy or live-model claim is
established by this review. The first two findings must be resolved before
the model comparison stage; they do not require rebuilding page input.

Implementation follow-up: all three unsafe behaviours are addressed in the current working
tree. Ordered fixture bindings preserve legacy filename loading and share
runtime validation. Execution tests cover evidence pages, separate documents,
resume invalidation and three-file/physical-page bounds, including blank PDF
pages and zero model requests on refusal. Root checks and the affected-test
rerun, app tests, formatting, Clippy and Python checks are recorded in
`plan.md` under slice 2a. Slice 2b closed label-only promotion and retained
exact exchanges with separate model/runtime identity. Slice 3 now links
33 exact inventory documents and validates recording-backed coverage by
replaying through the current corpus command. It rejects mocks, missing or
stale evidence and unsupported scopes; no actual model measurement has
been made. Full validation is recorded in `plan.md` under slice 3. Verified
money now compares decimal magnitude, currency and sign, with unknown or
unsupported forms reported explicitly. Counterexample tests cover both fixes.
