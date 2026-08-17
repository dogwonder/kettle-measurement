#!/usr/bin/env python3
"""Generate statement-04.pdf and statement-04.csv — a matched pair (#138, #217).

Why a script rather than a checked-in blob: `statement-03.pdf` arrived
with no provenance, so nobody can change one row of it without redrawing
the whole thing by hand. A fixture you cannot regenerate is a fixture
you cannot extend, and #137 will need to extend this one.

The pair is the point. `statement-04.csv` describes exactly the
transactions `statement-04.pdf` shows, so the CSV parser — which is
tested and trusted — is the oracle for PDF row reconstruction. A
reconstruction that merely looks plausible cannot pass.

What it exercises, deliberately:

- **Separate Paid Out / Paid In / Balance columns.** Real statements put
  direction in the column, not the sign. A single signed column (which
  is what statement-03 has) would let a reconstruction that ignores
  horizontal position pass, and ignoring horizontal position is the one
  thing #137 must not do.
- **An empty money column on every row.** This is where direction is
  lost: after a space-join, "24.99" carries no trace of which column it
  came from.
- **Both directions**, so an inverted reconstruction fails rather than
  passing by symmetry.
- **Six months and pages, with repeated column headers and page
  furniture.** None of it may become a transaction. Repeated merchants
  reach recurrence, annualised totals and a price rise rather than
  stopping at a structurally valid but empty report (#217).
- **`%d %b %y` dates**, matching what real statements use (#136).

Everything is invented. Never a real export (CLAUDE.md).

    pip install reportlab
    python3 packs/app.kttl.subscription-audit/fixtures/make-statement-04.py
"""

from decimal import Decimal
from pathlib import Path

from reportlab.lib.pagesizes import A4
from reportlab.pdfgen import canvas

HERE = Path(__file__).parent

# Five rows per month: three subscriptions, deliberately irregular
# coffee spending, and income. Netflix rises after three payments, so
# both amount clusters are long enough to establish one monthly series.
MONTHS = [
    ("Jan", "10.99", "3.40"),
    ("Feb", "10.99", "4.10"),
    ("Mar", "10.99", "3.20"),
    ("Apr", "12.99", "4.80"),
    ("May", "12.99", "3.90"),
    ("Jun", "12.99", "4.40"),
]

# (date, description, paid_out, paid_in) — exactly one amount per row.
ROWS = []
for month, netflix, coffee in MONTHS:
    ROWS.extend(
        [
            (f"03 {month} 25", "PUREGYM LTD", "24.99", ""),
            (f"05 {month} 25", "NETFLIX.COM", netflix, ""),
            (f"07 {month} 25", "SPOTIFY LTD", "9.99", ""),
            (f"12 {month} 25", "KAFFA COFFEE", coffee, ""),
            (f"28 {month} 25", "ACME PAYROLL", "", "2450.00"),
        ]
    )

ROWS_PER_PAGE = 5
PAGES = (len(ROWS) + ROWS_PER_PAGE - 1) // ROWS_PER_PAGE

# Column x positions, in points. Money columns are right-aligned as a
# real statement draws them, which is exactly why reconstruction has to
# work from a fragment's extent rather than its left edge alone.
X_DATE, X_DESC = 40, 120
X_OUT, X_IN, X_BALANCE = 340, 420, 520

TOP = 780
ROW_HEIGHT = 22


def draw_page(
    pdf: canvas.Canvas, page: int, rows: list, opening_balance: Decimal
) -> Decimal:
    pdf.setFont("Helvetica-Bold", 11)
    pdf.drawString(X_DATE, TOP, "ACME BANK Statement January to June 2025")

    pdf.setFont("Helvetica-Bold", 9)
    header_y = TOP - 30
    pdf.drawString(X_DATE, header_y, "Date")
    pdf.drawString(X_DESC, header_y, "Description")
    # Headers sit over their columns, right-aligned like the values.
    pdf.drawRightString(X_OUT, header_y, "Paid Out")
    pdf.drawRightString(X_IN, header_y, "Paid In")
    pdf.drawRightString(X_BALANCE, header_y, "Balance")

    pdf.setFont("Helvetica", 9)
    balance = opening_balance
    for n, (date, description, paid_out, paid_in) in enumerate(rows):
        y = header_y - ROW_HEIGHT * (n + 1)
        balance += Decimal(paid_in or "0") - Decimal(paid_out or "0")
        pdf.drawString(X_DATE, y, date)
        pdf.drawString(X_DESC, y, description)
        if paid_out:
            pdf.drawRightString(X_OUT, y, paid_out)
        if paid_in:
            pdf.drawRightString(X_IN, y, paid_in)
        pdf.drawRightString(X_BALANCE, y, f"{balance:,.2f}")

    # Page furniture: must never be read as a transaction, though it
    # sits in the same columns and carries digits.
    pdf.setFont("Helvetica", 8)
    pdf.drawString(X_DATE, 60, f"Page {page} of {PAGES}")
    pdf.drawString(X_DATE, 48, "ACME BANK plc. Registered in England 00000000.")
    return balance


def main() -> None:
    pdf = canvas.Canvas(
        str(HERE / "statement-04.pdf"), pagesize=A4, invariant=1
    )
    balance = Decimal("1000.00")
    for page in range(PAGES):
        balance = draw_page(
            pdf,
            page + 1,
            ROWS[page * ROWS_PER_PAGE : (page + 1) * ROWS_PER_PAGE],
            balance,
        )
        pdf.showPage()
    pdf.save()

    # The oracle: the same transactions, in the CSV shape HSBC exports
    # (#136), so the trusted parser can say what the PDF ought to yield.
    lines = ["Date,Description,Paid Out,Paid In"]
    lines += [f"{d},{desc},{out},{into}" for d, desc, out, into in ROWS]
    (HERE / "statement-04.csv").write_text("\n".join(lines) + "\n")

    print(f"wrote statement-04.pdf ({PAGES} pages) and statement-04.csv")


if __name__ == "__main__":
    main()
