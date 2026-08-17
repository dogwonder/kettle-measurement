//! #427: pack-declared relations between fixtures, runner-judged.
//!
//! A gold fixture asks whether one answer is right; a relation asks
//! whether two results relate the way the meaning of their inputs
//! says they must. Invariance: the same meaning must produce the same
//! projection. Algebraic: a declared transformation of the inputs
//! produces a declared transformation of the outputs — swapping a
//! comparison's roles inverts its changes.
//!
//! The pack owns the declarations (`fixtures/relations.json`); the
//! runner owns the vocabulary and the judging. Relations join on
//! stable fixture and item identities, never output order, and a
//! declaration the runner cannot interpret is refused rather than
//! skipped — a silently unjudged relation would read as a held one.

use super::{ClassificationOutcome, Extracted, ExtractionOutcome, FixtureResult, ScoredDecision};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The committed file beside a pack's fixtures.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationsFile {
    pub relations: Vec<RelationDeclaration>,
}

/// One declared relationship between two fixtures' results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationDeclaration {
    /// Authored, kebab-case, pack-unique — the #237 identity rules.
    pub id: String,
    pub kind: RelationKind,
    /// `fixture_id`s, never file names: identity the baseline joins on.
    pub left: String,
    pub right: String,
}

/// The runner-owned relation vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// The two fixtures mean the same thing; the projection must match.
    Invariance { projection: Projection },
    /// One authored edit separates the two fixtures, and the judged
    /// outputs must differ in exactly the declared projection of that
    /// edit — everything else invariant (#465, the third of #427's
    /// three kinds). Invariance cannot express it (the outputs are
    /// meant to differ) and the inversion law cannot either (nothing
    /// swaps roles). It fails in both directions: an undeclared
    /// movement is an over-reaction, and a declared movement that
    /// never appears is the under-reaction no per-item gate and no
    /// recorded-answer mutation can see, because no recording contains
    /// the changed document.
    ControlledChange {
        projection: Projection,
        /// What the authored edit was, in words a person reads beside
        /// the failure.
        edit: String,
        /// The projection entries only the left result may assert,
        /// grouped exactly as the projection groups them (the empty
        /// group for single-document packs). Declared verbatim so the
        /// judge asserts the difference rather than rediscovering it.
        only_left: BTreeMap<String, BTreeSet<String>>,
        /// The projection entries only the right result may assert.
        only_right: BTreeMap<String, BTreeSet<String>>,
    },
    /// The outputs differ in a declared direction.
    Algebraic(AlgebraicLaw),
}

/// Which slice of a result a relation compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Projection {
    /// Every asserted obligation, as a set.
    ObligationsSet,
    /// Every asserted obligation with the anchor it was read against.
    /// Per-item scoring compares anchors by the date they name (#287,
    /// #452), so among dateless anchors "the invoice date" and "the
    /// date of this letter" score identically — 660 of 890 dateless
    /// anchors in the letter bed. #292's rule was to put the wording
    /// back the day it becomes visible; a contrastive pair over anchor
    /// wording is that day, and this projection is what it compares.
    /// A new projection rather than a change to [`ObligationsSet`],
    /// because widening the entry there would silently change what
    /// every declared invariance relation means.
    ObligationsWithAnchors,
    /// Every asserted term, grouped by the role it was read from.
    TermsByRole,
    /// Every asserted classification, grouped by the merchant it was
    /// made about — the scored decision key (#310), so two descriptor
    /// forms of one merchant are one entry, exactly as they are one
    /// decision.
    ClassificationsByMerchant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlgebraicLaw {
    /// Swapping a comparison's two roles inverts its term changes:
    /// each role's readings become the other's.
    TermChangeInversion,
    /// Removing one merchant's rows from a statement removes exactly
    /// that merchant's classification: every survivor keeps its class,
    /// nothing new appears, and exactly one merchant is gone.
    MerchantRemoval,
}

/// One judged relation, kept beside the per-item records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationResult {
    pub id: String,
    pub kind: RelationKind,
    pub left: String,
    pub right: String,
    #[serde(flatten)]
    pub outcome: RelationOutcome,
}

