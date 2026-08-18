//! The declared network boundary, and the guard that holds source to
//! it (#233).
//!
//! Kettle's promise is that task data stays on the machine. The model
//! path is only one part of that: telemetry, crash reporting, update
//! checks, remote report assets or a transitive dependency could each
//! open a channel while inference remains perfectly local.
//!
//! So the boundary is written down, in `privacy-boundary.toml`, and
//! this crate fails the build when source grows a network path the file
//! does not declare.
//!
//! **What this can and cannot prove.** A source scan proves that no
//! *undeclared call site* was added. It does not prove what a packaged
//! release does at runtime — a dependency's own background thread is
//! invisible to it — and the audit that produced this crate found
//! exactly that shape of thing, in `jsonschema`'s default features.
//! #233 requires a dynamic observation of a packaged build as well.
//! That half is **not written yet**: `app/RELEASE-CHECKS.md` will be
//! where the method lives, and has no network section until it is.
//! Neither check substitutes for the other, and this crate must not be
//! described as though it did.
//!
//! Its own crate rather than a test inside `runner` for two reasons:
//! the scan must see `app/` too, which is a separate Cargo workspace,
//! and a source scanner has no business shipping inside the runner
//! library.

mod scan;

use std::path::{Path, PathBuf};

/// One network path Kettle is allowed to use, as declared in
/// `privacy-boundary.toml`.
///
/// `reason` and `added` are not decoration. A boundary file whose
/// entries cannot be dated or justified is a list of exceptions, and
/// the point of writing it down is that a reviewer can ask why each one
/// is there — the same rule `STAGED_GOVUK_COMPONENTS` follows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct DeclaredPath {
    /// Where in the source it lives, repo-relative.
    pub file: String,
    /// What reaches the network, in the terms the scanner reports.
    pub calls: String,
    /// Loopback model traffic, an explicit model download, or neither.
    pub kind: PathKind,
    pub reason: String,
    pub added: String,
}

/// What a declared path is *for*. The distinction #233 turns on:
/// loopback traffic to our own sidecar and a download a person
/// explicitly asked for are both fine, and nothing else is — with one
/// later addition that is not traffic at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathKind {
    /// 127.0.0.1 only, between the runner and llama-server.
    Loopback,
    /// External HTTPS, reached only after a person chooses to download
    /// weights.
    ExplicitModelDownload,
    /// An address printed on a public page, added 18 August 2026 when
    /// the measurement layer went public (#478).
    ///
    /// The kind exists because the scanner reads source, and a URL a
    /// reader may click looks in source exactly like a URL the product
    /// fetches. The difference is real: nothing is requested until a
    /// person acts, nothing is sent that any click does not send, and
    /// the surfaces this may be declared on are not in the packaged
    /// app. That last part is the only half a reviewer cannot check by
    /// reading, so it is the half [`PUBLISHED_ADDRESS_SURFACES`]
    /// enforces.
    PublishedAddress,
}

/// Where an address may be printed, as repo-relative prefixes.
///
/// A kind with no constraint is a label: `published_address` on a
/// telemetry call in `exec.rs` would pass a test that only checks the
/// word. One surface, the assurance registry, which `bundle.resources`
/// does not contain — so an address declared this way cannot be one
/// the product takes.
///
/// The evidence pages render these addresses and declare none of their
/// own, which is a property of the registry being the single place an
/// address is written rather than a rule about screens. It is also why
/// this list did not need `app/demo/`: the public projection does not
/// carry `app/`, so an entry pointing there would be a live declaration
/// here and a stale one in the tree it describes.
pub const PUBLISHED_ADDRESS_SURFACES: [&str; 1] = ["assurance/"];

/// A network call site the scanner found in source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSite {
    pub file: PathBuf,
    pub line: usize,
    pub calls: String,
}

/// The committed boundary.
pub fn declared(root: &Path) -> Result<Vec<DeclaredPath>, String> {
    #[derive(serde::Deserialize)]
    struct Boundary {
        #[serde(default)]
        path: Vec<DeclaredPath>,
    }

    let file = root.join("privacy-boundary.toml");
    let text =
        std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.to_string_lossy()))?;
    let boundary: Boundary =
        toml::from_str(&text).map_err(|e| format!("{}: {e}", file.to_string_lossy()))?;
    Ok(boundary.path)
}

/// Every network call site in the workspace's source — Rust and
/// frontend, including `app/`.
///
/// Sees `reqwest` / `ureq` / raw `TcpStream` / `std::net` in Rust, and
/// `fetch` / `XMLHttpRequest` / `WebSocket` / `EventSource` plus remote
/// URLs in stylesheets and templates on the frontend.
///
/// What it deliberately does not read — vendored and generated trees,
/// test code, and development-only scripts — is listed with its
/// reasoning in `privacy-boundary.toml`, next to the inventory it
/// affects, rather than here where a reviewer checking the boundary
/// would not look.
pub fn call_sites(root: &Path) -> Vec<CallSite> {
    scan::scan(root)
}
