# Amount/list comparison on a rented CUDA pod — plan and completed status

**Completed 8 September 2026.** The authorised sitting expanded to five
corpus arms and then three full-bed passes. [All four result stages](measurement-02-pod-results.md)
record the outcome: 0.3.1 and 0.3.2 were withdrawn and `main` was restored
to 0.3.0. Recordings are archived in `dogwonder/kettle-runs` at `d8d94223`.
The initial procedure below is historical; it is not authorisation to rent
or run again. Any next amount/list attempt changes one thing and is measured
on the full pod bed before consulting these 43 exposed corpus cases.

The JSON pins are retained as recorded. Current sources and pack bytes
have changed, so a new bundle from `main` cannot satisfy the old pins.
The Metal executable at `target/debug/kettle` was also overwritten; a Metal
run needs a fresh freeze. The chrono build defect described below was fixed
in `eb53e255`; the historical pod executable used `--features pdf`, which
remains the required build feature for pod work.

**Original preparation, before authorisation.** This is the Linux/CUDA
plan for the comparison [measurement 2](measurement-02.md) froze for Metal.
The Metal freeze and its JSON are unchanged; [`measurement-02-pod.json`](measurement-02-pod.json)
reuses its source, corpus, inventory, pack and weight pins and leaves every
pod-side identity `null` until the box has built them. No model process was
started, no pod was rented and nothing was pushed during preparation.

The box named for it is a RunPod RTX 4090 (Ada, `sm_89`), pod
`dhqfsu8me9r380`. A pod that exists is billing already; the sitting below is
bounded by wall clock, not by an estimate copied from an earlier run.

## What changes from the Metal plan, and what does not

