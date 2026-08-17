//! Kettle pipeline engine: executes task packs against local files via a
//! llama-server sidecar. See CLAUDE.md for the locked decisions.

pub mod actions;
pub mod aggregate;
pub mod assurance;
pub mod cache;
pub mod claim;
pub mod claim_trace;
pub mod cleanup;
pub mod comparison_report;
pub mod document;
pub mod download;
pub mod eval;
pub mod exec;
pub mod fmt;
pub mod kinds;
pub mod letter_report;
pub mod modality;
pub mod ocr;
pub mod packs;
pub mod parse;
pub mod pdf;
pub mod recurrence;
pub mod render;
pub mod results;
pub mod run;
pub mod run_dir;
pub mod scoring;
pub mod sidecar;
pub mod terms;
pub mod timeline;
