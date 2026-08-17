#!/usr/bin/env python3
"""Generate statement-05.pdf — a signed Amount column the balance explains (#218).

The counterpart to statement-03.pdf. Both have Date / Description /
Amount with a single signed money column; the difference is that this
one carries a running Balance, so the arithmetic settles what the sign
means and Kettle can read it without guessing.

The convention here is `balance[n] = balance[n-1] + amount[n]` — a
negative amount is money out. Nothing says so in words, and nothing
should have to: the balances say it.

Deliberately the *less* obvious of the two conventions to test against,
in the sense that a reader assuming "positive = spend" would invert
every figure and still produce a plausible-looking report. That is the
failure this fixture exists to catch.

Everything is invented. Never a real export (CLAUDE.md).

    pip install reportlab
    python3 packs/app.kttl.subscription-audit/fixtures/make-statement-05.py
"""

from pathlib import Path

from reportlab.lib.pagesizes import A4
from reportlab.pdfgen import canvas

HERE = Path(__file__).parent
X_DATE, X_DESCRIPTION, X_AMOUNT_R, X_BALANCE_R = 60, 180, 430, 540

OPENING = 1000.00

# (date, description, signed amount). Money out is negative.
ROWS = [
    ("03/01/2025", "PUREGYM LTD", -24.99),
    ("05/01/2025", "NETFLIX.COM", -10.99),
    ("09/01/2025", "SAINSBURYS SPRDMKT", -63.20),
    ("12/01/2025", "THAMES WATER", -31.00),
    ("18/01/2025", "SPOTIFY UK", -11.99),
    ("22/01/2025", "SAINSBURYS SPRDMKT", -48.75),
    ("28/01/2025", "ACME PAYROLL", 2450.00),
]


def main() -> None:
    pdf = canvas.Canvas(str(HERE / "statement-05.pdf"), pagesize=A4, invariant=1)
    pdf.setFont("Helvetica", 10)

    pdf.drawString(X_DATE, 780, "ACME BANK")
    pdf.drawString(X_DESCRIPTION, 780, "Statement January 2025")

    pdf.drawString(X_DATE, 720, "Date")
    pdf.drawString(X_DESCRIPTION, 720, "Description")
    pdf.drawRightString(X_AMOUNT_R, 720, "Amount")
    pdf.drawRightString(X_BALANCE_R, 720, "Balance")

    balance = OPENING
    y = 700
    for date, description, amount in ROWS:
        balance = round(balance + amount, 2)
        pdf.drawString(X_DATE, y, date)
        pdf.drawString(X_DESCRIPTION, y, description)
        pdf.drawRightString(X_AMOUNT_R, y, f"{amount:.2f}")
        pdf.drawRightString(X_BALANCE_R, y, f"{balance:.2f}")
        y -= 20

    pdf.showPage()
    pdf.save()
    print(f"wrote statement-05.pdf ({len(ROWS)} rows, closing balance {balance:.2f})")


if __name__ == "__main__":
    main()
