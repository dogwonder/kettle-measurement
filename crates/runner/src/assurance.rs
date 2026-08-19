//! The assurance-claims registry (#434): product-level claims, each
//! naming the evidence it stands on and the changes that stop it
//! applying.
//!
//! Kettle's evidence is strong but distributed — pack manifests, tiers,
//! baselines, tests, issues, release checks, product copy — and a
//! public sentence can outlive the measurement that once made it true.
//! The registry is the one reviewable place that joins the sentence to
//! its measurement. It does not replace the detailed evidence; it
//! points at it, and validation says what the pointer is worth today.
//!
//! Two kinds of wrongness are kept distinct throughout:
//!
//! - a **refusal** is a registry that cannot be trusted as a record — a
//!   claim whose evidence file does not exist, a proven claim with no
//!   invalidation triggers. CI fails on these, because the registry
//!   itself is broken.
//! - a **downgrade** is a claim whose evidence no longer supports it —
//!   a baseline from an earlier pack or scoring version. The registry
//!   is fine; the world moved. The claim's effective status becomes
//!   unproven, with the mismatch named, and CI stays green. This is
//!   deliberate: evidence goes stale on someone else's merge (a
//!   `SCORING_VERSION` bump lands and every measured claim ages in
//!   place), and a registry that turned that into a red build would be
//!   deleted within a week.
//!
//! Validation never rewrites the file. The recorded status is what was
//! believed when the claim was recorded; the effective status is what
//! the evidence supports now; rendering both is how "proven last week,
//! unproven since Tuesday's bump" stays visible.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// What a claim's evidence supports.
///
/// `Failed` is not an error state: a claim measured and found false —
/// "a quote identifies the value it evidences" after #460 — is one of
/// the most useful records the registry holds, and it must render as
/// its own state, never blur into "not yet measured".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Proven,
    Unproven,
    Failed,
}

/// What a claim is about. Every field is optional because claims range
/// from pack-and-model-specific ("this model clears these ceilings") to
/// product-wide ("input files are read-only"); validation checks each
/// declared field against the evidence, and an undeclared field simply
/// does not constrain.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack_version: Option<String>,
    /// The model file name, as baselines record it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval_set: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
}

/// A pointer to something a reviewer can open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Evidence {
    /// An eval baseline written by `--write-baseline`. Carries its own
    /// provenance — scoring version, pack version, model, eval set —
    /// which validation reads and compares against the claim's scope.
    Baseline { path: String },
    /// A tracker issue. Context and history, never proof: an issue is
    /// where a finding is discussed, not where it is measured, so this
    /// kind can carry a failed or unproven claim but cannot make one
    /// proven.
    Issue { number: u32 },
    /// An owning test: the repo-relative file and the test name within
    /// it. Validation checks the name is really in the file, so a
    /// renamed or deleted test cannot leave a claim standing on air.
    Test { path: String, name: String },
    /// A dynamic or manual release check — a packaged-build network
    /// capture, a signing verification — that CI cannot execute. "Not
    /// automatable" is not "not evidenced": this is evidence, with the
    /// discipline that it says when it was taken and when it stops
    /// counting. Past `expires`, the claim it carries is downgraded.
    Manual {
        description: String,
        recorded: chrono::NaiveDate,
        expires: chrono::NaiveDate,
    },
}

impl Evidence {
    /// Whether this kind can carry a proven claim on its own.
    fn probative(&self) -> bool {
        match self {
            Evidence::Baseline { .. } | Evidence::Test { .. } | Evidence::Manual { .. } => true,
            Evidence::Issue { .. } => false,
        }
    }

    /// The repo-relative file a reader would open, where there is one.
    /// A manual check has no file — it is described in the registry
    /// itself — and an issue is a tracker reference, not a path.
    fn path(&self) -> Option<&str> {
        match self {
            Evidence::Baseline { path } => Some(path),
            Evidence::Test { path, .. } => Some(path),
            Evidence::Issue { .. } | Evidence::Manual { .. } => None,
        }
    }

