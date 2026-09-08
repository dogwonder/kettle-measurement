# Source facts, pack expectations and task slots

Updated 8 September 2026. Corpus scoring v6 adds explicit interpretations of
the first measurement; pack scoring remains 19. The v5 recordings and source
outcomes are retained. No model answer authors an expected value.

## Separate questions

| Report field | Question answered | What it does not establish |
|---|---|---|
| `score.asks[].raw` | Did the candidate copy the authored source words? | Semantic equivalence of different wording. |
| `score.asks[].verified` | Does the final field agree with the authored source fact? | Agreement with a pack's implicit policy, task meaning or evidence attachment. |
| `score.asks[].evidence` | Is the reading attached to the authored source site? | Whether a different site is permitted by the pack's reading contract. |
| `deadline_contract.reading` / `.evidence` | Did the candidate copy the expected reading at its expected site? | Correct period structure or calendar resolution. |
| `deadline_contract.structure` | Does the returned period structure match the authored expectation? | Correctness of the final date. `null` means no structure expectation was authored. |
| `deadline_contract.resolution` | Does the final date satisfy the explicitly authored policy, or source facts when no policy is declared? | Proof that a default was explicitly stated in the document. |
| `score.task_slots` | How many authored slots and returned candidates matched, and where are the unmatched candidates? | Whether action content was omitted, duplicated or invented. |

The original `whole_item_*`, `missed_*` and `invented_*` keys are retained for
compatibility. `whole_item` covers selected source fields only; `missed` counts
unmatched authored slots and `invented` counts unmatched candidates. Prefer
`task_slots` when describing these counts. Semantic action coverage is always
`not-assessed`; retained candidate `text` and indices allow direct inspection.
A combined task can contain both actions while matching only one slot.

Unmatched candidates are classified only by their authored site:
`negative-site`, `additional-candidate-at-ask-site`, or `no-authored-ask-site`.
No text-matching heuristic turns those classes into a semantic verdict.

## Deadline annotations

`deadline_words` and `deadline_at` continue to identify the original source
phrase. An optional `deadline_reading: {at, value}` declares a different pack
reading expectation. Its text must occur at the declared source passage, and
acquisition maps it into actual segment coordinates before scoring.

The shipped letter prompt asks for a pointer's **target row date**. In
`relation-form-010`, the source phrase remains “by the date shown beside it”
at passage 2; the reading contract is “6 April 2026” at passage 3. Returning
the target date can satisfy that contract while failing source-phrase copying.
The prompt is unchanged; the two expectations are now distinguished.

`deadline_resolution: {policy, base, due}` declares a pack-policy result.
The initial named policy, `letter-date-default`, supports bare calendar-day
or week periods, with a dated base fact present in the document. Validation
refuses unsupported qualifiers, missing bases, arithmetic overflow, a date
inconsistent with the authored base/period, or an override of an explicit
source date/base. Other policies need their own reviewed declaration.

For `relation-form-001`, source truth remains ambiguous: “within 14 days”
does not explicitly name its base. The expected structure is corrected from
zero/no-period to 14 days. Separately, the shipped default uses the printed
10 March dateline and expects 24 March. This records an application convention,
not a new claim about what the document explicitly states. The generator
authors that distinction; it imports no Kettle parser or model answer.

## Reading the first replay

All original source scores remain unchanged: 9/28 raw and 25/28 verified
whole-field slots, one unmatched slot, one unmatched candidate, two source
evidence mismatches. The wrong annual-total selection still fails both amount
columns. No whitelist forgives omitted words or changed dates.

The additional deadline views record 10 correct / 17 wrong / 1 missing
readings; 25 correct / 2 wrong / 1 missing structures; and 27 correct / 0 wrong /
1 missing resolutions. Reading-contract evidence has one mismatch. The two
structure mismatches report `named_date` for a non-period where the authored
expectation is `none`; correct dates do not erase them.

These denominators include a task-slot mismatch whose recorded text contains
both requested actions. They are component-contract counts, not overall model
accuracy. Time, place, reference and semantic action meaning remain unsupported
or unassessed. A candidate comparison must state which of these questions it
intends to answer; it cannot use a single combined score as a product verdict.

## Evidence and validation

The implementation is committed as Kettle `395652dc`; generator annotations
are in kettle-examples `90e41a6` on `work/reading-capabilities`. The completed
replay, comparison with v5, derived coverage and checksums are retained in
kettle-runs `f5a36c65`, entry
`2026-09-08-corpus-diagnostic01-contracts-v6-replay`. These are local commits;
nothing was pushed. The original v5 archive `2185ad34` remains unchanged.

All 33 generation exchanges and every original source-score outcome matched.
Model, generation machine, runtime, sidecar and pipeline identities were
preserved; corpus/scoring identities changed explicitly. Coverage still derives
29 attempted inventory scopes with four selected scopes unsupported. There was
no new model generation, prompt edit, tier or baseline update.

Validation passed: 1,071 root Rust tests; 148 app Rust tests with one existing
ignore; formatting and Clippy in both workspaces; 20 Python checks and 24
generator tests. The tests retain wrong-date, wrong-structure, wrong-evidence,
wrong-amount, unmatched-slot and unsupported-capability outcomes. They also
refuse invalid policy dates/bases, unsupported policy declarations and overflow.
