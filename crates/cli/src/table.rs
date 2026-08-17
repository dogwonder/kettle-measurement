//! Renders a parsed statement as a plain-text table for `kettle parse` —
//! the quick "did Kettle read my statement correctly?" checkpoint.

use runner::parse::{Direction, ParsedStatement};

pub fn render(parsed: &ParsedStatement) -> String {
    let mut out = String::new();

    out.push_str(&format!("{:<10}  {:>10}  Merchant\n", "Date", "Amount"));
    for transaction in &parsed.transactions {
        let signed = match transaction.direction {
            Direction::Debit => -transaction.amount,
            Direction::Credit => transaction.amount,
        };
        out.push_str(&format!(
            "{}  {signed:>10}  {}\n",
            transaction.date, transaction.raw_merchant
        ));
    }

    let count = parsed.transactions.len();
    let plural = if count == 1 { "" } else { "s" };
    out.push_str(&format!("\n{count} transaction{plural}\n"));

    if !parsed.warnings.is_empty() {
        let rows = parsed.warnings.len();
        let row_word = if rows == 1 { "row" } else { "rows" };
        out.push_str(&format!("\nSkipped {rows} {row_word} it couldn't read:\n"));
        for warning in &parsed.warnings {
            out.push_str(&format!("  - {warning}\n"));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use runner::parse::{Direction, Transaction};
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn transaction(date: &str, merchant: &str, amount: &str, direction: Direction) -> Transaction {
        Transaction {
            date: NaiveDate::from_str(date).unwrap(),
            raw_merchant: merchant.to_owned(),
            amount: Decimal::from_str(amount).unwrap(),
            direction,
        }
    }

    #[test]
    fn renders_signed_amounts_summary_and_warnings() {
        let parsed = ParsedStatement {
            transactions: vec![
                transaction("2025-01-03", "PUREGYM LTD", "24.99", Direction::Debit),
                transaction("2025-01-28", "ACME PAYROLL", "2450.00", Direction::Credit),
            ],
            warnings: vec!["row 3: could not read the date \"Balance\" — row skipped".into()],
        };

        let out = render(&parsed);

        // Money out shows negative, right-aligned to a 10-wide column.
        assert!(out.contains("2025-01-03      -24.99  PUREGYM LTD"), "{out}");
        assert!(
            out.contains("2025-01-28     2450.00  ACME PAYROLL"),
            "{out}"
        );
        assert!(out.contains("2 transactions"), "{out}");
        assert!(out.contains("Skipped 1 row it couldn't read:"), "{out}");
        assert!(out.contains("row 3"), "{out}");
    }

    #[test]
    fn no_warnings_section_when_nothing_was_skipped() {
        let parsed = ParsedStatement {
            transactions: vec![transaction(
                "2025-01-03",
                "PUREGYM LTD",
                "24.99",
                Direction::Debit,
            )],
            warnings: vec![],
        };

        let out = render(&parsed);

        assert!(out.contains("1 transaction\n"), "{out}");
        assert!(!out.contains("Skipped"), "{out}");
    }
}
