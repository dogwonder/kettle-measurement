//! Whether a passage requires something of its reader, or merely
//! offers it (#406).
//!
//! *"You must also confirm in writing"* obliges a person. *"You **may**
//! also confirm in writing"* does not, and a run that reports the
//! second as an action has invented work for somebody. The v14 letter
//! run did exactly that, at high confidence, on the
//! `controlled-must-to-may` twin whose authored edit is that one word.
//!
//! This is the #258 shape: **the model labels, Rust decides.** A modal
//! verb is a checkable property of the passage, not a judgement about
//! it, so the check belongs here rather than in a prompt — and unlike a
//! prompt edit it can be verified by replaying answers already
//! recorded.
//!
//! The rule is deliberately timid in one direction only. It fires when
//! a passage grants permission to its reader and nothing beside it
//! requires anything; where both appear — *"You may pay by card, but
//! the balance must reach us by 3 March"* — it stands aside, because
//! the passage does require something and only a reading could say
//! what. Firing routes the claim to a person rather than dropping it,
//! so the cost of firing wrongly is review rate, which the pack
//! declares as a cost, and never a missed obligation, which no ceiling
//! forgives.
//!
//! **What the bed can say about it.** Three passages across the 826
//! committed letter fixtures carry a permission modal with nothing
//! requiring beside them, and two are the must-to-may twins this was
//! written for. So the rule is measured on the defect and hardly
//! exposed elsewhere: the argument for it is the asymmetry above, not
//! evidence of safety at scale. Growing that exposure is bed work, and
//! it is the condition for ever making this stricter than review.

/// Permission granted to the reader — the constructions a British
/// letter offers something in.
///
/// Addressed to the reader on purpose: *"we may write to you again"*
/// grants the sender something and says nothing about what the reader
/// owes, so it must not disarm a requirement elsewhere in the passage.
/// The trailing spaces matter: *"you cannot appeal after 3 March"* is a
/// prohibition, not an offer, and `"you can "` declines to match it.
const PERMISSION: [&str; 7] = [
    "you may ",
    "you can ",
    "if you wish",
    "you are welcome to",
    "you are free to",
    "at your option",
    "optionally",
];

/// Anything that requires. One of these in the passage and the rule
/// stands aside — an over-broad list here makes the rule quieter,
/// which is the safe direction: a passage wrongly left alone is scored
/// exactly as it was before this existed.
const REQUIREMENT: [&str; 13] = [
    "must",
    "shall",
    // Covers *required* and *requirement* too, which is why neither is
    // listed: a shorter list that matches more is the safe direction.
    "require",
    "need to",
    "should",
    "please",
    "is due",
    "are due",
    "no later than",
    "we ask",
    "to be returned",
    "failure to",
    "obliged to",
];

/// Does this passage offer something to its reader and require nothing?
///
/// Case-insensitive substring matching, which is enough for the job and
/// honest about being no more than that: this is a modal-verb check,
/// not a parser, and a passage it cannot read is one it leaves alone.
pub fn grants_without_requiring(passage: &str) -> bool {
    let lowered = passage.to_lowercase();
    PERMISSION.iter().any(|marker| lowered.contains(marker))
        && !REQUIREMENT.iter().any(|marker| lowered.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::grants_without_requiring;

    #[test]
    fn a_permission_granted_to_the_reader_requires_nothing() {
        assert!(grants_without_requiring(
            "You may also confirm in writing that you have made this payment, \
             within 28 days of the date of this letter."
        ));
        assert!(grants_without_requiring(
            "You can return the form in the envelope enclosed."
        ));
        assert!(grants_without_requiring(
            "Keep this letter for your records if you wish."
        ));
    }

    #[test]
    fn the_requirement_the_edit_replaced_is_not_a_permission() {
        assert!(!grants_without_requiring(
            "You must also confirm in writing that you have made this payment, \
             within 28 days of the date of this letter."
        ));
        assert!(!grants_without_requiring(
            "Payment must be received within 14 days."
        ));
        assert!(!grants_without_requiring(
            "Please pay £120.00 within 14 days of the date of this letter."
        ));
    }

    #[test]
    fn a_passage_that_offers_and_requires_is_left_alone() {
        // Both present, so what the passage asks for is a reading, and
        // the rule has no business making it. It stands aside and the
        // claim is scored exactly as it was before this check existed.
        assert!(!grants_without_requiring(
            "You may pay by card or cheque, but the balance must reach us by \
             3 March 2026."
        ));
    }

    #[test]
    fn permission_the_sender_takes_is_not_permission_the_reader_gets() {
        // *We* may write again; that grants the reader nothing, and a
        // passage saying only this obliges nobody anyway. What matters
        // is that it cannot disarm a requirement standing beside it.
        assert!(!grants_without_requiring(
            "We may write to you again about this account."
        ));
    }
}