| | Metal (measurement 2) | Pod (this plan) |
|---|---|---|
| Source, corpora, inventory, packs | 82 pinned files, two selections, seven inventories, two five-file packs | identical bytes; `bundle` and `freeze` both check every pin |
| Weights | installed copy, SHA-256 `13c16f42…` | downloaded on the pod or an existing copy, same SHA-256 required |
| Sidecar | `sidecars/macos-arm64`, b10145 Metal, eleven pinned dylibs | `sidecars/linux-x86_64`, b10145 built on the pod with CUDA, `KETTLE_CUDA_ARCH=89`, `libggml-cuda.so` required; every file hashed at freeze |
| Executable | `target/debug/kettle`, pinned | built on the pod with `cargo build --locked -p kettle --features pdf`, rustc 1.97.1; hashed at freeze. The feature is needed to compile at all (see below); no PDF is read |
| Request policy | context 8192, temperature 0, reasoning off, parallel 1, 4096 answer tokens, shipped retries | identical, verified from each sidecar log (`thinking = 0`, `using device CUDA0`) |
| Budget | 43 initial exchanges per arm, 1,200 s per arm | identical, plus a setup and sitting cap below |
| Environment | whatever shell launched it | one fixed `env -i` environment for every launch, recorded in the receipt, because `RuntimeIdentity` hashes the environment |
| What it can say | a Metal reading | a CUDA reading; **not comparable** with the Metal recordings at decision level (#596) and never a Metal tier |

Order is unchanged: old `diagnostic-01`, old `product-regressions-01`, then the
same two under the new pack. Both arms run freshly on the one pod and one
runtime; an archived old arm is never paired with a fresh new arm.

## The procedure, and what each step is allowed to spend

`scripts/corpus-pod.sh` is the whole procedure. `scripts/pod-eval.sh` is a
different workflow (a full pack bed and a baseline) and is not used.

| Step | Where | Spends | Refuses |
|---|---|---|---|
| `bundle` | home | nothing | a modified bundled path (other checkout changes are noted and excluded); any `*.private.*`, `.gguf`, `fixtures/` or `tests/` path; any byte that differs from the plan pins |
| `check-bundle` | home | a local build | a manifest mismatch; a no-model run that exits non-zero or leaves a case unscored |
| `preflight` | pod | two 512 MB write probes | not being on a GPU box; a llama-server already running |
| `setup` | pod | CUDA build, one weights download, CLI build, four no-model runs | no CUDA backend built; weights digest or byte count mismatch |
| `freeze` | pod | nothing; starts no model | any input differing from the plan pins |
| **review at home** | home | nothing | — the frozen JSON is what run approval is requested on |
| `run old`, `run new` | pod | model time under one 1,200 s deadline per arm | any frozen pin changed; an output directory that exists; `new` after an incomplete `old`; a running llama-server |
| `replay old`, `replay new` | pod | CPU only | a per-case score, summary or identity that differs; fewer exact requests than exchanges; any legacy match |
| `pack` | pod | nothing | — |

Budget, all of it wall clock:

- Pod initialise limit 15 minutes; setup limit 45 minutes; whole sitting
  120 minutes from first SSH to verified tarball at home, after which the
  sitting stops where it is. The frozen plan records the actual
  `costPerHr`; at 4090 rates that cap is a small number of pounds, and the
  cap is what is approved, not the estimate.
- Per arm, one shared monotonic deadline of 1,200 seconds started before the
  first CLI launch, covering both selections, both model loads, every retry
  and the gap between commands. At the deadline the CLI's process group,
  sidecar included, gets TERM then KILL; partial output stays; the next arm
  is not launched and nothing is rerun. The second selection never gets a
  fresh 1,200 seconds.
- 86 initial exchanges overall, 43 per arm. The shipped executor may add
  one schema retry per batch attempt, one pairing re-ask per original batch
  and recursive truncation splits; every exchange is recorded and
  `exchanges.json` counts them by kind from the executor's own markers.
  86 is not a total cap.
- No second model, no download during an arm, no `--runs`, no resume.

## Transfer and credentials

The bundle is `git archive` of tracked paths only, so the data rules'
guarantee that no `*.private.*` file is tracked is inherited. It carries
the 82 runtime sources, the two corpora, the seven inventories, the two
packs (five files each; the old one from `53124ce8`), the two plans, the
three scripts and a hashed manifest. No fixtures, tests, weights, sidecar
or `app/`. No deploy key or clone is needed on the pod, and nothing on the
pod ever pushes.

The proxy address (`ssh <pod>@ssh.runpod.io`) drops into a shell whatever
command it is given and does not carry `scp` reliably. Files go over the
exposed TCP port from `runpodctl pod get dhqfsu8me9r380` (`.ssh.ip`,
`.ssh.port`; `scp -P`) or through `runpodctl send` / `receive`. Verify the
digest of anything that crosses in either direction; the run directory
exists nowhere else until it lands.

On the pod, from the extracted bundle root, in a `tmux` session:

```sh
export CUDA_PATH=/usr/local/cuda    # ls /usr/local/cuda* if that is not it
./scripts/corpus-pod.sh preflight
./scripts/corpus-pod.sh setup       # KETTLE_CUDA_ARCH is chosen from the card; 89 for a 4090
./scripts/corpus-pod.sh freeze      # → evals/corpus/measurement-02-pod.frozen.json, send it home
```

Then, only after the frozen plan is reviewed and the run is approved:

```sh
./scripts/corpus-pod.sh run old && ./scripts/corpus-pod.sh run new
./scripts/corpus-pod.sh replay old && ./scripts/corpus-pod.sh replay new
./scripts/corpus-pod.sh pack
```

`run new` refuses unless `old/receipt.json` says `completed`, so the `&&`
is belt and braces. Each arm's directory holds its receipt, both sidecar
logs, three `nvidia-smi` readings, `exchanges.json`, the pack it ran with,
the frozen plan and the bundle manifest.

## Reading the result

As measurement 2 says: each selection and each arm separately, then paired
per-case changes in amount selection, negative-site assertions, unmatched
slots and candidates, source fields and evidence, deadline reading,
structure and resolution. Inspect the retained action text for the list
duplicate, `split-list`, `active-heading`, `merged-list`, `footer-task`
and the combined-action case; do not turn that reading into a scorer.
Correctness before any runtime figure, and runtime figures stay in the
receipt.

What this sitting cannot establish: a Metal tier, a full-pack or exam pass,
photograph accuracy, semantic action coverage, #625's schema-cost and
photo-table questions, #428's independence, or any cross-runtime claim.
The existing v19 baseline and tier failures stand. The weekly-versus-pod
merge-policy conflict is still a decision, not something a diagnostic run
settles. Archive only synthetic evidence, and only with separate
authorisation for `kettle-runs`.

## Local check of this preparation — before the chrono fix

`check-bundle` extracted the bundle into a scratch directory, verified its
manifest, built the CLI and ran the four no-model acquisition checks (each
pack over each selection: exit 0, zero unscored cases, no model identity).
The first attempt found that a **default-feature build of the CLI does not
compile on main**: `crates/runner/src/eval/challenge.rs` has called
`chrono::Utc::now()` since `70c2a1a8`, and the workspace's chrono carries
no `clock` feature; only `pdfium-render`, behind the `pdf` feature,
enables it. `cargo test` passes because dev-dependencies unify the feature
in, and the pinned Metal binary was evidently built with more than the
default set. So the pod builds with `--features pdf`, recorded in the
plan. The one-line fix (add `now` to the workspace chrono features) is not
applied here, because `Cargo.toml` is one of the 82 pinned sources and
changing it would void both freezes; it is a separate change to make
after this measurement. `scripts/pod-eval.sh` builds with the default set
and would fail at the same line on main today.
