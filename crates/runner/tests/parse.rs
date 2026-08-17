use chrono::NaiveDate;
use runner::parse::{parse_statement, parse_statement_file, Direction};
use rust_decimal::Decimal;
use std::path::PathBuf;
use std::str::FromStr;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs/app.kttl.subscription-audit/fixtures")
        .join(name)
}

#[test]
fn statement_01_yields_expected_transactions() {
    let parsed = parse_statement_file(&fixture("statement-01.csv")).unwrap();
    let transactions = parsed.transactions;
    assert!(parsed.warnings.is_empty());

    assert_eq!(transactions.len(), 58);

    let first = &transactions[0];
    assert_eq!(first.date, NaiveDate::from_ymd_opt(2025, 1, 3).unwrap());
    assert_eq!(first.raw_merchant, "PUREGYM LTD");
    assert_eq!(first.amount, Decimal::from_str("24.99").unwrap());
    assert_eq!(first.direction, Direction::Debit);

    let last = transactions.last().unwrap();
    assert_eq!(last.date, NaiveDate::from_ymd_opt(2025, 12, 28).unwrap());
    assert_eq!(last.raw_merchant, "ACME PAYROLL");
    assert_eq!(last.amount, Decimal::from_str("2450.00").unwrap());
    assert_eq!(last.direction, Direction::Credit);
}

#[test]
fn amounts_survive_as_exact_decimals() {
    let transactions = parse_statement_file(&fixture("statement-01.csv"))
        .unwrap()
        .transactions;

    // 4 × 10.99 + 8 × 12.99 — would drift under floats.
    let netflix: Decimal = transactions
        .iter()
        .filter(|t| t.raw_merchant == "NETFLIX.COM")
        .map(|t| t.amount)
        .sum();
    assert_eq!(netflix, Decimal::from_str("147.88").unwrap());
}

#[test]
fn statement_02_messy_parses_in_full() {
    let parsed = parse_statement_file(&fixture("statement-02-messy.csv")).unwrap();
    assert_eq!(parsed.transactions.len(), 53);
    assert!(parsed.warnings.is_empty());
}

#[test]
fn debit_credit_columns_detect_direction() {
    let csv_data = "\
Date,Description,Debit,Credit
2025-01-03,PUREGYM LTD,24.99,
2025-01-28,ACME PAYROLL,,2450.00
";
    let transactions = parse_statement(csv_data.as_bytes()).unwrap().transactions;

    assert_eq!(transactions.len(), 2);
    assert_eq!(transactions[0].amount, Decimal::from_str("24.99").unwrap());
    assert_eq!(transactions[0].direction, Direction::Debit);
    assert_eq!(
        transactions[1].amount,
        Decimal::from_str("2450.00").unwrap()
    );
    assert_eq!(transactions[1].direction, Direction::Credit);
}

#[test]
fn inflow_only_column_is_money_coming_in() {
    let csv_data = "\
Date,Payee,Inflow,skip
03 Jan 25,ACME PAYROLL,2450.00,
04 Jan 25,PUREGYM LTD,-24.99,
05 Jan 25,EMPTY AMOUNT,,
";
    let parsed = parse_statement(csv_data.as_bytes()).unwrap();

    assert_eq!(parsed.transactions.len(), 2);
    assert_eq!(
        parsed.transactions[0].amount,
        Decimal::from_str("2450.00").unwrap()
    );
    assert_eq!(parsed.transactions[0].direction, Direction::Credit);
    assert_eq!(
        parsed.transactions[1].amount,
        Decimal::from_str("24.99").unwrap()
    );
    assert_eq!(parsed.transactions[1].direction, Direction::Debit);
    assert_eq!(parsed.warnings.len(), 1);
    assert!(
        parsed.warnings[0].contains("row 4"),
        "{:?}",
        parsed.warnings
    );
    assert!(
        parsed.warnings[0].contains("amount is empty"),
        "{:?}",
        parsed.warnings
    );
}

#[test]
fn outflow_only_column_is_money_going_out() {
    let csv_data = "\
Date,Payee,Outflow
03-Jan-2025,PUREGYM LTD,24.99
04-Jan-2025,PUREGYM REFUND,-9.99
05/01/2025,JOHN LEWIS,15.50
";
    let parsed = parse_statement(csv_data.as_bytes()).unwrap();

    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    assert_eq!(parsed.transactions.len(), 3);
    assert_eq!(
        parsed.transactions[0].amount,
        Decimal::from_str("24.99").unwrap()
    );
    assert_eq!(parsed.transactions[0].direction, Direction::Debit);
    assert_eq!(
        parsed.transactions[1].amount,
        Decimal::from_str("9.99").unwrap()
    );
    assert_eq!(parsed.transactions[1].direction, Direction::Credit);
    assert_eq!(
        parsed.transactions[2].date,
        NaiveDate::from_ymd_opt(2025, 1, 5).unwrap()
    );
    assert_eq!(parsed.transactions[2].direction, Direction::Debit);
}

