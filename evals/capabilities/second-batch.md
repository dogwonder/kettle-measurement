# Replay and resume — 6 September 2026

Work package 2 of [the plan](../../plan.md) is implemented locally. New disk
recordings retain the complete versioned generation request, and replay uses
the same identity as in-memory recordings. The CLI regression reproduces the
expanded-enum case: the old response still validates against the new schema,
but replay refuses it. An identical request replays successfully.

Legacy prompt-only archives remain usable with an explicit CLI message and
`replay_compatibility` metadata. Their schema compatibility remains unknown.
Missing, corrupt or unsupported new identities are refused. Mixing a legacy
copy into an exact recording cannot restore fallback for a changed schema.
Existing archives were not rewritten.

Resume now hashes the complete effective pipeline and execution configuration.
The input inventory is in [the evaluation guide](../README.md#replay-compatibility-and-resume-inputs).
The first two regressions were observed failing before the fix: changing only
the upstream normalisation prompt, or only model context with the same filename,
incorrectly reused one result. They now miss at the evaluator boundary; an
unchanged configuration reuses the complete fixture result.

Additional mock checks change same-filename weight bytes, in-memory batch and
kind mappings, examples, upstream schema, render template, runtime policy,
threads, environment identity, runtime binary, bundled libraries, quantisation
and machine metadata. Missing weight identity disables caching. New cache keys
use a v2 namespace; old cache files are left intact and cannot match.

Weight files in these tests are synthetic bytes used only for hashing. The
tests start local mock HTTP servers and never load model weights. Runtime and
file inputs must remain frozen during an evaluation. Adjacent bundled shared
libraries are included with the executable; the inherited environment is
represented only by a digest. Executable and environment identity favour extra
misses over reuse across uncertain configurations.

## Validation

Focused checks pass: 16 runner replay tests, seven effective-input resume tests,
six run-directory tests and the CLI disk-replay acceptance test. The existing
resume, mutation and evaluator suites also pass.

| Check | Result |
|---|---|
| Root formatting and Clippy, all targets/features | Passed |
| Root Rust tests, all features | 1,038 passed |
| Separate app formatting and Clippy, all targets/features | Passed |
| Separate app Rust tests, all features | 147 passed; one existing local-model test ignored |
| Proposed public projection, separate Cargo target | 1,038 tests passed |
| Public projection `claims`, `packs list --json`, `scores` | All exited successfully; existing claim statuses retained |

The projection was built using a disposable Git index containing the proposed
new paths as well as tracked paths. It read current working-tree contents; the
real index was untouched. Its full suite ran offline in a separate Cargo target
directory, so source-workspace artifacts could not satisfy a missing resource.
This checks the proposed files; CI should still verify the eventual committed
projection. Final documentation-only validation notes were added afterwards.

The frontend checks were not rerun: this batch adds optional evaluation metadata,
with no change to the saved application report or frontend interfaces.

```sh
cargo test -p runner --test replay --test resume_inputs --test run_dir
cargo test -p kettle --test replay_identity
```

No pack prompt, schema, scoring version, baseline or tier changed. There was no
model evaluation; #628 remains paused. Changes were uncommitted at validation;
the plan records subsequent commits. Work package
3's broader capability inventory is next, followed by the shared corpus slice.
