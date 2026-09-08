# Amount/list comparison on a rented RTX 4090 — run 8 September 2026

**Final status, after all four stages below:** 0.3.1 and 0.3.2 are withdrawn;
`main` carries 0.3.0 again (`3f47790f`). The full bed supersedes the first
two dispositions recommending further prompt sentences. The two product
defects remain open; any next attempt changes one thing and is measured on
the full pod bed before consulting the 43 exposed corpus cases. The quote
pairing defect was fixed separately in `875ce950`.

The five corpus arms and three bed passes are archived in
`dogwonder/kettle-runs` at `d8d94223`: [corpus recordings](https://github.com/dogwonder/kettle-runs/tree/d8d94223/2026-09-08-corpus-amount-list-old-new-032-4b-9b-27b-pod4090)
and [bed recordings](https://github.com/dogwonder/kettle-runs/tree/d8d94223/2026-09-08-letter-qwen3.5-4b-v19-pack031-032-development-exam-pod4090).
The observations below retain the order in which they were made.

**Run and read; nothing promoted.** Both arms of the
[pod plan](measurement-02-pod.md) ran freshly on RunPod pod
`dhqfsu8me9r380` (RTX 4090, driver 580.126.20, CUDA 12.8 toolkit, Ubuntu
24.04.3) under the [frozen identities](measurement-02-pod.frozen.json):
llama-server 10145 (ad256ded) with `libggml-cuda.so`, the CLI at
`sha256:5f61bf91…` built with `--features pdf` by rustc 1.97.1, weights
`13c16f42…`. Every arm's pins were re-verified before launch. Each sidecar
log records `thinking = 0` and `using device CUDA0 (NVIDIA GeForce RTX
4090)`; the card held 3.7 GB at 84–96% during both arms. Both arms ran
under one fixed environment, so their runtime identities are equal.

| Arm | `diagnostic-01` | `product-regressions-01` | Exchanges | Arm wall clock |
|---|---|---|---|---|
| old 0.3.0 | 33 cases, 62 s | 10 cases, 25 s | 44 = 43 initial + 1 pairing re-ask | 87 s of 1,200 |
| new 0.3.1 | 33 cases, 62 s | 10 cases, 25 s | 44 = 43 initial + 1 pairing re-ask | 87 s of 1,200 |

No schema retry, no truncation split, no timeout. Exact replay of each
selection with its own pack matched every per-case score, summary and
identity: 33 and 11 exact requests per arm, zero legacy matches. The
recordings are under `evals/runs/amount-list-old-new-qwen35-4b-cuda-02/`
(gitignored) with receipts, sidecar logs, `nvidia-smi` readings,
`exchanges.json` and `replay-check.json`; the tarball digest is
`25dafaad…`. These were subsequently archived with the later arms above.

## The question: did the patch fix amount selection and list attribution?

**Partly, and it introduced one wrong date.**

| Distinction | Case | Old 0.3.0 | New 0.3.1 |
|---|---|---|---|
| Instalment beside annual total, prose | `money-form-013` | two tasks: annual £3,052.90 (wrong) and instalment £305.29 | **one task, £305.29** — fixed |
| Instalment after / before the annual figure | `instalment-after`, `instalment-before` | correct | correct (not discriminating) |
| Instalment in a table row, pointer in prose | `instalment-table` | €1,200.00 (wrong) at the prose ask | €1,200.00 (wrong) **and** a second "Pay the instalment €200.00" task on the table row, a negative site — worse |
| Annual total genuinely asked in full | `annual-total-due` | £960.00 chosen, then **discarded by the runner** | same |
| Absent instalment amount | `instalment-not-printed` | kept, amount empty | same |
| Unsettled already-paid condition | `already-paid-no-task` | "Send the receipt" invented at the negative site | same (raw also carries £55.00; verified strips it) |
| Two bullets, deadline on the introduction | `split-list` | three tasks: the introduction's combined task plus both bullets | **two tasks, the bullets only** — fixed |
| Heading duplicate from measurement 1 | `format-form-002` | combined task at the heading plus both bullets | same, not fixed |
| Two actions in one sentence | `obligation-form-013` | one task carrying both actions (one authored slot missed) | same |
| Heading that itself asks, merged paragraph, footer | `active-heading`, `merged-list`, `footer-task` | correct | correct (not discriminating) |

Summary counts, old → new. `diagnostic-01` (28 slots, 7 negative sites):
raw amount wrong 1 → 0; unmatched candidates 2 → 1; whole-item raw 10 → 9;
deadline reading correct 11 → 10, structure wrong 2 → 3, resolution wrong
**0 → 1**; verified deadline correct 26 → 25. `product-regressions-01`
(12 slots, 5 negative sites): deadline reading correct 9 → 11; whole-item
raw 9 → 10; raw amount wrong 1 → 1; unmatched candidates 3 → 3 raw and
2 → 2 verified (the combined introduction task left, a table-row task
arrived); missed 1 → 1.

## Two findings that matter more than the counts

**A curly apostrophe loses a correct payment task to review, on both
prompts.** In `annual-total-due` the model chose £960.00 in both arms,
echoing the passage with a straight `'` where the source prints `’`. The
executor's exact echo check called that a confabulated pairing, re-asked
once, received the same straight apostrophe, and sent the decision to
needs-review; the report records it as unpaired and the slot as missed.
Real letters print curly apostrophes routinely. This is an instrument
finding, independent of the prompt, and it is the reason the case reads
"missing" rather than "correct" in both columns. Whether the echo
comparison may normalise apostrophes (and nothing else) is a decision for
`exec.rs`, not for this record.

**The new prompt dropped "within" once and moved a deadline by two
weeks.** In `relation-form-002` ("you must reply within 14 days of the
hearing on 1 June 2026") the old arm read "within 14 days of the hearing
on 1 June 2026", structure 14 days from the named day, due 15 June. The
new arm read "14 days of the hearing on 1 June 2026", structure `none`
counting from the named date, and showed **1 June**. That is a wrong date
a person would act on, produced by the patch. Three further raw-side
copy changes in the new arm did not move a date: `date-form-029` and
`relation-form-008` shortened "within N days of …" to "within N days",
and `party-form-009` copied the whole appointment sentence (time and
place) as the deadline. Two moved the other way: `obligation-form-013`
and `format-form-002` now copy "by 24 March 2026" as the reading contract
expects.

## What this is not

A CUDA reading. The old arm on CUDA reads `diagnostic-01` as 11/16/1 on
deadline reading and 10 whole items raw where the Metal recording of the
same pack read 10/17/1 and 9, so the runtimes differ on at least one
decision (#596); nothing here compares with or replaces the Metal
measurement. No tier, baseline, harm ceiling, full-pack, exam,
photograph or semantic-action claim. The ten regression cases were
authored by the same assistant as the patch. Semantic action coverage
stays `not-assessed`; the table above is a reviewer's reading of the
retained action text, not a scorer. Runtime figures are the pod's for
this sitting only.

## Disposition after the first sitting — superseded by the full bed below

The patch is not ready to merge as it stands: it fixes the prose
instalment selection and the split-list duplicate, leaves the table-row
selection and the heading duplicate, and costs one wrong date. The next
prompt edit should keep the selection and attribution paragraphs and
repair the copied-deadline instruction so that "within" survives, then be
measured again on the same two selections, on the same pod class, before
any full-bed run. The apostrophe echo failure has its own home.

## Second sitting, same pod: a 0.3.2 candidate and the 9B

Run the same evening on the same box after a second freeze
([`measurement-02-pod.frozen.json`](measurement-02-pod.frozen.json) now
carries four arms; executable, sidecar and 4B weights unchanged). Arm
`cand` is pack 0.3.2: the 0.3.1 prompt plus one sentence after the
deadline-copy instruction, *copy the whole phrase from its first word*,
with a fresh example no bed or corpus prints. Arm `9b` is the 0.3.1 pack
read by Qwen3.5-9B Q4_K_M (`d784ce9e…`, the digest Hugging Face serves).
Both arms: 44 exchanges (43 initial, the same pairing re-ask), exact
replay, no timeout; `cand` 85 s, `9b` 118 s. Tarball `86615727…`.

| | old 0.3.0 | new 0.3.1 | cand 0.3.2 | 9B on 0.3.1 |
|---|---|---|---|---|
| `diagnostic-01` verified whole items (of 28) | 25 | 25 | 24 | 18 |
| `diagnostic-01` verified deadline correct / wrong / missing | 26 / 1 / 1 | 25 / 2 / 1 | 24 / 2 / 2 | 18 / 1 / 9 |
| `diagnostic-01` unmatched candidates (verified) | 2 | 1 | **0** | **0** |
| `diagnostic-01` missed slots | 1 | 1 | 1 | **4** |
| `product-regressions-01` verified whole items (of 12) | 10 | 10 | 10 | **11** |
| `product-regressions-01` unmatched candidates (verified) | 2 | 2 | 2 | **0** |
| `product-regressions-01` verified amount wrong | 1 | 1 | 1 | **0** |

**The candidate sentence repairs what it was written for and moves two
other dates.** `relation-form-002` is back to "within 14 days of the
hearing on 1 June 2026", due 15 June; `date-form-029` copies "within 28
days of receipt" whole again; and the `format-form-002` heading
duplicate, untouched by 0.3.1, is gone under 0.3.2 (the two bullets
only). Against that: `format-form-005`, "Pay by the date above", which
0.3.0 and 0.3.1 left with no date, now copies the letter's own dateline
"10 March 2026" and shows it as the due date, which is wrong; and
`date-form-024`, "within 2 weeks", is read as 14 days, the verifier
refuses the converted count against the phrase, and the date that 0.3.1
showed (24 March) is lost. One wrong date and one lost date for one
wrong date fixed and one duplicate removed. The amount side is identical
to 0.3.1: `instalment-table` still selects €1,200.00 and still adds the
table-row task.

**The 9B invents nothing and denies three payments.** Across both
selections it produces zero unmatched candidates: the table-row
instalment is read as €200.00 at the prose ask, the already-paid
condition creates no task, the heading duplicate is absent. But on
`money-form-001`, `-003` and `-005`, three plain one-line payment asks
("Pay the total due", "Pay the overdue balance", "Pay the amount due"),
it answers *no obligations* at high confidence for every passage, so the
verified column shows no task at all. Nine of its deadlines resolve to
nothing because it names the ask's own passage as where the dateline is
printed (`from.at` = 2 where "10 March 2026" is in passage 0), and the
reading verifier correctly refuses a base that is not verbatim in the
passage named. The shape is the one already on record for this family:
scale buys restraint and pays in omission. It is one diagnostic on 43
exposed cases and names no tier.

**The apostrophe echo failure holds on all four arms**, 9B included:
£960.00 is chosen every time and discarded to review every time.

## Disposition after the second sitting — superseded by the full bed below

None of the three prompts is merge-ready. 0.3.2 is the best of the 4B
arms on list attribution (no duplicates on either selection) and on the
"within" regression, and it should be kept, but the copied-deadline
sentence needs to say that a pointer ("the date above") is copied as
printed and never replaced by a date from elsewhere, and that "2 weeks"
is 2 weeks. The table-row instalment (`instalment-table`) has not moved
under any 4B prompt and is where the next prompt sentence should go. The
apostrophe echo comparison is an `exec.rs` change with its own test. The
9B result is a candidate note for #539, not a recommendation.

## Third sitting: the full bed on 0.3.2, and the 27B

**Full letter bed, 0.3.2, development and exam, same pod.** Run by hand
with the eval flags `pod-eval.sh` would use, under the fixed
environment, and compared against the 7 September RTX 4090 baseline
(`evals/baseline-v19-letter.json`, same backend, same card). Recordings:
`evals/runs/pod-2026-09-08-letter-032/` (gitignored; tarball
`a33424f5…`, 539 development and 515 exam run directories, both logs and
both pod baselines).

| | 7 Sep, 0.3.0, development | 0.3.2 development | 0.3.2 exam |
|---|---|---|---|
| pooled obligations | 0.84 (n=757) | **0.71** (n=757) | **0.68** (n=733) |
| any-letter obligation, confident-wrong over decisions | 0.02 (n=251) | **0.26** (n=251) | **0.18** (n=263) |
| any-letter no_obligation, over decisions | 0.00 (n=100) | 0.00 | 0.01 (n=100) |
| undated-relative recall | 1.00 (n=31) | **0.19** | **0.00** |
| dated-anchor / payment-anchored recall | 0.92 (n=36) | **0.28** | 0.86 |
| appointment-preparation recall | 0.37 (n=180) | **0.14** | **0.06** |
| repeated-ask recall | 1.00 (n=64) | **0.61** | 0.84 |
| three-asks recall | 1.00 (n=126) | 1.00 | — |
| verdict | FAIL (five decisions from the ceiling) | **FAIL** | **FAIL** |

**Both FAIL, and far below the prompt they were meant to improve.** The
196 development decisions that differ from the baseline, read one by
one from the log's `current` lines:

- 44 deadlines returned empty and 12 kinds changed from `other` to
  `response`, almost all in appointment-preparation letters ("for 29
  March 2026 at 3.50pm" → "").
- 43 decisions with identical printed text that score differently: the
  base for the period, `from`, now comes back as the ask's own passage
  with an empty value, so the date is not resolved (25 undated-relative,
  16 invoice-totals).
- 34 asks missed outright, 25 of them the second printing of a
  repeated ask — the 0.3.1 paragraph *record each distinct action there
  once* read as "once per letter".
- 24 payment periods replaced by a date: "within 30 days" → "10 March
  2026", the letter's dateline — the 0.3.2 sentence misfiring, the shape
  `format-form-005` showed once on the 43 cases.
- 17 inventions (8 conditional-advisory, 7 appointment-preparation) and
  3 "for" dropped from an appointment time.

The 43-case selection saw one instance of each of the last two shapes
and none of the first three. That is the sentence in `CLAUDE.md` about
the scratch loop made concrete: a prompt edit that reads well on the
cases it was written against can take a bed from 0.84 to 0.71 without
the selection noticing. Which of these are 0.3.1's paragraphs and which
are 0.3.2's sentence is what the 0.3.1 development pass below answers.

**Qwen3.5-27B Q4_K_M on the 0.3.1 pack, 43 cases** (arm `27b`; weights
`81657841…` matching the Hugging Face digest, on the network volume;
43 exchanges, no re-ask, exact replay; 351 s against the 4B's 87 s).
On `product-regressions-01` it is the only arm to read every slot: 12 of
12 whole items, zero inventions, zero misses — the table-row instalment
€200.00 at the prose ask, no already-paid task, and the £960.00 annual
charge kept, because it echoed the curly apostrophe as printed. On
`diagnostic-01`: 24 of 28 whole items, three misses (two of the plain
"pay the total due" asks the 9B also denied, plus "within 14 working
days"), two candidates the truth calls inventions (a "quote your
reference" ask on two letters, which a reader might well call an ask),
and the two-actions-in-one-sentence case split correctly into two
tasks. Denial of a bare payment ask is shared by the 9B and the 27B and
absent in the 4B; restraint on the negative sites is shared by both
larger models. One diagnostic, 43 exposed cases, no tier.

## The 0.3.1 development bed: the damage is 0.3.1's, not 0.3.2's

Same fixtures, same baseline, same fixed environment, the committed
0.3.1 pack from `0da1fa4e` (recordings in
`evals/runs/pod-2026-09-08-letter-031/`, tarball `bda15813…`, 539 run
directories).

| development set | 0.3.0 (7 Sep) | 0.3.1 | 0.3.2 |
|---|---|---|---|
| pooled obligations | 0.84 | 0.73 | 0.71 |
| end-to-end | 0.91 | 0.82 | 0.78 |
| any-letter obligation, confident-wrong | 0.02 | 0.19 | 0.26 |
| any-letter no_obligation | 0.00 PASS | 0.00 PASS | 0.00 PASS |
| appointment-preparation recall | 0.37 | 0.12 | 0.14 |
| repeated-ask recall | 1.00 | 0.61 | 0.61 |
| dated-anchor / payment-anchored recall | 0.92 | 0.50 | 0.28 |
| undated-relative recall | 1.00 | 0.61 | 0.19 |
| discordant decisions vs baseline | — | 174 | 196 |
| verdict | FAIL | **FAIL** | **FAIL** |

**Eleven of the thirteen points lost were already lost at 0.3.1.** The
four large shapes are present in both arms in almost the same numbers:
41 emptied appointment deadlines (0.3.2: 44), 43 missed asks including
the same 25 second printings of a repeated ask (0.3.2: 34), 30 decisions
whose printed text is right but whose base no longer resolves (0.3.2:
43), and 18 payment periods rewritten (0.3.2: 24). So the regression
belongs to the 0.3.1 list-attribution and amount-selection paragraphs,
not to 0.3.2's one added sentence; 0.3.2 deepens two of the four shapes
and fixes none of them.

The three paragraphs 0.3.1 added were each written from a corpus case
and each does something the bed did not ask for:

- *record each action only at the passage that states it* suppresses the
  second printing of a genuinely repeated ask — 25 decisions.
- *an introduction that only supplies a deadline has no action of its
  own* strips the deadline from appointment-preparation asks that were
  reading correctly — 41 decisions, and it is the largest single loss.
- the amount-selection paragraph moved the reading of `from`, so a
  period that resolved against the letter's dateline now names the ask's
  own passage and resolves to nothing — 30 decisions.

**Disposition.** 0.3.1 and 0.3.2 are both withdrawn as merge candidates;
0.3.0 remains the best prompt Kettle has. The two real product defects
the corpus found — the annual-total selection and the list duplicate —
are worth about 2 decisions on the bed and cost 60 to fix this way. Any
next attempt should change **one thing**, be measured on the full
bed before the 43 cases are consulted at all, and treat a corpus case as
a way to see a failure, never as the surface a prompt is fitted to. The
apostrophe echo failure in `exec.rs` was fixed the same evening: pairing
now reads the four typographic quote marks as their ASCII cousins and
nothing else, and a replay of the `old` arm's recording through the fix
flips `annual-total-due` from unpaired to £960.00 verified with every
request still matching exactly. No bed fixture prints a curly quote, so
recorded bed scores are unaffected.