#[test]
fn signed_values_in_separate_columns_keep_the_columns_direction() {
    let csv_data = "\
Date,Description,Debit,Credit
2025-01-03,PUREGYM LTD,-24.99,
2025-01-28,ACME PAYROLL,,-2450.00
";
    let parsed = parse_statement(csv_data.as_bytes()).unwrap();

    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    assert_eq!(
        parsed.transactions[0].amount,
        Decimal::from_str("24.99").unwrap()
    );
    assert_eq!(parsed.transactions[0].direction, Direction::Debit);
    assert_eq!(
        parsed.transactions[1].amount,
        Decimal::from_str("2450.00").unwrap()
    );
    assert_eq!(parsed.transactions[1].direction, Direction::Credit);
}

#[test]
fn both_debit_and_credit_populated_is_skipped() {
    let csv_data = "\
Date,Description,Debit,Credit
2025-01-03,AMBIGUOUS,24.99,25.00
";
    let parsed = parse_statement(csv_data.as_bytes()).unwrap();

    assert!(parsed.transactions.is_empty());
    assert_eq!(parsed.warnings.len(), 1);
    assert!(
        parsed.warnings[0].contains("both Debit and Credit"),
        "{:?}",
        parsed.warnings
    );
}

#[test]
fn both_debit_and_credit_empty_is_skipped() {
    let csv_data = "\
Date,Description,Debit,Credit
2025-01-03,NO MOVEMENT,,
";
    let parsed = parse_statement(csv_data.as_bytes()).unwrap();

    assert!(parsed.transactions.is_empty());
    assert_eq!(parsed.warnings.len(), 1);
    assert!(
        parsed.warnings[0].contains("both Debit and Credit are empty"),
        "{:?}",
        parsed.warnings
    );
}

#[test]
fn empty_merchant_is_skipped() {
    let csv_data = "\
Date,Description,Amount
2025-01-03,,-24.99
";
    let parsed = parse_statement(csv_data.as_bytes()).unwrap();

    assert!(parsed.transactions.is_empty());
    assert_eq!(parsed.warnings.len(), 1);
    assert!(
        parsed.warnings[0].contains("merchant is empty"),
        "{:?}",
        parsed.warnings
    );
}

#[test]
fn zero_amount_is_skipped() {
    let csv_data = "\
Date,Description,Amount
2025-01-03,BALANCE FORWARD,0
2025-01-04,NEGATIVE ZERO,-0.00
";
    let parsed = parse_statement(csv_data.as_bytes()).unwrap();

    assert!(parsed.transactions.is_empty());
    assert_eq!(parsed.warnings.len(), 2);
    assert!(
        parsed
            .warnings
            .iter()
            .all(|warning| warning.contains("amount is zero")),
        "{:?}",
        parsed.warnings
    );
}

#[test]
fn headers_tolerate_bom_whitespace_and_ascii_case() {
    let csv_data = "\
\u{feff} date , DESCRIPTION , amount
2025-01-03,PUREGYM LTD,-24.99
";
    let parsed = parse_statement(csv_data.as_bytes()).unwrap();

    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    assert_eq!(parsed.transactions.len(), 1);
    assert_eq!(parsed.transactions[0].raw_merchant, "PUREGYM LTD");
    assert_eq!(parsed.transactions[0].direction, Direction::Debit);
}

