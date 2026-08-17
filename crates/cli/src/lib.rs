//! The parts of the `kettle` command that are worth testing on their own.
//!
//! `doctor`, `parse` and `packs list` are thin enough to live in the
//! binary. `eval` is not: it has a baseline comparison, exit codes CI
//! depends on and a table people read, none of which should need a model
//! to test (CLAUDE.md — CI runs fmt, test and clippy only, and evals run
//! locally).

pub mod bed;
pub mod claims;
pub mod eval;
pub mod mutate;
pub mod packs;
pub mod project;
pub mod render;
pub mod scores;
