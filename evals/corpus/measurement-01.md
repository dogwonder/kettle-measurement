# First incumbent corpus measurement — 7 September 2026

The 33-case diagnostic now has real model answers and exact replay. It found an annual-total/instalment selection error that survives verification, an extra combined task at a bullet-list heading, and scoring distinctions that must be resolved before a candidate ranking is meaningful. The recorded v5 scores remain unchanged.

## Frozen scope and evidence

The [measurement plan](measurement-01.json) was committed as `62efd6f0` before any answers were observed. It pins all 33 cases of `reading-diagnostic-01`, the existing pack inputs, executable, weights and sidecar bundle. The implementation was `70c2a1a8`; no prompt, source fact or scorer was edited during the run.

One text-only pass used the installed Qwen3.5-4B Q4_K_M weights on the Apple M1 Pro (32 GB, macOS 26.6.2), b10145 Metal, context 8192, temperature 0, reasoning off, parallel 1 and maximum answer length 4096. It completed within the declared 1200-second stop limit. No additional model, format arm, challenge, full bed or automatic rerun was started.

All 33 cases were scorable, with 33 generation exchanges and no execution, acquisition or attribution errors. There were no schema retries. Each generation request records temperature 0, and the sidecar log records `thinking = 0`. All returned obligation candidates had high confidence; the corpus diagnostic list was empty. That empty diagnostic list does not mean all answers were right.

The complete synthetic run, exact replay, frozen plan, launch receipt, sidecar log, derived coverage and file checksums are committed locally in `kettle-runs` as `2185ad34`, entry `2026-09-07-corpus-diagnostic01-qwen35-4b-v5-metal`. Nothing was pushed. The archive manifest describes the source boundary and replay command.

## Recorded results

These are counts under `corpus-fields-v5`, with pack scoring unchanged at 19. Whole-item correctness covers the selected fields; it does not certify the free-form task wording, evidence attachment, absence of extra tasks, or product accuracy.

| Count | Raw | Verified |
|---|---:|---:|
| Whole positive ask slots correct | 9/28 | 25/28 |
| Missing expected slots | 1 | 1 |
| Extra candidates | 1 | 1 |
| Extra candidates at the seven explicit negative sites | 0 | 0 |

There were two misattached evidence fields. Time, place and reference remain unsupported on both sides; they never count as correct.

| Selected field | Raw correct / wrong / missing | Verified correct / wrong / missing |
|---|---:|---:|
| kind | 27 / 0 / 1 | 27 / 0 / 1 |
| party | 27 / 0 / 1 | 27 / 0 / 1 |
| deadline | 9 / 18 / 1 | 26 / 1 / 1 |
| amount | 26 / 1 / 1 | 26 / 1 / 1 |

Exact replay reproduced the entire per-case reports and summary, with 33 exact requests and zero legacy compatibility requests. Model, generation-machine, runtime, sidecar, corpus, pipeline and executable identities also matched. The coverage command independently replayed the recording and established **29 measured scopes out of 112 inventory cases**, with **four selected scopes unsupported**. These are recorded attempts, including wrong answers; they are not 29 capability passes. Inventory source labels remain unchanged because positive evidence is derived from a supplied, validated recording.

## What the recordings show

1. **Wrong financial fact selection survives verification.** In `money-form-013-letter`, the source says annual council tax £3,052.90 and first instalment £305.29 due by 3 April. The model selects £3,052.90, and the verified output retains it with no diagnostic. Both values occur in the same passage: textual containment establishes that a number is present, not that it belongs to this payment.

2. **A list heading produces a duplicate combined task.** In `format-form-002-letter`, the model attaches a task containing both actions to “By 24 March 2026 please:”, then also returns each bullet as its own task. All three survive verification. The one extra candidate is a duplicate and is attributed to the wrong source site; it is not a new instruction invented from nothing. The seven explicit negative sites all remain empty.

3. **Task count is not the same as missing action content.** In `obligation-form-013-letter`, the source requests a returned form and a copy of photo ID. The model returns one task: “Return the signed form and send a copy of your photo ID”. The oracle expects two slots and records one missing. Both actions appear in the answer; this observation does not establish that an instruction was omitted. The scorer currently does not judge that task text semantically.

4. **Strict deadline copying needs a separate interpretation.** All 18 raw deadline `wrong` outcomes have a verified deadline outcome of `correct`, including cases correctly left unresolved. Examples include dropping “on” from an absolute date, omitting a separate anchor sentence, expanding a date to include time, and reading the target date instead of retaining the pointer phrase. These are not eighteen wrong calendar dates. Some concern harmless wording boundaries; others concern the declared evidence/anchor contract. Do not silently treat them all as equivalent or remove them to improve the score.

