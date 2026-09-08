# PDF/large-input coverage and external challenge — 8 September 2026

The remaining implementation and first-measurement scope of #256 and #428
is complete, with failures retained. This is not a passing letter-pack verdict
or a claim that all input formats work. The shipped pack remains 0.3.0,
scoring 19; corpus scoring remains v6. No prompt, schema, scorer or ceiling
changed during this work.

## #256: model answers through the real formats

The frozen [format plan](measurement-03-formats.json) ran the existing two
authored letters as text, PDF and photographs: six case executions, four
expected asks per arm and two explicit cancellation negative sites per arm.
Qwen3.5-4B Q4_K_M ran on the M1 Pro with b10145 Metal, context 8192,
temperature zero, reasoning off, parallel one and the ordinary retry policy.
Each arm made exactly two exchanges; no retries or extra model attempts ran.

| Input | Scored asks | Missed asks | Extra assertions | Correct resolved dates | Correct amounts, including authored absence |
|---|---:|---:|---:|---:|---:|
| Text | 4 | 0 | 1 | 4/4 | 4/4 |
| PDF | 4 | 0 | 0 | 2/4 | 4/4 |
| Photograph | 4 | 0 | 0 | 2/4 | 4/4 |

All six cases acquired and scored; none was excluded for missing text or
ambiguous attribution. In the one-page PDF/photo case the model supplied
relative structures (19 and 14 days from the dateline) alongside the printed
absolute dates. Verification retained the words but refused to compute those
unsupported structures, so the two resolved dates are absent. The three-page
case retained both dates. Text supplied one extra assertion. Exact copying of
the deadline phrase is a separate score, retained in the
[machine-readable results](measurement-03-04-results.json); resolved-date
correctness must not conceal that copying disagreement.

The [large fixture](large-01/README.md) supplies a generated busy year:
2,400 wholly synthetic transactions, 120 credits and 2,280 debits, rendered as
CSV and a 60-page PDF. The real statement reader produced byte-identical
transaction output for both, with all 2,400 rows and the independently authored
net of 613,600 pence. The generator and expectations are committed beside the
assets, and regeneration was byte-identical. This exercises large statement
acquisition; it does not measure model performance on a long letter or relax
the letter pack's three-page limit.

The original duration-estimate criterion was explicitly withdrawn by
[the 25 August correction](https://github.com/dogwonder/kettle/issues/256#issuecomment-5403368664).
No fitted prediction or duration copy is owed. Elapsed times remain in the
run receipts as sitting-local telemetry only.

## #428: external wording and private field workflow

[Three UKHSA templates](challenge-01/README.md) supply one externally authored
wording family. Original DOCX files, URLs, hashes, adaptation source and
licence are retained. They reuse no Kettle development/exam templates.
The developer authored the adaptations, annotations and plain rendering;
this is not independent adjudication, preservation of the original Word
layout or three independent family-level samples.

The [challenge plan](measurement-04-challenge.json) froze the source and
truth before answers, then consumed the lifecycle for one attempt. The
unchanged 0.3.0 pack saw a text case and a PDF case, making two exchanges:

- Both retained the booking ask but labelled it `attendance` against the
  authored `response` (phone to arrange an appointment).
- Both put booking words in the deadline reading; neither invented a resolved
  date or payment amount. The visible task words still say to book; a wrong
  enum is not evidence that the task text told somebody to attend immediately.
- No additional assertions were recorded at the 36 scored negative sites.
- The third case's PNG scan was refused before any model request because this
  pack declares JPEG/HEIC, not PNG. This is a preparation error and an
  unscored acquisition outcome, not model performance on an image. The first
  two scored asks are not a denominator of three.

The failed image case is retained unchanged. The ledger is consumed, and no
fresh attempt or changed answer was used to improve the result. A future JPEG
version of this exposed wording would be regression evidence, not a fresh
holdout. The separate #256 run above supplies actual photographed model
evidence; it does not retroactively supply this challenge's missing image arm.

The [private workflow](FIELD-EVIDENCE.md) starts from ordinary packaged-app
run evidence. It creates gitignored owner-only observations, records original
report/manifest plus explicit model/pack/runtime provenance, and leaves every
claim unreviewed until adjudicated. Correct, wrong, incomplete, unsupported
and uncheckable remain separate. Missing claims have an explicit target.
Runtime declarations name their basis; the older desktop receipt cannot
retroactively provide a complete sidecar digest.

The private registry records each discovery, its coverage category, issue and
synthetic reproducer locators, and missing follow-ups. Public export constructs
only counts and closed shape categories under `field-summary.schema.json`.
The command never publishes a private file or interprets approval as truth.
The workflow was exercised on private-style synthetic evidence; no actual
private document was inspected, adjudicated or exported during this sitting.

## Replay, validation and retained evidence

Exact replay reproduced all case scores, raw/verified readings, acquisition
and attribution outcomes and summaries: two exact requests per format arm,
and two for the three-case challenge including its input refusal. Zero legacy
request matches. The challenge replay retains the consumed lifecycle.

Passed: 1,080 root Rust tests, 35 Python checks, formatting and all-target,
all-feature Clippy. The privacy export acceptance test validates its explicit
schema and rejects added content. The source scan names just two non-runtime
challenge JSON files containing published addresses; a red-first test keeps
adjacent code and other JSON scanned. Native Vision and loopback tests required
execution outside the sandbox. Both new generators reproduced their assets
byte for byte.

The archive entry is `2026-09-08-formats-and-external-challenge-v19` in
`dogwonder/kettle-runs`: all original/replay directories, receipts, source
snapshots, pack, generated inputs, source templates, sidecar logs and hashes.
No model weights, executable, private observations or real personal records
belong in that entry. Its MANIFEST gives relocation/replay instructions.

## Remaining product work

The date-structure refusals, extra task and booking-kind discrepancy remain
reading failures under #595. This sitting did not attempt a fix. #625 still
owns its old/new schema-token comparison and photo-table requirements;
these simpler format cases do not fulfil those. The current failing baseline
and tier are unchanged. Further long-letter model coverage, broader independent
families and another held-out claim require their own explicit selection;
they are not prerequisites for maintaining the workflows delivered here.
