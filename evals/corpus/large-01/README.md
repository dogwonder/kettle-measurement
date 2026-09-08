# A busy year's generated statement (#256)

`generate.py` authors 2,400 wholly synthetic transactions across 2025, using
integer pence, and renders the same rows as CSV and a 60-page PDF. It imports
no Kettle parser or model. `fixtures/expected.json` records counts, net pence
and source/output hashes. Regenerate into a new directory with Python and
ReportLab:

```sh
python3 evals/corpus/large-01/generate.py /tmp/new-large-statement
```

The real CLI statement reader parses both forms. Their complete printed
transaction outputs matched on 8 September 2026: 2,400 rows, including the last
page and 31 December. This is acquisition coverage at a busy year's scale.
It does not restore the withdrawn subscription product or claim model accuracy
on a long document. The shipped letter pack retains its three-page limit.

No duration estimate is fitted: #256's 25 August correction withdrew that
criterion. Timings remain sitting-local diagnostics. The paired letter model
measurement is recorded separately in `../measurement-03-formats.json`.
