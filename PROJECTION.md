# This tree is a projection

Generated from `dogwonder/kettle` at `34e9860` on 2026-08-24, carrying 2587 files. It is not edited here: every file is a copy, and a change made in this repository would be overwritten by the next projection rather than reaching the product.

## What it carries

- `crates/`
- `packs/`
- `evals/`
- `assurance/`
- `fixtures/`
- `scripts/`
- `privacy-boundary.toml`
- `Cargo.toml`
- `Cargo.lock`
- `LICENSE`
- `NOTICE`
- `README.md`
- `.gitignore`
- `app/src-tauri/models.json`
- `app/src-tauri/tauri.conf.json`

That is the measurement layer — the pipeline crates, the task packs with their prompts and their development and exam beds, the committed baselines, and the assurance registry. The Tauri shell and the Svelte frontend are not here, so links in `README.md` to `CLAUDE.md`, `app/DECISIONS.md` and `app/RELEASE-CHECKS.md` point into the half that stays closed.

## What that means for the registry

`assurance/claims.json` names the evidence behind each product-level claim, and a few of those citations are surfaces or tests inside the closed half. Validation reads this marker and treats those as absent by design; every citation the boundary *does* publish is checked here exactly as strictly as it is in the source tree. Run `cargo run -p kettle -- claims` to re-derive the statuses rather than trusting the ones recorded in the file.

## Reproduction

The code and the beds are here; the model weights and the llama-server sidecar are not, and neither is redistributable from here. The honest wording is *inspectable, and re-runnable given the weights* — `evals/README.md` names the weights each baseline was recorded against.