    /// Where a reader outside this repository opens this evidence.
    ///
    /// Derived from the same `inside_boundary` sentence the validator
    /// and `kettle project` read, so a path cannot be publishable to the
    /// projection and unlinkable on the page, or the reverse. Evidence
    /// with no file has no address: a manual check travels as its own
    /// description, and an issue is a private tracker reference whose
    /// substance is the claim's note.
    pub fn public_url(&self, published: &[String], published_at: Option<&str>) -> Option<String> {
        let base = published_at?;
        let path = self.path()?;
        inside_boundary(path, published)
            .then(|| format!("{}/blob/main/{path}", base.trim_end_matches('/')))
    }

    /// Whether a reader outside this repository could open it.
    ///
    /// A manual check counts: its description travels in the registry,
    /// which is itself published. An issue does not carry a proven
    /// claim in the first place, so its reachability never decides one.
    fn publicly_reachable(&self, published: &[String]) -> bool {
        match self.path() {
            None => true,
            Some(path) => inside_boundary(path, published),
        }
    }
}

/// The file the generated public tree carries to say what it is (#478).
///
/// `kettle project` writes it. Its presence is the only thing that tells
/// validation which tree it is looking at, and the distinction is load
/// bearing: in the working tree a missing surface means somebody deleted
/// the copy a claim depends on, and in the projection the same absence is
/// the boundary doing its job.
pub const PROJECTION_MARKER: &str = "PROJECTION.json";

/// Whether a repo-relative path is one the declared boundary publishes.
///
/// One sentence, read the same way everywhere: the registry's evidence
/// check, the surface check and `kettle project`'s selection all mean the
/// same thing by "published", so a path cannot be publishable to one and
/// not to another.
fn inside_boundary(path: &str, published: &[String]) -> bool {
    published.iter().any(|prefix| path.starts_with(prefix))
}

/// Whether `root` is the generated public projection rather than the
/// working tree.
fn is_projection(root: &Path) -> bool {
    root.join(PROJECTION_MARKER).exists()
}

/// What a claim's baseline evidence was recorded against, beyond what
/// scope names (#489). Recording a value here is what turns a declared
/// invalidation trigger — "bed change", "model or sidecar change" —
/// into something validation can check mechanically: each recorded
/// value is compared against the matched baseline report, and a value
/// that has moved downgrades the claim. A field left absent is not
/// checked; validation never guesses what a claim was measured on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedAgainst {
    /// The bed digest, exactly as the baseline records it
    /// (`blake3:…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bed: Option<String>,
    /// The model file name, as baselines record it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The sidecar version, as `llama-server --version` reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidecar: Option<String>,
}

/// What a claim asserts its baseline evidence found (#489). Evidence
/// that says the opposite of the sentence cannot go on supporting it:
/// the 11 August baseline swap flipped a verdict fail → pass under a
/// claim asserting the failure, and validation — which read only
/// provenance — kept it proven. Optional, like every content check:
/// a claim that states no expectation is not checked against one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expectation {
    /// The verdict the claim asserts, as the baseline records it
    /// (`pass` or `fail`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
}

