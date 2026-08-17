# fixtures/run-01

A synthetic example of one completed run's outputs, derived from
`packs/app.kttl.subscription-audit/fixtures/statement-01.csv` (invented merchants,
no real data). Two consumers:

- **Design work** — screens 2–4 prototype against this data (report
  viewer, action review).
- **Frontend tests** — the Vitest render tests (#45) run against
  `fixtures/run-01`.

`results.json` is the source of truth for the report. Regenerate the
HTML through the same renderer a real run uses (#225), from the
repository root:

```sh
cargo run -p kettle -- render \
  fixtures/run-01/results.json \
  packs/app.kttl.subscription-audit/report.html.tera \
  --output fixtures/run-01/report.html
```

Read the HTML diff before committing it. Reports are self-contained and
keep the stylesheet present at render time, so regenerating deliberately
updates this example to the current report design; it does not rewrite
reports saved by real runs.

The shapes carry the design-philosophy requirements filed on the tracker:
every finding has `evidence` (the transactions that produced it — #26),
needs-review items have plain-language `reason`s (#23), and actions are
proposals that export as .ics/text, never executed (#30).

**Money is strings, not JSON numbers.** JSON numbers become floats in
JavaScript; amounts stay exact by staying strings end to end. The runner
emits them this way (`rust_decimal` serialises to string).

The fixture still uses the original `kettle/run-report@0` wire version,
which is the shape the runner currently reads and writes. It predates
claim kinds (#366), so a report rendered from it shows no kind marks —
the letter and renewal packs are where those render. Do not
hand-edit `report.html`; change `results.json` or the template and run
the command above.
