#!/usr/bin/env python3
"""Generate statement-03.pdf — an intentionally unsupported layout (#218).

The statement has a text layer and positioned Date / Description /
Amount columns, but it deliberately does not say whether positive
amounts are money in or money out. Kettle must not infer that convention
from merchant names or from the signs themselves. Page two also
continues without repeating the header, as real statements sometimes do.

Everything is invented. Never a real export (CLAUDE.md).

    pip install reportlab
    python3 packs/app.kttl.subscription-audit/fixtures/make-statement-03.py
"""

from pathlib import Path

from reportlab.lib.pagesizes import A4
from reportlab.pdfgen import canvas

HERE = Path(__file__).parent
X_DATE, X_DESCRIPTION, X_AMOUNT = 60, 180, 400


def main() -> None:
    pdf = canvas.Canvas(
        str(HERE / "statement-03.pdf"), pagesize=A4, invariant=1
    )

    pdf.setFont("Helvetica", 10)
    pdf.drawString(X_DATE, 780, "ACME BANK")
    pdf.drawString(X_DESCRIPTION, 780, "Statement January 2025")
    pdf.drawString(X_DATE, 720, "Date")
    pdf.drawString(X_DESCRIPTION, 720, "Description")
    pdf.drawString(X_AMOUNT, 720, "Amount")
    pdf.drawString(X_DATE, 700, "03/01/2025")
    pdf.drawString(X_DESCRIPTION, 700, "PUREGYM LTD")
    pdf.drawString(X_AMOUNT, 700, "-24.99")
    pdf.drawString(X_DATE, 680, "15/01/2025")
    pdf.drawString(X_DESCRIPTION, 680, "NETFLIX.COM")
    pdf.drawString(X_AMOUNT, 680, "-10.99")
    pdf.showPage()

    pdf.setFont("Helvetica", 10)
    pdf.drawString(X_DATE, 780, "Page 2 of 2")
    pdf.drawString(X_DATE, 720, "28/01/2025")
    pdf.drawString(X_DESCRIPTION, 720, "ACME PAYROLL")
    pdf.drawString(X_AMOUNT, 720, "2450.00")
    pdf.showPage()

    pdf.save()
    print("wrote statement-03.pdf (2 pages)")


if __name__ == "__main__":
    main()
