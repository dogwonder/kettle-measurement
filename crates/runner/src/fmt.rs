//! How numbers and dates read to a person. Shared by the actions
//! emitter (#30) and the report template (#32) so the same amount never
//! appears two ways in one run.
//!
//! CONTRACT: implemented, not stubbed — two lanes depend on these, and
//! two implementations would drift.

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// "£1,046.64" — always two decimal places, thousands separated.
/// Sterling only for now; the report carries its currency and packs
/// that need another one will have to say so.
pub fn money(amount: Decimal) -> String {
    let rounded = amount.round_dp(2);
    let negative = rounded.is_sign_negative();
    let text = format!("{:.2}", rounded.abs());
    let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), "00"));

    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }

    let sign = if negative { "−" } else { "" };
    format!("{sign}£{grouped}.{fraction}")
}

/// "14 March 2027".
pub fn date(day: NaiveDate) -> String {
    format!(
        "{} {} {}",
        day.day(),
        MONTHS[day.month0() as usize],
        day.year()
    )
}

/// "May 2025".
pub fn month(day: NaiveDate) -> String {
    format!("{} {}", MONTHS[day.month0() as usize], day.year())
}

/// "2025-05" — the wire form, for `price_rise.month`.
pub fn iso_month(day: NaiveDate) -> String {
    format!("{:04}-{:02}", day.year(), day.month())
}

/// "1 thing", "3 passages" — a count with its noun agreeing in number.
/// Here rather than in a report builder because every builder wants it
/// and two copies of it had already grown (#394).
pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "5th", "1st", "22nd" — for "on or near the 5th".
pub fn ordinal(day: u32) -> String {
    let suffix = match (day % 10, day % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{day}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn decimal(text: &str) -> Decimal {
        Decimal::from_str(text).expect("test decimal parses")
    }

    #[test]
    fn money_groups_thousands_and_keeps_two_places() {
        assert_eq!(money(decimal("1046.64")), "£1,046.64");
        assert_eq!(money(decimal("95")), "£95.00");
        assert_eq!(money(decimal("1234567.5")), "£1,234,567.50");
    }

    #[test]
    fn dates_read_the_way_people_write_them() {
        let day = NaiveDate::from_ymd_opt(2027, 3, 14).expect("valid date");
        assert_eq!(date(day), "14 March 2027");
        assert_eq!(month(day), "March 2027");
        assert_eq!(iso_month(day), "2027-03");
    }

    #[test]
    fn counts_agree_in_number() {
        assert_eq!(count(1, "thing", "things"), "1 thing");
        assert_eq!(count(3, "passage", "passages"), "3 passages");
        assert_eq!(count(0, "term", "terms"), "0 terms");
    }

    #[test]
    fn ordinals_handle_the_teens() {
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(3), "3rd");
        assert_eq!(ordinal(5), "5th");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(12), "12th");
        assert_eq!(ordinal(13), "13th");
        assert_eq!(ordinal(21), "21st");
    }
}