/// One product-level assurance claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub id: String,
    /// The exact or canonical sentence. What a surface quotes.
    pub wording: String,
    /// The status as recorded — what was believed when this entry was
    /// written. Validation derives the effective status from evidence
    /// and never preserves this on trust.
    pub status: Status,
    pub scope: Scope,
    pub evidence: Vec<Evidence>,
    /// What the baseline evidence was recorded against — the values
    /// behind the declared invalidation triggers that validation can
    /// check. Absent fields are unchecked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recorded_against: Option<RecordedAgainst>,
    /// What the claim asserts the baseline evidence found. Absent
    /// fields are unchecked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expects: Option<Expectation>,
    /// When the entry was recorded (ISO date). Relative dates rot.
    pub recorded: chrono::NaiveDate,
    /// The changes that stop this claim applying. Mandatory for a
    /// proven claim: a claim that nothing could invalidate is not a
    /// measurement, it is a slogan.
    #[serde(default)]
    pub invalidation: Vec<String>,
    /// User-facing places that quote or depend on this claim, as
    /// repo-relative paths. Discoverability is the point: the day a
    /// claim fails, this is the list of copy to re-read.
    #[serde(default)]
    pub surfaces: Vec<String>,
    /// Who or what re-examines this claim — a command to re-run, an
    /// issue to reopen.
    pub review_route: String,
    /// Claims this one cannot hold beside. Declared, not inferred:
    /// wording is for people, and a validator guessing contradiction
    /// from prose would be a judge. Both halves effectively proven is
    /// a refusal.
    #[serde(default)]
    pub contradicts: Vec<String>,
    /// Why the status is what it is, when the wording alone cannot say
    /// — "the FAIL rested on the eval's own join (#457)". Prose for
    /// the reviewer; validation never reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The committed registry file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    /// The part of the tree that goes public (#478), as repo-relative
    /// path prefixes. Evidence outside it is real and inspectable *here*
    /// and unopenable by a reader of kttl.app, which is a different
    /// thing from unevidenced and must not be allowed to look the same.
    ///
    /// Absent means undeclared, and an undeclared boundary constrains
    /// nothing: the registry predates publication and must stay valid
    /// for anyone validating a tree that is not being published.
    #[serde(default)]
    pub published: Vec<String>,
    /// Where the published tree lives, once it does (#478). The
    /// boundary says *what* goes public; this says *where* it went, and
    /// the two are one sentence for the same reason the boundary is one
    /// list: a reader following a link and a job pushing the tree must
    /// mean the same repository.
    ///
    /// Absent until the flip, and absent means no addresses are
    /// emitted. A registry that predates publication stays valid, and a
    /// tree that is not published cites nothing it cannot open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    /// Where the run recordings the baselines rest on were published.
    ///
    /// A second repository because it is data rather than software —
    /// pushed by hand after a sitting, not generated on merge — and a
    /// second field here rather than a constant in a page, for the
    /// reason every address in this file is here: the projection does
    /// not carry `app/`, so an address written into a screen is one the
    /// public tree can neither render nor check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recordings_at: Option<String>,
    pub claims: Vec<Claim>,
}

/// One claim after validation: what the evidence supports today, and
/// why, in sentences that name versions rather than gesture at them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssessedClaim {
    pub id: String,
    /// The status as recorded, kept beside the effective one so a
    /// renderer can show "recorded proven, now unproven" rather than
    /// silently replacing history.
    pub recorded: Status,
    pub effective: Status,
    pub reasons: Vec<String>,
}

/// The whole registry after validation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Assessment {
    pub claims: Vec<AssessedClaim>,
    /// Structural problems that make the registry untrustworthy as a
    /// record. Any entry here should fail CI.
    pub refusals: Vec<String>,
    /// User-facing copy standing on a claim that is no longer
    /// effectively proven. Distinct from a refusal (the registry is
    /// fine) and from a quiet downgrade (nobody is being told): this is
    /// a sentence in front of people that its own evidence no longer
    /// supports, and it should fail whatever gate guards that copy.
    pub stale_copy: Vec<String>,
}

