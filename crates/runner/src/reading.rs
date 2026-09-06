//! One reading shape, one verifier (`app/METHOD.md` §3; #625; review
//! of #626, Task 4).
//!
//! Every value the model reads off the page is a [`Reading`]: the
//! passage id it lives in and the value verbatim. [`check`] is the one
//! path from a raw reading to a reportable value, in this order:
//!
//! 1. an empty value is **absence** — a defined convention, checked
//!    against nothing, suppressed by every renderer;
//! 2. `at` is one of the ids in the *request* the model answered
//!    (`exec::StepOutcome::shown`, #624) — else **refused**;
//! 3. `at` indexes a passage of the claim's own document — else
//!    **refused**; a wrong-document id is a contradiction, not
//!    "nothing named";
//! 4. `value` is in that passage, whitespace-squashed (#460 rule one)
//!    — else **refused**. A sum must be a whole money token there:
//!    `£360` is not in `£360.00`, because a truncated figure matching
//!    the front of a larger one is exactly the wrong sum;
//! 5. `value` parses as the kind the field declares — else the words
//!    are **kept** and nothing is derived from them (outcome 2);
//! 6. the passage holds more than one sum, or the value is in more
//!    than one passage of the document — a **warning** only.
//!
//! A refused reading is refused as a reading, never as an obligation:
//! an ask whose amount is refused is an ask with no amount, shown with
//! the sentence it was read from. Rust never repairs a value, never
//! case-folds, never searches another passage to make a copy match.

use crate::document::Segment;
use std::collections::BTreeSet;

/// The passage id a value lives in, and the value verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Reading {
    pub at: usize,
    pub value: String,
}

impl Reading {
    pub fn new(at: usize, value: impl Into<String>) -> Self {
        Reading {
            at,
            value: value.into(),
        }
    }

    /// The defined absence: no value, at the obligation's own passage.
    pub fn absent(at: usize) -> Self {
        Reading {
            at,
            value: String::new(),
        }
    }

    pub fn is_absent(&self) -> bool {
        self.value.trim().is_empty()
    }
}

/// What a field's value must parse as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A non-empty name.
    Name,
    /// A sum of money: a whole money token in the passage.
    Money,
    /// The letter's words for when; parsed elsewhere, if at all.
    Phrase,
}

/// Why a reading was refused: the page contradicted it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// `at` was not in the request the model answered.
    NotShown { at: usize, shown: BTreeSet<usize> },
    /// `at` indexes nothing, or a passage of another document.
    NotThisDocument { at: usize },
    /// The value is not in the passage at `at`.
    NotInPassage { at: usize },
}

impl Refusal {
    pub fn detail(&self, field: &str) -> String {
        match self {
            Refusal::NotShown { at, shown } => {
                let shown = shown
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{field} names passage {at}, not one shown in the request answered ({shown}): \
                     the model never saw it, so the reading is refused"
                )
            }
            Refusal::NotThisDocument { at } => format!(
                "{field} names passage {at}, which is not a passage of this document: refused"
            ),
            Refusal::NotInPassage { at } => {
                format!("{field}'s value is not in passage {at}: refused")
            }
        }
    }
}

/// A warning: true of the document, never of the claim (#460 rule two).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// The passage prints more than one sum.
    SeveralSumsInPassage { at: usize, sums: usize },
    /// The value is verbatim in more than one passage of the document.
    ValueInSeveralPassages { passages: usize },
}

impl Warning {
    pub fn detail(&self, field: &str) -> String {
        match self {
            Warning::SeveralSumsInPassage { at, sums } => {
                format!("passage {at} prints {sums} sums; {field} copied one of them")
            }
            Warning::ValueInSeveralPassages { passages } => {
                format!("{field}'s value is in {passages} passages of this document")
            }
        }
    }
}

/// The verifier's answer for one reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checked {
    /// No value was read. Nothing to check, nothing to show.
    Absent,
    /// The page vouches for it.
    Supported {
        /// The passage at `at`.
        passage: Segment,
        /// Whether the value parses as the field's kind. `false` is
        /// outcome 2: the words are kept and nothing is derived.
        parses: bool,
        warnings: Vec<Warning>,
    },
    /// The page contradicts it.
    Refused(Refusal),
}

/// Where the value must be found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Site {
    /// In the passage at `at` — the ordinary reading.
    Named,
    /// In the obligation's own passage, with `at` a location claim
    /// checked for itself (shown, this document). The deadline's
    /// interim shape (review of #626, Task 4): its value is still the
    /// letter's phrase for when, copied from the ask's own passage,
    /// while `at` says where the date is printed — the ask's passage,
    /// or a due-date row it points at. Task 5 makes a pointing
    /// deadline's value the printed date, and this variant goes.
    Own,
}

/// Check one reading against the page.
///
/// `own` is the passage the obligation was read from; `shown` the ids
/// of the request the model answered from; `segments` every passage of
/// the run, indexed by batch id.
pub fn check(
    reading: &Reading,
    kind: Kind,
    own: &Segment,
    shown: &BTreeSet<usize>,
    segments: &[Segment],
) -> Checked {
    check_at(reading, kind, Site::Named, own, shown, segments)
}