#[test]
fn monzo_and_starling_headers_map_correctly() {
    // Synthetic rows in the banks' real export shapes: DD/MM/YYYY dates,
    // signed amounts, merchant in Name (Monzo) / Counter Party (Starling).
    let monzo = "\
Transaction ID,Date,Time,Type,Name,Emoji,Category,Amount,Currency,Local amount,Local currency,Notes and #tags,Address,Receipt,Description,Category split,Money Out,Money In,Balance,Balance currency
tx_0001,03/01/2025,09:12:44,Card payment,Puregym,,Fitness,-24.99,GBP,-24.99,GBP,,,,PUREGYM LTD REF 998,,-24.99,,975.01,GBP
tx_0002,28/01/2025,11:00:00,Faster payment,Acme Payroll,,Income,2450.00,GBP,2450.00,GBP,,,,ACME PAYROLL JAN,,,2450.00,3425.01,GBP
";
    let transactions = parse_statement(monzo.as_bytes()).unwrap().transactions;
    assert_eq!(transactions.len(), 2);
    assert_eq!(
        transactions[0].date,
        NaiveDate::from_ymd_opt(2025, 1, 3).unwrap()
    );
    assert_eq!(transactions[0].raw_merchant, "Puregym");
    assert_eq!(transactions[0].amount, Decimal::from_str("24.99").unwrap());
    assert_eq!(transactions[0].direction, Direction::Debit);
    assert_eq!(transactions[1].raw_merchant, "Acme Payroll");
    assert_eq!(transactions[1].direction, Direction::Credit);

    let starling = "\
Date,Counter Party,Reference,Type,Amount (GBP),Balance (GBP)
03/01/2025,Puregym,PUREGYM LTD REF 998,CONTACTLESS,-24.99,975.01
28/01/2025,Acme Payroll,ACME PAYROLL JAN,FASTER PAYMENT,2450.00,3425.01
";
    let transactions = parse_statement(starling.as_bytes()).unwrap().transactions;
    assert_eq!(transactions.len(), 2);
    assert_eq!(
        transactions[0].date,
        NaiveDate::from_ymd_opt(2025, 1, 3).unwrap()
    );
    assert_eq!(transactions[0].raw_merchant, "Puregym");
    assert_eq!(transactions[0].amount, Decimal::from_str("24.99").unwrap());
    assert_eq!(transactions[0].direction, Direction::Debit);
    assert_eq!(transactions[1].raw_merchant, "Acme Payroll");
    assert_eq!(transactions[1].direction, Direction::Credit);
}

#[test]
fn old_monzo_layout_maps_by_position() {
    // The original 12-column Monzo export did not offer a dependable
    // named-header contract. These deliberately generic headers prove
    // that the recognised row shape, then position, drives the mapping.
    let monzo = "\
column-0,column-1,column-2,column-3,column-4,column-5,column-6,column-7,column-8,column-9,column-10,column-11
tx_0001,2025-01-03 09:12:44 +0000,-24.99,GBP,-24.99,GBP,fitness,,Puregym,,,
tx_0002,2025-01-28 11:00:00 +0000,2450.00,GBP,2450.00,GBP,income,,Acme Payroll,,,
";
    let parsed = parse_statement(monzo.as_bytes()).unwrap();

    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    assert_eq!(parsed.transactions.len(), 2);
    assert_eq!(
        parsed.transactions[0].date,
        NaiveDate::from_ymd_opt(2025, 1, 3).unwrap()
    );
    assert_eq!(parsed.transactions[0].raw_merchant, "Puregym");
    assert_eq!(
        parsed.transactions[0].amount,
        Decimal::from_str("24.99").unwrap()
    );
    assert_eq!(parsed.transactions[0].direction, Direction::Debit);
    assert_eq!(parsed.transactions[1].raw_merchant, "Acme Payroll");
    assert_eq!(parsed.transactions[1].direction, Direction::Credit);
}

#[test]
fn positional_detection_does_not_make_a_bad_first_row_a_file_error() {
    let monzo = "\
column-0,column-1,column-2,column-3,column-4,column-5,column-6,column-7,column-8,column-9,column-10,column-11
opening-balance,not-a-date,,,,,,,,,,
tx_0001,2025-01-03 09:12:44 +0000,-24.99,GBP,-24.99,GBP,fitness,,Puregym,,,
";
    let parsed = parse_statement(monzo.as_bytes()).unwrap();

    assert_eq!(parsed.transactions.len(), 1);
    assert_eq!(parsed.transactions[0].raw_merchant, "Puregym");
    assert_eq!(parsed.warnings.len(), 1);
    assert!(
        parsed.warnings[0].contains("row 2"),
        "{:?}",
        parsed.warnings
    );
}

