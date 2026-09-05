# fixtures/persisted

One run directory per **supported persisted schema**, exactly as the
app writes it (minus `raw/`, which hydration never reads). These are
the mechanism behind rule 2 of the persisted-schema policy
(`runner::results::schema_version`, #419): every version this build
reads has a committed directory that `hydrate_runs` reopens in
`app/src-tauri/src/core.rs::every_supported_persisted_shape_reopens`.
A reader nothing exercises is a reader that will silently stop reading,
which is how saved comparisons vanished twice under `@0`.

| directory | document | how it was made |
|---|---|---|
| `run-report@0` | subscription audit, `results.json` + `actions.json` + `report.html` | the no-model floor over the pack's own synthetic statement |
| `comparison-report@0` | renewal comparison | the canned renewal answer the app tests use |
| `letter-report@0` | letter | the canned letter answer over a committed bed fixture |
| `pending-letter@0` | a letter parked on its date (`pending.json`, no results) | the app tests' parked outcome |
| `pending-letter-untagged` | the same, as written before the tag existed: no `schema`, no `Obligation.shown` | derived from the row above by deleting those keys |

Wholly synthetic (CLAUDE.md, data rules): invented merchants, a bed
letter, an invented parked ask. Nothing here came from a real document.

**When a shape changes**: bump the version, add a directory for the new
one, keep the old one for as long as the app reads it, and delete it
when support is withdrawn — the test reads what is here, so a stale
directory fails rather than rots.