/// [`check`], saying where the value must be found.
pub fn check_at(
    reading: &Reading,
    kind: Kind,
    site: Site,
    own: &Segment,
    shown: &BTreeSet<usize>,
    segments: &[Segment],
) -> Checked {
    if reading.is_absent() {
        return Checked::Absent;
    }
    let at = reading.at;
    if !shown.contains(&at) {
        return Checked::Refused(Refusal::NotShown {
            at,
            shown: shown.clone(),
        });
    }
    let Some(named) = segments.get(at).filter(|row| row.document == own.document) else {
        return Checked::Refused(Refusal::NotThisDocument { at });
    };
    let passage = match site {
        Site::Named => named,
        Site::Own => own,
    };
    let contained = match kind {
        Kind::Money => contains_money_token(&passage.text, &reading.value),
        Kind::Name | Kind::Phrase => contains_squashed(&passage.text, &reading.value),
    };
    if !contained {
        return Checked::Refused(Refusal::NotInPassage { at });
    }
    let parses = match kind {
        Kind::Money => {
            money_token(reading.value.trim()).is_some_and(|end| end == reading.value.trim().len())
        }
        Kind::Name => !reading.value.trim().is_empty(),
        Kind::Phrase => true,
    };
    let mut warnings = Vec::new();
    if kind == Kind::Money {
        let sums = count_money_tokens(&passage.text);
        if sums > 1 {
            warnings.push(Warning::SeveralSumsInPassage { at, sums });
        }
    }
    let passages = segments
        .iter()
        .filter(|row| row.document == own.document && contains_squashed(&row.text, &reading.value))
        .count();
    if passages > 1 {
        warnings.push(Warning::ValueInSeveralPassages { passages });
    }
    Checked::Supported {
        passage: passage.clone(),
        parses,
        warnings,
    }
}

/// #460 rule one, whitespace-insensitive: a line break is an artefact
/// of the page, not of the value.
pub fn contains_squashed(text: &str, wanted: &str) -> bool {
    let wanted = squash(wanted);
    !wanted.is_empty() && squash(text).contains(&wanted)
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The value is in the text as a **whole** money token: not the front
/// of a larger figure (`£360` in `£360.00`, `£1,2` in `£1,250.00`) and
/// not the tail of one (`50.00` in `£1,250.00`).
fn contains_money_token(text: &str, wanted: &str) -> bool {
    let text = squash(text);
    let wanted = squash(wanted);
    if wanted.is_empty() {
        return false;
    }
    let mut from = 0;
    while let Some(found) = text[from..].find(&wanted) {
        let start = from + found;
        let end = start + wanted.len();
        let before_ok = text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '.' && c != ',');
        let after_ok = match text[end..].chars().next() {
            None => true,
            Some(c) if c.is_ascii_digit() => false,
            // `£360` followed by `.00`: the front of a larger figure.
            Some('.') | Some(',') => !text[end + 1..].starts_with(|c: char| c.is_ascii_digit()),
            Some(c) => !c.is_alphanumeric(),
        };
        if before_ok && after_ok {
            return true;
        }
        from = start + text[start..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

/// The length of the money token at the start of `text`, if one is
/// there: an optional sign (`£`, `€`, `$`, or a trailing code `GBP`,
/// `EUR`, `USD`), digits with optional thousands commas, optional
/// pence. A bare figure with neither sign, code nor pence is a count,
/// not a sum.
fn money_token(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut signed = false;
    for sign in ["£", "€", "$"] {
        if text.starts_with(sign) {
            at += sign.len();
            signed = true;
            break;
        }
    }
    while at < bytes.len() && bytes[at] == b' ' {
        at += 1;
    }
    let digits_start = at;
    while at < bytes.len() && (bytes[at].is_ascii_digit() || bytes[at] == b',') {
        at += 1;
    }
    if at == digits_start || !bytes[digits_start].is_ascii_digit() {
        return None;
    }
    let mut pence = false;
    if bytes.get(at) == Some(&b'.')
        && bytes.get(at + 1).is_some_and(u8::is_ascii_digit)
        && bytes.get(at + 2).is_some_and(u8::is_ascii_digit)
    {
        at += 3;
        pence = true;
    }
    let rest = &text[at..];
    let code = ["GBP", "EUR", "USD"]
        .iter()
        .find(|code| rest.trim_start().starts_with(**code) && rest.starts_with(' '));
    let end = match code {
        Some(code) => at + (rest.len() - rest.trim_start().len()) + code.len(),
        None => at,
    };
    if !signed && code.is_none() && !pence {
        return None;
    }
    if text[end..]
        .chars()
        .next()
        .is_some_and(char::is_alphanumeric)
    {
        return None;
    }
    Some(end)
}

fn count_money_tokens(text: &str) -> usize {
    let text = squash(text);
    let mut count = 0;
    let mut i = 0;
    while i < text.len() {
        if !text.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let before_ok = text[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        if before_ok {
            if let Some(len) = money_token(&text[i..]) {
                count += 1;
                i += len;
                continue;
            }
        }
        i += 1;
    }
    count
}