/// Check a registry document against the committed JSON Schema.
///
/// Serde already refuses what it cannot parse; the schema is the same
/// contract in a form a reviewer — or a future non-Rust consumer — can
/// read without the source. Holding the file to both keeps them from
/// drifting apart, with the test suite asserting the schema is no
/// looser than the parser.
pub fn conforms_to_schema(schema_text: &str, registry_text: &str) -> Result<(), String> {
    let schema: serde_json::Value = serde_json::from_str(schema_text)
        .map_err(|e| format!("The claims schema is not JSON: {e}"))?;
    let document: serde_json::Value = serde_json::from_str(registry_text)
        .map_err(|e| format!("The claims registry is not JSON: {e}"))?;
    let validator = jsonschema::validator_for(&schema)
        .map_err(|e| format!("The claims schema is not a valid JSON Schema: {e}"))?;
    let failures = validator
        .iter_errors(&document)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect::<Vec<_>>();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

impl Registry {
    /// Parse a registry. Unknown fields are refused by serde so the
    /// committed file cannot quietly grow vocabulary validation does
    /// not check.
    pub fn from_json(text: &str) -> Result<Registry, String> {
        serde_json::from_str(text)
            .map_err(|e| format!("Could not make sense of the claims registry: {e}"))
    }

    /// Re-derive what each claim is worth against the evidence on disk.
    ///
    /// `root` is the directory evidence paths are relative to — the
    /// repository root in production, a temp directory in tests.
    /// `current_scoring_version` is [`crate::eval::SCORING_VERSION`],
    /// passed in rather than read here so a test can exercise the
    /// staleness rules without waiting for a real bump. `today` comes
    /// from the edge for the same reason `--write-baseline`'s timestamp
    /// does: a date read mid-function is a thing no test can pin down.
    pub fn validate(
        &self,
        root: &Path,
        current_scoring_version: u32,
        today: chrono::NaiveDate,
    ) -> Assessment {
        let mut assessment = Assessment::default();
        let projection = is_projection(root);

        // Ids first: every later rule joins on them, so a registry
        // loose about ids cannot be validated, only guessed at.
        for (index, claim) in self.claims.iter().enumerate() {
            if self.claims[..index].iter().any(|seen| seen.id == claim.id) {
                assessment.refusals.push(format!(
                    "{}: this id names more than one claim; an id must name exactly once",
                    claim.id
                ));
            }
            for other_id in &claim.contradicts {
                if !self.claims.iter().any(|other| &other.id == other_id) {
                    assessment.refusals.push(format!(
                        "{}: contradicts {other_id}, which names no claim in the registry",
                        claim.id
                    ));
                }
            }
        }

        for claim in &self.claims {
            assessment.assess(
                claim,
                root,
                &self.published,
                projection,
                current_scoring_version,
                today,
            );
        }

        // Surfaces are checked on *effective* statuses, after every
        // claim has been assessed. A surface that does not exist is a
        // refusal — a claim cannot say "this copy depends on me" about
        // copy nobody can find. A surface that exists on a claim no
        // longer effectively proven is stale copy: the claim ageing was
        // green, the claim ageing while still quoted is not.
        for claim in &self.claims {
            let effective = assessment
                .claims
                .iter()
                .find(|assessed| assessed.id == claim.id)
                .map(|assessed| assessed.effective);
            for surface in &claim.surfaces {
                if !root.join(surface).exists() {
                    // The projection was never meant to carry the closed
                    // half, so its absence there says nothing about the
                    // copy (#478). Narrow on purpose: a path the boundary
                    // *does* publish is still missing, wherever we are.
                    if projection && !inside_boundary(surface, &self.published) {
                        continue;
                    }
                    assessment.refusals.push(format!(
                        "{}: surface {surface} does not exist — the copy this claim \
                         says depends on it cannot be found",
                        claim.id
                    ));
                } else if effective.is_some_and(|effective| effective != Status::Proven) {
                    assessment.stale_copy.push(format!(
                        "{surface} quotes {}, which is no longer proven — the copy is \
                         standing on evidence that lapsed",
                        claim.id
                    ));
                }
            }
        }

        // Contradiction is checked on *effective* statuses, after every
        // claim has been assessed: a proven claim beside a failed or
        // downgraded one is the registry doing its job, and only both
        // halves standing proven is it asserting nonsense.
        for claim in &self.claims {
            for other_id in &claim.contradicts {
                let holds = |id: &str| {
                    assessment
                        .claims
                        .iter()
                        .any(|assessed| assessed.id == id && assessed.effective == Status::Proven)
                };
                if holds(&claim.id) && holds(other_id) {
                    assessment.refusals.push(format!(
                        "{} and {} are declared contradictory and both stand proven — \
                         the registry cannot say which half is wrong, only that one is",
                        claim.id, other_id
                    ));
                }
            }
        }
        assessment
    }
}

impl Assessment {
    fn assess(
        &mut self,
        claim: &Claim,
        root: &Path,
        published: &[String],
        projection: bool,
        current_scoring_version: u32,
        today: chrono::NaiveDate,
    ) {
        let mut reasons = Vec::new();

        // Scope and invalidation are mandatory on a proven claim. This
        // is a refusal, not a downgrade: a claim that nothing could
        // invalidate is not a measurement, it is a slogan, and the
        // registry is broken as a record until someone says what would
        // stop the sentence applying.
        if claim.status == Status::Proven && claim.invalidation.is_empty() {
            self.refusals.push(format!(
                "{}: a proven claim must name its invalidation triggers — the changes \
                 that stop it applying. One that nothing could invalidate is not a \
                 measurement.",
                claim.id
            ));
        }

        // "Not evidenced" is a registry defect on a proven claim. An
        // issue reference is where a finding is discussed, not where it
        // is measured, so it cannot be the only thing under "proven" —
        // though it carries a failed claim perfectly well.
        if claim.status == Status::Proven && !claim.evidence.iter().any(Evidence::probative) {
            self.refusals.push(format!(
                "{}: a proven claim must reference inspectable evidence — a baseline, \
                 test or release check — not only discussion.",
                claim.id
            ));
        }

        // The same rule one step out (#478). Evidence a reader of
        // kttl.app cannot open is not evidence to them, and a proven
        // claim citing only that renders as an assertion with a
        // citation that dead-ends — the force-loss #477 was reopened
        // about. Checked only once a boundary is declared, so a tree
        // nobody is publishing is not held to a publication rule.
        if claim.status == Status::Proven
            && !published.is_empty()
            && claim.evidence.iter().any(Evidence::probative)
            && !claim
                .evidence
                .iter()
                .any(|evidence| evidence.probative() && evidence.publicly_reachable(published))
        {
            let unreachable: Vec<&str> = claim
                .evidence
                .iter()
                .filter(|evidence| evidence.probative())
                .filter_map(Evidence::path)
                .collect();
            self.refusals.push(format!(
                "{}: a proven claim must keep evidence the public can open, and {} \
                 {} outside the published tree. Cite something inside it, or the \
                 claims page states this as fact over a citation nobody can follow.",
                claim.id,
                unreachable.join(", "),
                if unreachable.len() == 1 { "is" } else { "are" },
            ));
        }

        for evidence in &claim.evidence {
            match evidence {
                Evidence::Baseline { path } => check_baseline(
                    claim,
                    root,
                    path,
                    current_scoring_version,
                    &mut reasons,
                    &mut self.refusals,
                ),
                // An issue number cannot be checked from here and does
                // not need to be: it never proves, so nothing hangs on
                // its contents.
                Evidence::Issue { .. } => {}
                Evidence::Test { path, name } => match std::fs::read_to_string(root.join(path)) {
                    Ok(source) if source.contains(name.as_str()) => {}
                    Ok(_) => self.refusals.push(format!(
                        "{}: the owning test {name} is not in {path} — renamed or \
                             deleted, either way the claim now points at nothing",
                        claim.id
                    )),
                    // A test in the closed half is unreadable in the
                    // projection by construction (#478), and the claim
                    // keeps its status: the measurement stands, and #516
                    // already guarantees a proven claim cites something
                    // the projection carries, which is checked here as
                    // strictly as anywhere.
                    Err(_) if projection && !inside_boundary(path, published) => {}
                    Err(e) => self.refusals.push(format!(
                        "{}: the owning test file {path} could not be read: {e}",
                        claim.id
                    )),
                },
                Evidence::Manual {
                    description,
                    expires,
                    ..
                } => {
                    if *expires <= today {
                        reasons.push(format!(
                            "the release check \"{description}\" expired on {expires} \
                             and has not been repeated",
                        ));
                    }
                }
            }
        }

        // A downgrade only ever takes support away from "proven":
        // unproven cannot be promoted by validation (only a person
        // recording new evidence does that), and failed is a finding
        // that stale evidence does not un-find.
        let effective = match claim.status {
            Status::Proven if !reasons.is_empty() => Status::Unproven,
            recorded => recorded,
        };

        self.claims.push(AssessedClaim {
            id: claim.id.clone(),
            recorded: claim.status,
            effective,
            reasons,
        });
    }
}

/// What validation needs from a baseline file, parsed leniently.
///
/// Deliberately not [`crate::eval::EvalReport`]: a baseline recorded by
/// an older harness must stay readable enough to be *refused with a
/// sentence*, and full deserialisation would make old evidence
/// unparseable the day a report field changes shape. Provenance fields
/// only — plus, since #489, the few per-report values the content
/// checks compare (bed digest, sidecar version, verdict), every one
/// optional so an old baseline that lacks them still parses.
#[derive(Debug, Deserialize)]
struct BaselineProvenance {
    scoring_version: Option<u32>,
    #[serde(default)]
    reports: Vec<ReportProvenance>,
}

#[derive(Debug, Deserialize)]
struct ReportProvenance {
    pack: Option<String>,
    pack_version: Option<String>,
    eval_set: Option<String>,
    model: Option<ModelProvenance>,
    bed: Option<String>,
    sidecar: Option<SidecarProvenance>,
    verdict: Option<String>,
    #[serde(default)]
    fixtures: Vec<FixtureProvenance>,
}

#[derive(Debug, Deserialize)]
struct FixtureProvenance {
    fixture: Option<String>,
    stability: Option<StabilityProvenance>,
}

/// The spreads `--runs` recorded, read here rather than reused from
/// [`crate::eval::Stability`] for the same reason every other struct in
/// this file is its own: validation deserialises the fields it checks
/// and tolerates a baseline carrying anything else, so an unrelated
/// addition to the report shape cannot start refusing the registry.
#[derive(Debug, Deserialize)]
struct StabilityProvenance {
    #[serde(default)]
    steps: BTreeMap<String, SpreadProvenance>,
    end_to_end: Option<SpreadProvenance>,
    needs_review_rate: Option<SpreadProvenance>,
    /// More than one digest is repeats that recorded different things,
    /// whatever the spreads say — the total form of the question
    /// (#533). Read here for the same reason it exists there: the
    /// spreads do not cover `items`, and the harm ceiling is computed
    /// from them.
    #[serde(default)]
    record_digests: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SpreadProvenance {
    low: f32,
    high: f32,
}

impl SpreadProvenance {
    fn moved(&self) -> bool {
        self.low != self.high
    }
}

impl StabilityProvenance {
    /// What disagreed, named. `Stability::moved` answers the same
    /// question as a bool; a downgrade has to say which measurement
    /// moved, because the reason is the whole point of not refusing.
    fn moved(&self) -> Vec<String> {
        let mut moved = Vec::new();
        if self.record_digests.len() > 1 {
            moved.push(format!(
                "{} repeats recorded different results",
                self.record_digests.len()
            ));
        }
        for (step, spread) in &self.steps {
            if spread.moved() {
                moved.push(format!("{step} {} to {}", spread.low, spread.high));
            }
        }
        for (label, spread) in [
            ("end to end", &self.end_to_end),
            ("review rate", &self.needs_review_rate),
        ] {
            if let Some(spread) = spread.as_ref().filter(|spread| spread.moved()) {
                moved.push(format!("{label} {} to {}", spread.low, spread.high));
            }
        }
        moved
    }
}

#[derive(Debug, Deserialize)]
struct ModelProvenance {
    file: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SidecarProvenance {
    version: Option<String>,
}

fn check_baseline(
    claim: &Claim,
    root: &Path,
    path: &str,
    current_scoring_version: u32,
    reasons: &mut Vec<String>,
    refusals: &mut Vec<String>,
) {
    let claim_id = &claim.id;
    let scope = &claim.scope;
    let full = root.join(path);
    let text = match std::fs::read_to_string(&full) {
        Ok(text) => text,
        Err(e) => {
            // Evidence that does not exist is a broken registry, not a
            // stale claim: nothing can be re-measured against a path
            // that points at nothing.
            refusals.push(format!(
                "{claim_id}: baseline evidence {path} could not be read: {e}"
            ));
            return;
        }
    };
    let baseline: BaselineProvenance = match serde_json::from_str(&text) {
        Ok(baseline) => baseline,
        Err(e) => {
            refusals.push(format!(
                "{claim_id}: baseline evidence {path} is not a baseline file: {e}"
            ));
            return;
        }
    };

    match baseline.scoring_version {
        Some(version) if version == current_scoring_version => {}
        Some(version) => reasons.push(format!(
            "{path} was recorded at scoring version {version} and the harness now \
             scores at version {current_scoring_version}, so its numbers no longer \
             mean what the claim asserts",
        )),
        None => reasons.push(format!(
            "{path} was recorded before scoring was versioned, so nothing says what \
             its numbers mean",
        )),
    }

    // The baseline must contain a measurement of the thing the claim is
    // scoped to. A near-miss names what was found, so the reader sees
    // what to re-measure rather than a bare "no".
    let matched = baseline.reports.iter().find(|report| {
        scope_field_matches(&scope.pack, &report.pack)
            && scope_field_matches(&scope.pack_version, &report.pack_version)
            && scope_field_matches(&scope.eval_set, &report.eval_set)
            && scope_field_matches(
                &scope.model,
                &report.model.as_ref().and_then(|model| model.file.clone()),
            )
    });
    let Some(report) = matched else {
        let measured = baseline
            .reports
            .iter()
            .map(describe_report)
            .collect::<Vec<_>>()
            .join("; ");
        reasons.push(format!(
            "{path} does not measure what the claim is scoped to ({}) — it records {}",
            describe_scope(scope),
            if measured.is_empty() {
                "no reports at all".to_owned()
            } else {
                measured
            },
        ));
        return;
    };

    // The content checks (#489): the declared invalidation triggers a
    // claim has recorded values for, compared against the matched
    // report. Every mismatch is a downgrade, never a refusal — the
    // registry is fine, the evidence was re-recorded on something else,
    // and the claim needs a person to re-read it against what the
    // baseline now says.
    if let Some(against) = &claim.recorded_against {
        let mut compare = |label: &str, wanted: &Option<String>, found: Option<&String>| {
            let Some(wanted) = wanted else { return };
            match found {
                Some(found) if found == wanted => {}
                Some(found) => reasons.push(format!(
                    "{path} now records {label} {found} where this claim was recorded \
                     against {wanted} — the {label} moved, so the measurement under the \
                     sentence is not the one it was written on",
                )),
                None => reasons.push(format!(
                    "{path} records no {label}, so the {wanted} this claim was recorded \
                     against cannot be checked",
                )),
            }
        };
        compare("bed", &against.bed, report.bed.as_ref());
        compare(
            "model",
            &against.model,
            report.model.as_ref().and_then(|model| model.file.as_ref()),
        );
        compare(
            "sidecar",
            &against.sidecar,
            report
                .sidecar
                .as_ref()
                .and_then(|sidecar| sidecar.version.as_ref()),
        );
    }

    // Repeats that disagreed (#533). `--runs` exists to confirm
    // stability, not to improve an estimate: at temperature 0 under a
    // grammar nothing should move, so a spread is a fault to chase and
    // a claim cannot stand `proven` on a run set that disagreed with
    // itself. The asymmetry is deliberate and load-bearing — absent
    // stability is a `--runs 1` measurement and stays valid, because
    // requiring repeats everywhere would make every cheap measurement
    // unusable as evidence. What is refused is *measured and moved*,
    // never *unmeasured*.
    for fixture in &report.fixtures {
        let Some(stability) = &fixture.stability else {
            continue;
        };
        let moved = stability.moved();
        if moved.is_empty() {
            continue;
        }
        let name = fixture.fixture.as_deref().unwrap_or("an unnamed fixture");
        reasons.push(format!(
            "{path} records {name} moving across repeats ({}) — a claim cannot stand on \
             a run set that disagreed with itself",
            moved.join(", "),
        ));
    }

    if let Some(expected) = claim.expects.as_ref().and_then(|e| e.verdict.as_ref()) {
        match &report.verdict {
            Some(found) if found == expected => {}
            Some(found) => reasons.push(format!(
                "{path} records verdict {found} where the claim asserts {expected} — \
                 the evidence now contradicts the sentence it is cited under",
            )),
            None => reasons.push(format!(
                "{path} records no verdict, so it cannot support the {expected} the \
                 claim asserts",
            )),
        }
    }
}

/// A scope field constrains only when declared.
fn scope_field_matches(wanted: &Option<String>, recorded: &Option<String>) -> bool {
    match wanted {
        None => true,
        Some(wanted) => recorded.as_deref() == Some(wanted.as_str()),
    }
}

fn describe_scope(scope: &Scope) -> String {
    let mut parts = Vec::new();
    if let Some(pack) = &scope.pack {
        match &scope.pack_version {
            Some(version) => parts.push(format!("{pack} {version}")),
            None => parts.push(pack.clone()),
        }
    }
    if let Some(model) = &scope.model {
        parts.push(model.clone());
    }
    if let Some(set) = &scope.eval_set {
        parts.push(format!("{set} set"));
    }
    if parts.is_empty() {
        "any measurement".to_owned()
    } else {
        parts.join(", ")
    }
}

fn describe_report(report: &ReportProvenance) -> String {
    let pack = match (&report.pack, &report.pack_version) {
        (Some(pack), Some(version)) => format!("{pack} {version}"),
        (Some(pack), None) => pack.clone(),
        _ => "an unnamed pack".to_owned(),
    };
    let model = report
        .model
        .as_ref()
        .and_then(|model| model.file.clone())
        .unwrap_or_else(|| "no model named".to_owned());
    let set = report
        .eval_set
        .clone()
        .unwrap_or_else(|| "unknown".to_owned());
    format!("{pack}, {model}, {set} set")
}