impl RelationResult {
    pub fn held(&self) -> bool {
        matches!(self.outcome, RelationOutcome::Held)
    }
}

/// Held, failed with the smallest differing projection named, or
/// unjudgeable — review-routed inputs make a relation neither held nor
/// failed, exactly as a review-routed decision is neither a hit nor a
/// miss. The honest floor hits this immediately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum RelationOutcome {
    Held,
    Failed { smallest_diff: String },
    Unjudgeable { because: String },
}

/// Judge every declared relation against the scored fixtures. A
/// declaration naming a fixture this selection does not carry is an
/// error — a bed that cannot state its declared relationships is a
/// different bed from the one the declarations were written against.
pub fn judge(
    declarations: &[RelationDeclaration],
    fixtures: &[FixtureResult],
) -> Result<Vec<RelationResult>, String> {
    let by_id: BTreeMap<&str, &FixtureResult> = fixtures
        .iter()
        .map(|fixture| (fixture.fixture_id(), fixture))
        .collect();
    declarations
        .iter()
        .map(|declaration| {
            let left = by_id.get(declaration.left.as_str()).ok_or_else(|| {
                format!(
                    "relation '{}' names fixture '{}', which this bed does not carry",
                    declaration.id, declaration.left
                )
            })?;
            let right = by_id.get(declaration.right.as_str()).ok_or_else(|| {
                format!(
                    "relation '{}' names fixture '{}', which this bed does not carry",
                    declaration.id, declaration.right
                )
            })?;
            Ok(RelationResult {
                id: declaration.id.clone(),
                kind: declaration.kind.clone(),
                left: declaration.left.clone(),
                right: declaration.right.clone(),
                outcome: judge_one(&declaration.kind, left, right),
            })
        })
        .collect()
}

fn judge_one(kind: &RelationKind, left: &FixtureResult, right: &FixtureResult) -> RelationOutcome {
    if let Some(because) = review_routed(left).or_else(|| review_routed(right)) {
        return RelationOutcome::Unjudgeable { because };
    }
    match kind {
        RelationKind::Invariance { projection } => {
            let left_view = project(*projection, left);
            let right_view = project(*projection, right);
            difference(&left_view, &right_view)
                .map(|smallest_diff| RelationOutcome::Failed { smallest_diff })
                .unwrap_or(RelationOutcome::Held)
        }
        RelationKind::ControlledChange {
            projection,
            only_left,
            only_right,
            ..
        } => {
            let left_view = project(*projection, left);
            let right_view = project(*projection, right);
            let moved_left = one_sided(&left_view, &right_view);
            let moved_right = one_sided(&right_view, &left_view);
            controlled_diff(only_left, &moved_left, "left")
                .or_else(|| controlled_diff(only_right, &moved_right, "right"))
                .map(|smallest_diff| RelationOutcome::Failed { smallest_diff })
                .unwrap_or(RelationOutcome::Held)
        }
        RelationKind::Algebraic(AlgebraicLaw::TermChangeInversion) => {
            let left_view = project(Projection::TermsByRole, left);
            let right_view = project(Projection::TermsByRole, right);
            let roles: Vec<&String> = left_view.keys().collect();
            if roles.len() != 2 || right_view.len() != 2 {
                return RelationOutcome::Unjudgeable {
                    because: format!(
                        "an inversion needs exactly two roles on each side, found {} and {}",
                        left_view.len(),
                        right_view.len()
                    ),
                };
            }
            let (first, second) = (roles[0], roles[1]);
            let swapped_matches = left_view.get(first) == right_view.get(second)
                && left_view.get(second) == right_view.get(first);
            if swapped_matches {
                RelationOutcome::Held
            } else {
                let diff = difference(
                    &BTreeMap::from([(first.clone(), left_view[first].clone())]),
                    &BTreeMap::from([(first.clone(), right_view[second].clone())]),
                )
                .or_else(|| {
                    difference(
                        &BTreeMap::from([(second.clone(), left_view[second].clone())]),
                        &BTreeMap::from([(second.clone(), right_view[first].clone())]),
                    )
                })
                .unwrap_or_else(|| "the two sides disagree".to_owned());
                RelationOutcome::Failed {
                    smallest_diff: diff,
                }
            }
        }
        RelationKind::Algebraic(AlgebraicLaw::MerchantRemoval) => {
            let left_view = project(Projection::ClassificationsByMerchant, left);
            let right_view = project(Projection::ClassificationsByMerchant, right);
            // Nothing may appear, and every survivor keeps its class.
            for (merchant, classes) in &right_view {
                let Some(left_classes) = left_view.get(merchant) else {
                    return RelationOutcome::Failed {
                        smallest_diff: format!(
                            "only the right side asserts {merchant}: {}",
                            classes
                                .iter()
                                .next()
                                .map(String::as_str)
                                .unwrap_or("nothing")
                        ),
                    };
                };
                if left_classes != classes {
                    let moved = classes
                        .symmetric_difference(left_classes)
                        .next()
                        .map(String::as_str)
                        .unwrap_or("nothing");
                    return RelationOutcome::Failed {
                        smallest_diff: format!("the two sides disagree about {merchant}: {moved}"),
                    };
                }
            }
            // And exactly one merchant is gone.
            let removed: Vec<&String> = left_view
                .keys()
                .filter(|merchant| !right_view.contains_key(*merchant))
                .collect();
            match removed.as_slice() {
                [_] => RelationOutcome::Held,
                [] => RelationOutcome::Failed {
                    smallest_diff: "a removal must drop exactly one merchant; none is gone"
                        .to_owned(),
                },
                many => RelationOutcome::Failed {
                    smallest_diff: format!(
                        "a removal must drop exactly one merchant; {} are gone (first: {})",
                        many.len(),
                        many[0]
                    ),
                },
            }
        }
    }
}