#[test]
fn barclaycard_business_20_and_21_column_layouts_map_by_position() {
    for column_count in [20, 21] {
        let headers = (0..column_count)
            .map(|index| format!("column-{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut row = vec![
            "Jane Smith",
            "****9940",
            "10/04/2019",
            "FOOD WORKS",
            "10.00",
            "GBP",
            "10.00",
            "GBP",
            "1",
            "12/04/2019",
            "182500",
            "28271",
            "TXN1",
            "Restaurants",
            "PURCHASE",
            "Eating Places",
            "LONDON",
            "London",
            "SW1P",
            "5812",
        ];
        if column_count == 21 {
            row.push("2019-04");
        }
        let csv_data = format!("{headers}\n{}\n", row.join(","));

        let parsed = parse_statement(csv_data.as_bytes()).unwrap();

        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        assert_eq!(parsed.transactions.len(), 1);
        assert_eq!(
            parsed.transactions[0].date,
            NaiveDate::from_ymd_opt(2019, 4, 10).unwrap()
        );
        assert_eq!(parsed.transactions[0].raw_merchant, "FOOD WORKS");
        assert_eq!(
            parsed.transactions[0].amount,
            Decimal::from_str("10.00").unwrap()
        );
        assert_eq!(parsed.transactions[0].direction, Direction::Debit);
    }
}

#[test]
fn positional_layout_requires_a_recognisable_row_shape() {
    let csv_data = "\
column-0,column-1,column-2,column-3,column-4,column-5,column-6,column-7,column-8,column-9,column-10,column-11
one,two,three,four,five,six,seven,eight,nine,ten,eleven,twelve
";
    let error = parse_statement(csv_data.as_bytes())
        .unwrap_err()
        .to_string();

    assert!(error.contains("did not recognise the columns"), "{error}");
}

#[test]
fn unknown_headers_produce_plain_language_error() {
    let csv_data = "\
Datum,Omschrijving,Bedrag
2025-01-03,SPORTSCHOOL BV,-24.99
";
    let error = parse_statement(csv_data.as_bytes())
        .unwrap_err()
        .to_string();

    assert!(error.contains("Datum, Omschrijving, Bedrag"), "{error}");
    // Says what Kettle *can* read, so the user knows what to export.
    assert!(error.contains("Monzo"), "{error}");
    assert!(error.contains("Starling"), "{error}");
}

#[test]
fn bad_rows_are_skipped_with_warnings() {
    let csv_data = "\
Date,Description,Amount
2025-01-03,PUREGYM LTD,-24.99
Balance brought forward,,
2025-01-28,ACME PAYROLL,2450.00
";
    let parsed = parse_statement(csv_data.as_bytes()).unwrap();

    assert_eq!(parsed.transactions.len(), 2);
    assert_eq!(parsed.transactions[1].raw_merchant, "ACME PAYROLL");
    assert_eq!(parsed.warnings.len(), 1);
    assert!(
        parsed.warnings[0].contains("row 3"),
        "{}",
        parsed.warnings[0]
    );
}

// ---------------------------------------------------------------------------
// HSBC (#136). Found by the #111 acceptance session: a real export was
// refused because `Paid Out` / `Paid In` had no row in the mapping
// table. An ordinary debit/credit pair — the shape #135 already models
// — so this is coverage, not new machinery.

#[test]
fn hsbc_paid_out_and_paid_in_are_money_out_and_in() {
    // Four-digit named month: what the CSV export uses. The PDF of the
    // same account uses a two-digit year (#137), which is why the row
    // lists both.
    let csv_data = "\
Date,Description,Paid Out,Paid In
18 Dec 2025,PUREGYM LTD,24.99,
19 Dec 2025,ACME PAYROLL,,2450.00
";
    let parsed = parse_statement(csv_data.as_bytes()).unwrap();
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    let transactions = parsed.transactions;

    assert_eq!(transactions.len(), 2);
    assert_eq!(
        transactions[0].date,
        NaiveDate::from_ymd_opt(2025, 12, 18).unwrap()
    );
    assert_eq!(transactions[0].raw_merchant, "PUREGYM LTD");
    assert_eq!(transactions[0].amount, Decimal::from_str("24.99").unwrap());
    assert_eq!(transactions[0].direction, Direction::Debit);

    assert_eq!(
        transactions[1].amount,
        Decimal::from_str("2450.00").unwrap()
    );
    assert_eq!(transactions[1].direction, Direction::Credit);
}

/// The same headers arrive on HSBC's business card export. There, a
/// `Paid In` is a repayment to the card, not earnings — and a purchase
/// is still money spent. Reading it backwards would report every
/// purchase as income and every repayment as a subscription, which is
/// #79's failure mode with the sign inverted across a whole statement.
#[test]
fn hsbc_card_repayment_is_money_in_and_a_purchase_is_money_out() {
    let csv_data = "\
Date,Description,Paid Out,Paid In
02 Jan 2026,NETFLIX.COM,10.99,
05 Jan 2026,PAYMENT RECEIVED THANK YOU,,150.00
";
    let transactions = parse_statement(csv_data.as_bytes()).unwrap().transactions;

    assert_eq!(transactions[0].direction, Direction::Debit);
    assert_eq!(transactions[1].direction, Direction::Credit);
    // Magnitudes, never signed values carried through.
    assert!(transactions.iter().all(|t| t.amount > Decimal::ZERO));
}

/// The two-digit year the PDF export uses, on the same row (#137).
#[test]
fn hsbc_accepts_both_named_month_year_widths() {
    let csv_data = "\
Date,Description,Paid Out,Paid In
18 Dec 25,PUREGYM LTD,24.99,
";
    let transactions = parse_statement(csv_data.as_bytes()).unwrap().transactions;
    assert_eq!(
        transactions[0].date,
        NaiveDate::from_ymd_opt(2025, 12, 18).unwrap()
    );
}

/// HSBC has to be in the list of banks the unrecognised-columns message
/// names, or a person whose export *is* supported is told it isn't.
#[test]
fn hsbc_is_named_among_the_banks_kettle_can_read() {
    let unreadable = "Some,Other,Columns\n1,2,3\n";
    let error = parse_statement(unreadable.as_bytes()).unwrap_err();
    assert!(
        error.to_string().contains("HSBC"),
        "the message should name HSBC: {error}"
    );
}

/// A wide year format ahead of a narrow one is a silent date bug, not a
/// fall-through (#136).
///
/// chrono's `%Y` is variable-width: given "25" it yields year 0025
/// rather than failing, so a list that tries `%Y` first never reaches
/// the `%y` behind it. `%y` demands exactly two digits and rejects
/// "2025" cleanly, so narrow-before-wide is the only order that parses
/// both. A date nineteen centuries out isn't a visible error — it
/// quietly wrecks recurrence detection and the report's date span — so
/// every named-month format is pinned here.
#[test]
fn a_two_digit_year_is_never_read_as_the_first_century() {
    let sane = |csv: &str, shape: &str| {
        let parsed =
            parse_statement(csv.as_bytes()).unwrap_or_else(|e| panic!("{shape} should parse: {e}"));
        let year = parsed.transactions[0].date.format("%Y").to_string();
        assert_eq!(year, "2025", "{shape} misread the year");
    };

    // HSBC: named month, space separated.
    sane(
        "Date,Description,Paid Out,Paid In\n18 Dec 25,PUREGYM LTD,24.99,\n",
        "HSBC two-digit",
    );
    sane(
        "Date,Description,Paid Out,Paid In\n18 Dec 2025,PUREGYM LTD,24.99,\n",
        "HSBC four-digit",
    );

    // John Lewis: named month, hyphen separated — `%d-%b-%Y` leads its
    // list, so a two-digit year would land in year 25 without a
    // narrower format ahead of it.
    sane(
        "Date,Payee,Outflow\n18-Dec-25,PUREGYM LTD,24.99\n",
        "John Lewis two-digit",
    );
    sane(
        "Date,Payee,Outflow\n18-Dec-2025,PUREGYM LTD,24.99\n",
        "John Lewis four-digit",
    );
}

/// The oracle half of the #137 fixture pair (#138).
///
/// `statement-04.csv` describes exactly the transactions
/// `statement-04.pdf` shows. Pinning it here means PDF row
/// reconstruction has something trusted to be measured against, rather
/// than being judged on whether its output looks plausible.
#[test]
fn statement_04_csv_is_a_usable_oracle_for_its_pdf() {
    let parsed = parse_statement_file(&fixture("statement-04.csv")).unwrap();
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    let transactions = parsed.transactions;

    assert_eq!(transactions.len(), 30);
    // Both directions are present, so an inverted reconstruction cannot
    // pass by symmetry.
    assert_eq!(
        transactions
            .iter()
            .filter(|t| t.direction == Direction::Debit)
            .count(),
        24
    );
    assert_eq!(
        transactions
            .iter()
            .filter(|t| t.direction == Direction::Credit)
            .count(),
        6
    );

    let first = &transactions[0];
    assert_eq!(first.date, NaiveDate::from_ymd_opt(2025, 1, 3).unwrap());
    assert_eq!(first.raw_merchant, "PUREGYM LTD");
    assert_eq!(first.amount, Decimal::from_str("24.99").unwrap());
    assert_eq!(first.direction, Direction::Debit);

    // The row whose amount sits in the *other* money column, with the
    // same shape as its neighbours. This is the one a line-based
    // reconstruction gets wrong.
    let payroll = &transactions[4];
    assert_eq!(payroll.raw_merchant, "ACME PAYROLL");
    assert_eq!(payroll.amount, Decimal::from_str("2450.00").unwrap());
    assert_eq!(payroll.direction, Direction::Credit);
}