5. **The ambiguous-period oracle conflicts with the shipped asking policy.** In `relation-form-001-letter`, the dated document says “Payment of the total is due within 14 days.” The corpus declares the deadline ambiguous, with no base, and also authors a zero-count/no-period `deadline_read`. The model returns the printed 14-day period and counts from the letter date, producing 24 March. The prompt explicitly permits `letter_date` for the letter date “stated or not”. This needs a source/pack-policy decision and a separate review of the authored structure; the recorded verified `wrong` outcome alone does not establish a model failure.

6. **Correct field values can hide evidence differences.** `relation-form-010-letter` returns the right target date from its table row rather than the pointer words at the ask site. `format-form-005-letter` attaches “the date above” to the dateline, where those words do not occur; the output leaves the date unresolved. Both get a verified deadline field outcome of `correct`, but the evidence scorer records the attachment problems separately.

## Next action

Reconcile the relative-period source truth and shipped default-anchor policy, then define how combined actions, duplicated actions and strict copied phrases should be reported. Preserve the existing counts and use this archive to replay any downstream scoring change under a new scoring identity. Changes to prompts or requesting structure need fresh model answers on an explicitly scheduled selection.

The annual-total case supplies a concrete fact-selection failure for subsequent work. Evaluate any proposed change across varied totals/instalments and negative cases; recognising this one sentence would not establish the capability. Candidate comparisons should wait until their outcome axes distinguish wrong facts from representation differences.

Separately authored challenge cases, broader format measurements and candidate selection from #539 remain outstanding. A separate author is still needed; no challenge has been constructed or consumed. This exposed text selection cannot establish independent-family generalisation, photograph accuracy, tiers or pack harm ceilings.

## Every case

“Whole” means the current selected-field slot contract. Extra candidates and negative sites are counted separately.

| Case | Positive slots | Whole raw / verified | Extra raw / verified | Negative sites |
|---|---:|---:|---:|---:|
| `date-form-001-letter` | 1 | 0 / 1 | 0 / 0 | 0 |
| `date-form-010-letter` | 1 | 0 / 1 | 0 / 0 | 0 |
| `date-form-014-letter` | 1 | 0 / 1 | 0 / 0 | 0 |
| `date-form-017-letter` | 1 | 0 / 1 | 0 / 0 | 0 |
| `date-form-024-letter` | 1 | 0 / 1 | 0 / 0 | 0 |
| `date-form-028-letter` | 1 | 0 / 1 | 0 / 0 | 0 |
| `date-form-029-letter` | 1 | 0 / 1 | 0 / 0 | 0 |
| `date-form-031-letter` | 1 | 1 / 1 | 0 / 0 | 0 |
| `money-form-001-letter` | 1 | 1 / 1 | 0 / 0 | 0 |
| `money-form-003-letter` | 1 | 1 / 1 | 0 / 0 | 0 |
| `money-form-005-letter` | 1 | 1 / 1 | 0 / 0 | 0 |
| `money-form-013-letter` | 1 | 0 / 0 | 0 / 0 | 0 |
| `money-form-017-letter` | 1 | 1 / 1 | 0 / 0 | 0 |
| `obligation-form-001-letter` | 1 | 0 / 1 | 0 / 0 | 0 |
| `obligation-form-004-letter` | 0 | 0 / 0 | 0 / 0 | 1 |
| `obligation-form-006-letter` | 0 | 0 / 0 | 0 / 0 | 1 |
| `obligation-form-007-letter` | 0 | 0 / 0 | 0 / 0 | 1 |
| `obligation-form-009-letter` | 0 | 0 / 0 | 0 / 0 | 1 |
| `obligation-form-010-letter` | 0 | 0 / 0 | 0 / 0 | 1 |
| `obligation-form-012-letter` | 0 | 0 / 0 | 0 / 0 | 1 |
| `obligation-form-013-letter` | 2 | 0 / 1 | 0 / 0 | 0 |
| `party-form-006-letter` | 1 | 0 / 1 | 0 / 0 | 0 |
| `party-form-007-letter` | 1 | 0 / 1 | 0 / 0 | 0 |
| `party-form-009-letter` | 1 | 1 / 1 | 0 / 0 | 0 |
| `party-form-011-letter` | 0 | 0 / 0 | 0 / 0 | 1 |
| `relation-form-001-letter` | 1 | 1 / 0 | 0 / 0 | 0 |
| `relation-form-002-letter` | 1 | 1 / 1 | 0 / 0 | 0 |
| `relation-form-008-letter` | 1 | 1 / 1 | 0 / 0 | 0 |
| `relation-form-010-letter` | 1 | 0 / 1 | 0 / 0 | 0 |
| `format-form-002-letter` | 2 | 0 / 2 | 1 / 1 | 0 |
| `format-form-005-letter` | 1 | 0 / 1 | 0 / 0 | 0 |
| `time-form-001-letter` | 1 | 0 / 1 | 0 / 0 | 0 |
| `time-form-011-letter` | 1 | 0 / 1 | 0 / 0 | 0 |