/// The entries one side asserts and the other does not, by group —
/// groups whose one-sided set is empty are dropped, so a declaration
/// need not name groups where nothing moved.
fn one_sided(
    view: &BTreeMap<String, BTreeSet<String>>,
    other: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let empty = BTreeSet::new();
    view.iter()
        .filter_map(|(group, entries)| {
            let only: BTreeSet<String> = entries
                .difference(other.get(group).unwrap_or(&empty))
                .cloned()
                .collect();
            (!only.is_empty()).then(|| (group.clone(), only))
        })
        .collect()
}

/// The smallest disagreement between a controlled change's declared
/// one-sided entries and the movement that actually happened, named
/// with its direction: a declared movement that never appeared is the
/// under-reaction, an undeclared one the over-reaction.
fn controlled_diff(
    declared: &BTreeMap<String, BTreeSet<String>>,
    moved: &BTreeMap<String, BTreeSet<String>>,
    side: &str,
) -> Option<String> {
    let empty = BTreeSet::new();
    let groups: BTreeSet<&String> = declared.keys().chain(moved.keys()).collect();
    for group in groups {
        let declared_set = declared.get(group).unwrap_or(&empty);
        let moved_set = moved.get(group).unwrap_or(&empty);
        if let Some(missing) = declared_set.difference(moved_set).next() {
            return Some(format!(
                "the declared change never appeared on the {side} side: {group}: {missing}"
            ));
        }
        if let Some(extra) = moved_set.difference(declared_set).next() {
            return Some(format!(
                "the {side} side moved beyond the declared change: {group}: {extra}"
            ));
        }
    }
    None
}

/// A review-routed decision was surfaced to a person, not asserted, so
/// nothing about the assertion relationship can be judged from it —
/// whichever metric the decision carries.
fn review_routed(fixture: &FixtureResult) -> Option<String> {
    fixture.items.iter().find_map(|item| {
        matches!(
            &item.decision,
            ScoredDecision::Extraction {
                actual: ExtractionOutcome::NeedsReview { .. },
                ..
            } | ScoredDecision::Classification {
                actual: ClassificationOutcome::NeedsReview { .. },
                ..
            }
        )
        .then(|| {
            format!(
                "{} was routed to a person, so the relation has no assertion to judge",
                item.id
            )
        })
    })
}

