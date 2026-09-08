# Amount/list incumbent comparison — prepared 8 September 2026

**Prepared, not authorised or measured.** [The plan](measurement-02.json)
pins the inputs for the [product patch](product-defects.md). It is a new
measurement specification; measurement 1 and its recordings remain unchanged.
No model process was launched during preparation. The user subsequently
authorised a local commit of this work and requested a new-session handover
for pod preparation. Keep this Metal freeze intact; a pod comparison needs
its own Linux/CUDA identities and explicit execution and spending approval.
That plan now exists as [measurement-02-pod.md](measurement-02-pod.md), with
`scripts/corpus-pod.sh` as its procedure; this file and its JSON are unchanged.

## Question and frozen scope

Does the combined prompt and worked-example change improve amount selection
and list attribution without introducing errors on these exposed documents?
This comparison cannot separate the effect of the prompt from the examples.

| Order | Pack | Selection | Cases | Positive slots | Negative sites |
|---|---|---|---:|---:|---:|
| 1 | Original 0.3.0 from `53124ce8` | unchanged `diagnostic-01` | 33 | 28 | 7 |
| 2 | Original 0.3.0 | `product-regressions-01` | 10 | 12 | 5 |
| 3 | Local 0.3.1 patch | unchanged `diagnostic-01` | 33 | 28 | 7 |
| 4 | Local 0.3.1 patch | `product-regressions-01` | 10 | 12 | 5 |

Run old then new, with no inspection-driven edits or reruns between them.
The same assistant authored the new source truth and patch; the ten cases
are exposed regressions, not separately authored challenges. Both arms use
fresh answers in one sitting, not an archived old arm paired with a new run.

The JSON pins both selections and case order, seven inventory files, each
arm's five runtime pack files, 82 build/runtime source files, the CLI binary,
weights and eleven sidecar binaries/libraries. Only the pack version, prompt
and examples differ between arms. Runtime sources equal `53124ce8`; the
source digest excludes test files. The installed weights and sidecar hashes
match measurement 1. These SHA-256 preparation pins supplement the executor's
recorded BLAKE3 request, pipeline, corpus and runtime identities.

## Budget correction from review

The earlier proposal incorrectly described the executor as allowing at most
one retry per case. `run_segment_step` calls `run_batch`, which can perform
one schema retry per batch attempt, one targeted pairing re-ask per original
batch and recursive truncation splits. The CLI has no total-exchange cap.

The corrected proposal retains the shipped policy, records every exchange
and keeps **43 initial exchanges per arm, 86 overall**, with **1,200 seconds
per arm** across both selections. It does not promise a maximum of 172 total
exchanges. This correction is part of the proposal requiring explicit
authorisation; preparation does not authorise retries or model execution.

Use Qwen3.5-4B Q4_K_M, b10145 Metal, context 8192, temperature 0, reasoning
off, parallel 1 and an answer limit of 4096. Record the actual host at launch
and verify `thinking = 0` in each sidecar log. No download, paid compute,
additional model or automatic rerun belongs to this sitting.

## Execution hand-off after explicit authorisation

1. Recheck the working trees and every JSON pin. A changed executable or
   input requires a new reviewed freeze before observing answers; never
   silently replace these hashes. Preserve the local patch. Create a new
   output root at the path named in the plan; if it exists, stop.
2. Snapshot the five original pack files with `git show <base_commit>:<path>`
   and the five new files from the working tree under that output root as
   `inputs/old/pack` and `inputs/new/pack`, preserving paths relative to the
   pack directory. Copy both corpora and inventory files into `inputs` too.
   Verify the copied bytes against the pins. Copy the plan and record its
   SHA-256, working-tree revision/status and actual command arguments in a
   launch receipt before starting either arm.
3. Use the pinned executable directly. For each row above the command shape
   is below, substituting the snapshot and output paths. The CLI starts a
   sidecar for each selection; both starts count against that arm's budget.
   Do not use `cargo run`, which can rebuild the pinned executable.
4. Supervise the two commands of each arm with **one shared monotonic
   deadline** of 1,200 seconds starting before its first CLI launch. Start
   each CLI in its own process group and terminate that group, including
   the sidecar, at the deadline. Do not give the second selection a fresh
   1,200 seconds. On timeout or execution failure, retain partial files and
   stop the sitting without launching the next arm or extending the limit.
   The command below does not enforce this deadline by itself.
5. Preserve stdout/stderr, exit status, timestamps, all raw request/answer
   files and `target/eval-logs/corpus-<CLI-pid>.log` for each command. Record
   the initial, schema, pairing and truncation exchanges separately. An
   incomplete selection remains incomplete; never replace it with a rerun.

```text
target/debug/kettle corpus
  --corpus <snapshot-corpus.json>
  --inventory-dir <snapshot-inventory-dir>
  --pack-dir <snapshot-arm-pack-dir>
  --model models/qwen3.5-4b-q4_k_m.gguf
  --sidecar-binary sidecars/macos-arm64/llama-server
  --sidecars-dir sidecars
  --context 8192
  --out <new-arm-selection-output>
```

After generation, replay each output using its own frozen pack and corpus,
replacing `--model` with `--replay <original-output>` and choosing a new
output directory. Compare the complete case reports, summary and generation
identities. Require exact request matches and zero legacy matches, including
every retry. Preserve failures even if downstream reporting cannot complete.
Archive only synthetic evidence; writes to `kettle-runs` remain separately
authorised where required. Do not alter its existing entries.

## Reading the result

Report each selection and each arm separately, plus paired per-case changes:
amount selection, explicit negative-site assertions, unmatched authored slots
and candidates, source fields/evidence, deadline reading/evidence, structure
and resolution. Keep acquisition, execution and attribution failures visible.
Inspect retained action text for the original list duplicate, `split-list`,
`active-heading`, `merged-list`, `footer-task` and the original combined-action
case. Record the text and reviewer interpretation; slot counts alone do not
establish omissions or duplication. Do not turn that review into a new scorer.

Correctness comes before sitting-local runtime/memory comparisons. Semantic
action coverage remains `not-assessed`; time/place/reference are unsupported.
There is no tier, baseline, harm-ceiling or full-pack/exam claim. The separate
`formats-01` measurement, #625's old/new schema-cost and photo-table work,
#428's independent authorship and the weekly-versus-pod policy decision remain
outstanding. Keep the Mini decision due 21 September and leave kttl.app alone.

## Local review and validation

Kettle main and remote main still match `53124ce8`. Examples remains three
commits ahead at `90e41a6`, runs two ahead at `f5a36c65` with the same three
untracked `.DS_Store` files, and Mini is clean at local/remote `f57cbe9`.
Mini's instructions, README and bridge were reviewed read-only.

The existing prompt patch, fresh examples, ten source-truth cases, controlled
wrong-answer checks and dated stale-tier changes were reviewed together and
preserved before the subsequently authorised local commit. The stage permits absent current evidence and still
fails when a current passing row makes the exception obsolete. Mini's four
signature checks match its bridge imports and establish source compatibility.
No further product-code or truth change was needed in this review.

Passed this sitting: 13 corpus CLI tests (including actual PDF/photo
acquisition with controlled answers), four Mini API checks, the runner tier
guard, the app's current-pack-verdict test, the worked-example guard, 20 Python
checks, both workspace formatting checks and targeted Mini/tier Clippy.
Both frozen packs also acquired and scored both text selections with
`--no-model`: 86 case executions, zero unscored cases, no model identity.
These are instrument checks; a deterministic floor supplies no reading result.
The handover's full-suite results remain dated evidence, not a new full-suite
run. No archived recording was regenerated or rescored during this review.
