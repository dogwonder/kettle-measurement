# Next work across the remaining 17 issues

Rechecked all 17 open issue bodies and their latest comments on 8 September
2026, after the authorised closures of #612 and #610. Remote Kettle main is
still `53124ce8`. The existing amount/list patch remains local and uncommitted.
This is a local scheduling recommendation, not a GitHub label/body update or
authorisation for model runs. It supersedes the earlier review's next-step
recommendations where newer comments or implementation checks sharpen them.

The most valuable next result is whether the current prompt patch fixes the
observed errors. The instrument already supports that comparison; another
general testing framework is not a prerequisite. No remaining issue has the
same straightforward closure case as #612 and #610.

| Issue | Priority and next bounded action | What still prevents closure |
|---|---|---|
| [#630](https://github.com/dogwonder/kettle/issues/630) — runner/Mini decision | **Small local preparation completed in this pass.** `mini_api.rs` pins the four entry points and sidecar lifecycle signatures against Mini bridge `f57cbe9`; `app/DECISIONS.md` records the pending decision. Mini's clean checkout was read only. | Review usefulness and ownership on **21 September**, record the decision and give any resulting build a concrete home. Compatibility tests do not decide architecture. |
| [#595](https://github.com/dogwonder/kettle/issues/595) — capability gaps | **Next measurement priority.** Finish the amount/list patch as one reviewable change, then run the [bounded old/new comparison](product-defects.md) when authorised. Use its outcome to choose the next product fix. | All inventory outcomes/refusals, the bed's surface-form census and a fresh scale comparison remain separate criteria. Time/place/reference and semantic action coverage remain unsupported/unassessed. No broad claim from ten authored regressions. |
| [#256](https://github.com/dogwonder/kettle/issues/256) — PDF/photo/large inputs | **Next format measurement.** Start with the existing two-case `formats-01` bundle across text, PDF and photos: six case executions per model/prompt arm, with actual acquisition coordinates. Define a supported long-document scope separately. | Real end-to-end model answers, larger input and explicit reconciliation of the old duration-estimate requirement with the sitting-local telemetry policy. Three-page refusal is not long-document accuracy. The parked August “text route” comment is stale against the authorised format work. |
| [#625](https://github.com/dogwonder/kettle/issues/625) — unified readings | **Measurement follow-up, not a schema rebuild.** Specify the original before/after schema-cost comparison and include actual photo-table readings in a separately frozen run. | The old/new output-token count and named full-bed/photo measurements. Existing v19 implementation and the new prompt comparison cannot replace the old/new-schema question. Keep open until fulfilled or explicitly reassigned. |
| [#614](https://github.com/dogwonder/kettle/issues/614) — conditional asks | **Product follow-up in the next measured prompt cycle.** Inspect the six current `conditional-done` failures; keep the known negative in the scratch selection and test any change against genuine unconditional asks. | Correct current development/exam results and the packaged real-letter observation required by the retained promotion condition. One correct diagnostic negative does not settle the shape. |
| [#552](https://github.com/dogwonder/kettle/issues/552) — invoice exam misses | **Keep alongside #614 for planned full-pack validation.** The actorless-ask prompt change and development counterpart already exist; verify the current prompt on an explicitly scheduled exam run. | Current exam evidence. Do not rewrite the sealed voice to make development and exam agree or use development's 23/24 as an exam result. The August parked-branch reference is historical. |
| [#428](https://github.com/dogwonder/kettle/issues/428) — independent challenge | **Arrange separate authorship now; measure later.** Use the existing `CHALLENGE.md` hand-off and lifecycle. Obtain new families without exposing answers to prompt iteration. | Actual separately authored cases and their first measurement; private adjudication/export/registry acceptance remains unfinished. Same-assistant variants do not establish independence. No more lifecycle scaffolding is needed for the first challenge. |
| [#404](https://github.com/dogwonder/kettle/issues/404) — native file input | **Worth a bounded Tauri reproduction if this is the app being used.** Choose/drop the same synthetic text and PDF and capture whether native hover/drop events arrive, plus keyboard/accessibility behaviour. Current `core::dropped` still returns `None` for empty paths. | Establish whether the historical failure reproduces. If it does, fix the concrete refusal/control route and validate it through the app; if it does not, use the issue's explicit reproduction-based disposition. Multi-page support does not establish a working native drop. Avoid a control redesign ahead of that check. |
| [#585](https://github.com/dogwonder/kettle/issues/585) — website | **Wait for evidence and the #630 review; most code already exists.** PR #587 implemented the product copy, goal rendering, route labels, empty-state explanation and unproven claims. Code in `app/demo` agrees with the latest comment. Remaining work is a synthetic photographed worked example and the paired evidence card. | A suitable recorded example and current compatible paired evidence. The comment's v17 commands are historical; target the current identity when a run is scheduled. Leave kttl.app unchanged during the Mini experiment. Do not repeat the completed copy work or publish mock output as measured evidence. |
| [#539](https://github.com/dogwonder/kettle/issues/539) — candidates | **After the incumbent patch and format check.** Name one candidate/quant and settle whether a distinct lab runtime pin is permitted; freeze that comparison on the same outcomes. | A bounded compatible run or a documented blocked/parked disposition for each remaining candidate. The roster's release/compatibility assertions need verification at selection time. Do not download weights or launch the queue from triage. |
| [#432](https://github.com/dogwonder/kettle/issues/432) — system comparisons | **After useful matched measurements.** Reuse raw/verified and trace-derived views to ask one concrete policy question raised by an observed failure. | Full requested policy comparison, cascades, review burden and reproducible recommendation. Historic trace results and current scoring infrastructure do not finish it. Keep source truth independent; leave derived resource cells blank. |
| [#233](https://github.com/dogwonder/kettle/issues/233) — packaged privacy | **At the next installer.** Observe a clean packaged lifecycle and explicit download using the existing release procedure. | Dynamic packaged evidence; static inventory and guards already exist. Remains a release obligation, not something model tests can clear. |
| [#608](https://github.com/dogwonder/kettle/issues/608) — hosted macOS sidecar tests | **Keep parked unless macOS runtime CI is scheduled.** First capture the failing host's sidecar log and listener diagnostics. | Reproduction and repair on the hosted runner. Local sandbox listener failures are not proof of the historical host's cause. The old assertion that macOS would add no runtime coverage needs rechecking: actual Vision/PDF acquisition tests now exist. Do not enable broad hosted runs merely to collect a green local result. |
| [#431](https://github.com/dogwonder/kettle/issues/431) — participant study | **Defer recruitment until the presentation being evaluated is settled.** The latest comment records an implemented multi-select study harness and protocol amendments, not a completed study. Reconcile the study presentation with the Mini decision before spending participant effort. | Consented independent participants, frozen protocol/materials and the stated analysis. Author sittings and model scores cannot answer whether people catch mistakes. |
| [#451](https://github.com/dogwonder/kettle/issues/451) — second-model disagreement | **Keep parked.** Reopen when a concrete escaped failure could plausibly be detected by a second reading and the extra cost can be compared. | An actual paired experiment against authored truth; agreement is never correctness. |
| [#55](https://github.com/dogwonder/kettle/issues/55) — external packs | **Keep parked.** Trigger is a scheduled external author/distribution pathway. | Reviewed trust/loading/distribution contract and implementation when that pathway exists. No current need to build a plugin platform. |
| [#480](https://github.com/dogwonder/kettle/issues/480) — ideas list | **Retain as a list, not an active task.** Closing the missing amount field does not automatically schedule another pack. | This is an ongoing register, not an issue to burn down. A selected idea needs a bounded manifest/output and its own scheduled work. |

## Recommended sequence

1. Complete #630's small compatibility preparation — done locally in this pass;
   keep the decision open for 21 September.
2. Review and preserve the current amount/list patch, then authorise the already
   bounded incumbent comparison. Do not add another prompt change before learning
   whether this one helps. The weekly-versus-pod merge-policy conflict needs an
   explicit decision before claiming a merge bar has been met.
3. Measure the existing text/PDF/photo pair under #256, retaining the separate
   photo-table and schema-cost obligations under #625. Combine run logistics only
   where the recorded identities and questions match.
4. Let actual remaining failures choose between #614/#552 product work; obtain
   independent challenge authorship alongside this sequence. Candidate and
   architectural comparisons follow a stable incumbent and outcome contract.

The Tauri reproduction (#404) can be a useful separate local sitting; the
packaged privacy audit (#233) belongs to the next installer. Everything else
has a more specific dependency than “there are open issues”.

Local validation of the new #630 preparation: all four `mini_api` signature
checks passed with all features; workspace formatting and targeted Clippy
passed. No runtime implementation changed and no sidecar or model was launched.
The checks and documentation remain uncommitted alongside the earlier patch.