/// The compared slice: role (or the empty role for single-document
/// packs) to the set of assertions read from it. Built from scored
/// items — the same records a baseline carries — so relations judge
/// replayed and recorded results alike.
fn project(projection: Projection, fixture: &FixtureResult) -> BTreeMap<String, BTreeSet<String>> {
    let mut view: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for item in &fixture.items {
        // A classification's group is the merchant the decision was
        // about — the decision key (#310) — and its entry is what was
        // asserted of that merchant.
        if let (
            Projection::ClassificationsByMerchant,
            ScoredDecision::Classification {
                actual: ClassificationOutcome::Classified { classification },
                ..
            },
        ) = (projection, &item.decision)
        {
            view.entry(item.decision_key.clone())
                .or_default()
                .insert(format!(
                    "{}/{}",
                    classification.kind, classification.category
                ));
            continue;
        }
        let ScoredDecision::Extraction {
            actual: ExtractionOutcome::Found { extracted },
            ..
        } = &item.decision
        else {
            continue;
        };
        match (projection, extracted) {
            (Projection::TermsByRole, Extracted::Term(term)) => {
                // A comparison item's decision key is "role|passage"
                // (#310) — the role half is the projection's group.
                let role = item
                    .decision_key
                    .split_once('|')
                    .map(|(role, _)| role)
                    .unwrap_or_default();
                view.entry(role.to_owned())
                    .or_default()
                    .insert(format!("{}/{}/{}", term.term, term.basis, term.value));
            }
            (Projection::ObligationsSet, Extracted::Obligation(obligation)) => {
                view.entry(String::new())
                    .or_default()
                    .insert(obligation_entry(
                        &obligation.kind,
                        &obligation.party,
                        &obligation.deadline,
                        obligation.due.map(|due| due.to_string()).as_deref(),
                    ));
            }
            (Projection::ObligationsWithAnchors, Extracted::Obligation(obligation)) => {
                view.entry(String::new())
                    .or_default()
                    .insert(obligation_anchor_entry(
                        &obligation.kind,
                        &obligation.party,
                        &obligation.deadline,
                        &obligation.anchor,
                        obligation.due.map(|due| due.to_string()).as_deref(),
                    ));
            }
            _ => {}
        }
    }
    view
}

/// An [`Projection::ObligationsSet`] entry, in the one format the judge
/// compares. Public because the letter bed's controlled-change pass
/// declares these entries verbatim, and a second formatter would be a
/// second place for the two to disagree.
pub fn obligation_entry(kind: &str, party: &str, deadline: &str, due: Option<&str>) -> String {
    format!("{kind}/{party}/{deadline}/{}", due.unwrap_or("undated"))
}

/// An [`Projection::ObligationsWithAnchors`] entry — the same entry
/// with the anchor the obligation was read against.
pub fn obligation_anchor_entry(
    kind: &str,
    party: &str,
    deadline: &str,
    anchor: &str,
    due: Option<&str>,
) -> String {
    format!(
        "{kind}/{party}/{deadline}/{anchor}/{}",
        due.unwrap_or("undated")
    )
}

/// The smallest element present on one side and not the other, named
/// with its side — the failure a person reads first.
fn difference(
    left: &BTreeMap<String, BTreeSet<String>>,
    right: &BTreeMap<String, BTreeSet<String>>,
) -> Option<String> {
    let roles: BTreeSet<&String> = left.keys().chain(right.keys()).collect();
    for role in roles {
        let empty = BTreeSet::new();
        let left_set = left.get(role).unwrap_or(&empty);
        let right_set = right.get(role).unwrap_or(&empty);
        if let Some(only_left) = left_set.difference(right_set).next() {
            return Some(format!("only the left side asserts {role}: {only_left}"));
        }
        if let Some(only_right) = right_set.difference(left_set).next() {
            return Some(format!("only the right side asserts {role}: {only_right}"));
        }
    }
    None
}
