//! `kettle claims` — the assurance-claims registry, rendered (#434).
//!
//! One line per claim, effective status first. The recorded status is
//! kept beside a downgraded verdict ("recorded proven") because the
//! difference between the two *is* the news: it says a measurement
//! existed and the world moved, which reads nothing like "never
//! measured".
//!
//! Exit codes mirror `mutate`'s contract: 0 the registry is a sound
//! record — downgrades and failures are states, not errors; 1
//! user-facing copy stands on a claim that is no longer proven, which
//! is the one staleness someone must act on today; 2 the registry
//! itself cannot be trusted, and no number it renders should be quoted
//! until it can.

use runner::assurance::{Registry, Status};
use serde_json::json;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Ok = 0,
    StaleCopy = 1,
    Broken = 2,
}

pub struct Outcome {
    pub text: String,
    pub code: ExitCode,
}

fn broken(text: String) -> Outcome {
    Outcome {
        text,
        code: ExitCode::Broken,
    }
}

fn read_registry(root: &Path) -> Result<Registry, Outcome> {
    let registry_path = root.join("assurance/claims.json");
    let text = std::fs::read_to_string(&registry_path)
        .map_err(|e| broken(format!("Could not read {}: {e}\n", registry_path.display())))?;
    Registry::from_json(&text).map_err(|e| broken(format!("{e}\n")))
}

fn exit_code(assessment: &runner::assurance::Assessment) -> ExitCode {
    if !assessment.refusals.is_empty() {
        ExitCode::Broken
    } else if !assessment.stale_copy.is_empty() {
        ExitCode::StaleCopy
    } else {
        ExitCode::Ok
    }
}

/// Read, validate and render `assurance/claims.json` under `root`.
///
/// `current_scoring_version` and `today` come from the edge, like every
/// clock and version in this crate, so this stays a function a test can
/// hold still.
pub fn run(root: &Path, current_scoring_version: u32, today: chrono::NaiveDate) -> Outcome {
    let registry = match read_registry(root) {
        Ok(registry) => registry,
        Err(outcome) => return outcome,
    };

    let assessment = registry.validate(root, current_scoring_version, today);

    let mut out = String::new();
    if !assessment.refusals.is_empty() {
        let count = assessment.refusals.len();
        let plural = if count == 1 { "" } else { "s" };
        out.push_str(&format!(
            "{count} problem{plural} make the registry untrustworthy as a record:\n"
        ));
        for refusal in &assessment.refusals {
            out.push_str(&format!("  - {refusal}\n"));
        }
        out.push('\n');
    }

    for claim in &registry.claims {
        let Some(assessed) = assessment
            .claims
            .iter()
            .find(|assessed| assessed.id == claim.id)
        else {
            continue;
        };
        // FAILED is deliberately the loud one: a claim measured false
        // is the row a skimming reader must not slide past.
        let label = match assessed.effective {
            Status::Proven => "proven  ",
            Status::Unproven => "unproven",
            Status::Failed => "FAILED  ",
        };
        let history = if assessed.recorded != assessed.effective {
            format!(" (recorded {})", status_word(assessed.recorded))
        } else {
            String::new()
        };
        out.push_str(&format!(
            "{label}  {} — {}{history}\n",
            claim.id, claim.wording
        ));
        for reason in &assessed.reasons {
            out.push_str(&format!("            {reason}\n"));
        }
    }

    if !assessment.stale_copy.is_empty() {
        out.push('\n');
        for stale in &assessment.stale_copy {
            out.push_str(&format!("Stale copy: {stale}\n"));
        }
    }

    let code = exit_code(&assessment);
    Outcome { text: out, code }
}

/// The same assessment as [`run`], as a build input for public claim
/// surfaces. The recorded status remains beside the effective status:
/// a frontend must never import claims.json and accidentally publish a
/// stale recorded `proven` as today's verdict.
pub fn run_json(root: &Path, current_scoring_version: u32, today: chrono::NaiveDate) -> Outcome {
    let registry = match read_registry(root) {
        Ok(registry) => registry,
        Err(outcome) => return outcome,
    };
    let assessment = registry.validate(root, current_scoring_version, today);

    let claims = registry
        .claims
        .iter()
        .filter_map(|claim| {
            let assessed = assessment
                .claims
                .iter()
                .find(|assessed| assessed.id == claim.id)?;
            let mut projected = json!({
                "id": claim.id,
                "wording": claim.wording,
                "status": assessed.effective,
                "recorded_status": assessed.recorded,
                "status_reasons": assessed.reasons,
                "scope": claim.scope,
                "evidence": evidence_with_addresses(claim, &registry),
                "recorded": claim.recorded,
                "invalidation": claim.invalidation,
                "review_route": claim.review_route,
            });
            // What the claim's own record says it found (#478). An
            // issue citation is the one evidence kind a reader outside
            // the tracker cannot open, and the cited issues sit on the
            // failed and unproven claims — the rows the falsifiers page
            // exists to publish. The note is the substance, already
            // written and already reviewed; withholding it left the
            // page showing a number and nothing else. Omitted rather
            // than emitted empty, so a claim needing no gloss is not
            // made to invent one.
            if let Some(note) = &claim.note {
                projected["note"] = json!(note);
            }
            Some(projected)
        })
        .collect::<Vec<_>>();

    let mut document = json!({
        "schema": "kettle/public-claims@0",
        "claims": claims,
    });
    // Where the tree these paths live in was published (#478). Emitted
    // rather than assumed by the page, and omitted rather than emptied,
    // so a registry validated before the flip projects exactly as it
    // did — with descriptions where there are no addresses.
    if let Some(published_at) = &registry.published_at {
        document["published_at"] = json!(published_at);
    }
    if let Some(recordings_at) = &registry.recordings_at {
        document["recordings_at"] = json!(recordings_at);
    }
    let text = serde_json::to_string_pretty(&document).expect("public claims serialise") + "\n";
    let code = exit_code(&assessment);
    Outcome { text, code }
}

/// A claim's evidence, each item carrying the address a reader outside
/// this repository would open it at (#478), where the declared boundary
/// publishes it. Evidence outside the boundary keeps its description and
/// grows no link: real here, unopenable there, and the difference must
/// not be allowed to look like unevidenced.
fn evidence_with_addresses(
    claim: &runner::assurance::Claim,
    registry: &runner::assurance::Registry,
) -> serde_json::Value {
    let published_at = registry.published_at.as_deref();
    claim
        .evidence
        .iter()
        .map(|evidence| {
            let mut projected = json!(evidence);
            if let Some(url) = evidence.public_url(&registry.published, published_at) {
                projected["url"] = json!(url);
            }
            projected
        })
        .collect::<Vec<_>>()
        .into()
}

fn status_word(status: Status) -> &'static str {
    match status {
        Status::Proven => "proven",
        Status::Unproven => "unproven",
        Status::Failed => "failed",
    }
}
