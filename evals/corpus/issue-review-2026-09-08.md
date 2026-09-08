# Local issue reconciliation — 8 September 2026

Read-only review of all 19 open GitHub issue bodies against `53124ce8`, the v5
measurement and v6 replay, including the latest comments on #612, #610, #625,
#595, #614, #552, #630 and #432. The table records the review of those 19 issues.
Implementation checks and recorded attempts do not establish capability passes.

Subsequently authorised and completed on 8 September: **#612 and #610 closed**,
with [#612’s closure note](https://github.com/dogwonder/kettle/issues/612#issuecomment-5584482563)
and [#610’s closure note](https://github.com/dogwonder/kettle/issues/610#issuecomment-5584482870)
preserving the product and measurement follow-ups. The other 17 dispositions
remain proposals; no other issue was updated. The [subsequent next-work triage](remaining-issues-2026-09-08.md)
rechecks all remaining comments and records #630’s local compatibility preparation.

Remote tips were checked with `git ls-remote`: Kettle main already equals
`53124ce8` (the handover's unpushed note is stale here); examples' remote
`work/reading-capabilities` is `52a843c`, three commits behind local `90e41a6`;
runs' remote main is `72dfe950`, two behind local `f5a36c65`. Kettle and examples
were clean; runs had only its three pre-existing `.DS_Store` files. The initial review changed no sibling
files or remote state; only the two subsequently authorised issue closures
and their comments have been posted.

| Issue | Proposed disposition and concrete evidence / next requirement |
|---|---|
| [#612](https://github.com/dogwonder/kettle/issues/612) | Closed with authorisation for the missing field: schema, unified amount readings, report display and authored amount truth landed (`aac81076`, #627/#628, generator `90e41a6`). `timeline.rs` and `letter_actions.rs` exercise retained/refused readings and separate invoice amounts. Do not describe amount selection as solved: measurement 1's annual-total error is the active follow-up in [product-defects.md](product-defects.md). No new stratum is implied. |
| [#610](https://github.com/dogwonder/kettle/issues/610) | Closed with authorisation for the renderer's unauthored footer: generator `c9f09ce` fixed the original footers; subsequent explicit footer/annex decisions and rendering parity are recorded in [first-batch.md](../capabilities/first-batch.md). `formats-01` now includes an authored footer ask. Retain real PDF/photo model evidence under #256; regenerate only disposable old beds before a future run, never archives. |
| [#625](https://github.com/dogwonder/kettle/issues/625) | Keep open, narrow remaining work to measurement. #627/#628 implement readings, structural deadline verification and scoring 19. Tests exercise row-wise totals, partial money tokens and period/count contradictions. Existing text pod runs do not supply the requested before/after schema token comparison or paired photo-table measurement. Keep those requirements here until explicitly reassigned; do not close on implementation alone. |
| [#595](https://github.com/dogwonder/kettle/issues/595) | Keep open. 112 inventory cases; 53 executable scopes, 59 without checks; 29 recorded attempts, four selected scopes unsupported. Accepted misreads and unparsed forms remain gaps. The existing-bed surface-form census, independent source families and new 4B/9B/27B comparison are not established by the 33-case diagnostic. |
| [#432](https://github.com/dogwonder/kettle/issues/432) | Keep open. Trace ablations/no-model floor and corpus raw/verified v6 views exist (`eval_ablation.rs`, `corpus_execution.rs`). A matched deployment-policy comparison, cascades, burden/Pareto recommendation and the smallest policy clearing all harm boundaries remain unmeasured. Semantic actions are not assessed; time/place/reference are unsupported. Preserve independently authored expectations and blank resource cells for derived rows. |
| [#428](https://github.com/dogwonder/kettle/issues/428) | Keep open. Freeze/reserve/consume/replay lifecycle and routine-selection exclusion are implemented (`70c2a1a8`, `CHALLENGE.md`). Independently authored cases and their first measurement do not exist. Private adjudication, privacy-minimised export and discovery registry remain separate unfinished acceptance items. Metadata declaring independence is not proof of it. |
| [#256](https://github.com/dogwonder/kettle/issues/256) | Keep open. Ordered bindings, physical-page refusal and real PDF/Vision acquisition checks exist; merged distinct-kind attribution is implemented. Acquisition is not model accuracy. Still requires PDF/photo end-to-end model evidence, a supported larger document and any duration/estimate work explicitly reconciled with the sitting-local telemetry policy. Do not relax the shipped three-page limit to meet the large-document criterion. |
| [#539](https://github.com/dogwonder/kettle/issues/539) | Keep open. The first incumbent text diagnostic is a usable starting recording. Freeze a bounded candidate, quant, runtime, load/reasoning-off checks and budget before a matched comparison. The lab/product runtime-pin decision remains unresolved; queue labels are not fresh compatibility evidence. No weights, paid compute or load attempt scheduled by this review. |
| [#614](https://github.com/dogwonder/kettle/issues/614) | Keep open. Prompt work exists; the 7 September v19 baseline still has six inventions in 60 `conditional-done` decisions. A correct corpus negative is not a full development/exam or packaged real-letter result. Retain the promotion condition in `CHECKLIST.md`. |
| [#552](https://github.com/dogwonder/kettle/issues/552) | Keep open for current exam validation. Prompt support for actorless asks and development counterpart exist; development's 23/24 `points-at-a-table` does not clear the historical exam failure. Preserve exam truth and record rationale before any authoring change. |
| [#630](https://github.com/dogwonder/kettle/issues/630) | Keep pending until 21 September. Checklist retains the decision. The subsequent local triage implements the four-item Mini API signature checks in `mini_api.rs` and records the dated pending decision in `app/DECISIONS.md`; both remain uncommitted. Do not extract the host layer or alter kttl.app ahead of the experiment. |
| [#608](https://github.com/dogwonder/kettle/issues/608) | Keep parked. Passing local sidecar tests cannot diagnose the hosted-macOS timeout. A failing-host log/reproduction is still needed; do not claim Clippy establishes runtime coverage. |
| [#233](https://github.com/dogwonder/kettle/issues/233) | Keep open/parked for packaged observation. `privacy-boundary.toml`, static guards and release audit instructions exist. Launch/task/failure/export/deletion/shutdown and explicit-download network observation remain required. |
| [#404](https://github.com/dogwonder/kettle/issues/404) | Keep open/parked. Ordered desktop selection/drop support (`76ec8bb4`) does not prove that the original native silent-drop failure no longer reproduces or fulfil the accessibility criterion. Requires packaged/Tauri reproduction and refusal/accessibility verification. |
| [#431](https://github.com/dogwonder/kettle/issues/431) | Keep parked. No participant study supplied by this sprint; synthetic seeded-error tests are not evidence that people detect errors. |
| [#585](https://github.com/dogwonder/kettle/issues/585) | Keep open for the photographed worked example and compatible paired evidence card. The latest comment and current `app/demo` code confirm #587 already implemented the product copy, goal rendering, route labels and unproven claim presentation. Leave kttl.app unchanged during the Mini experiment. |
| [#451](https://github.com/dogwonder/kettle/issues/451) | Keep parked. No second-model disagreement experiment; agreement must never author truth. |
| [#480](https://github.com/dogwonder/kettle/issues/480) | Retain as the single ideas list. No new pack was scheduled by the corpus sprint. |
| [#55](https://github.com/dogwonder/kettle/issues/55) | Keep parked until external pack authors are scheduled; no trust/distribution claim follows from first-party fixture contracts. |

The weekly scratch-plus-replay merge rule and later pod-before-merge rule in
`CLAUDE.md` still conflict. This local prompt patch neither chooses a policy nor
claims to satisfy either merge bar. Model work is separately proposed in
[product-defects.md](product-defects.md); the review itself authorised no issue closure, posting, push or
model execution. The later user authorisation covered #612 and #610 only.

Comment reconciliation: #612 already records PR #619's v18 amount-field
measurement and archive, supporting closure of that original implementation
scope rather than any current accuracy claim. #625's comment predates the
merged v19 work; its old “when is not on main” prerequisite is stale. #552's
parked-branch/August-plan reference is historical, not the current implementation
status or evidence of an exam pass. #432 remains labelled parked; its August
trace observations do not supply the current matched-policy measurements, and
this review does not silently change its GitHub scheduling state.
