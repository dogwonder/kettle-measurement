"""Generate a wholly invented busy year's statement, CSV and 60-page PDF.

Requires reportlab. Integer pence are source truth; no Kettle parser/model
authors expectations. The generator sits beside its committed outputs (#256).
"""
import csv
from datetime import date, timedelta
import hashlib
import json
from pathlib import Path
import sys
from reportlab.pdfgen import canvas
from reportlab.lib.pagesizes import A4


def money(pence):
    return f"{pence // 100}.{pence % 100:02d}"


def generate(out):
    out.mkdir(parents=True, exist_ok=False)
    rows, balance = [], 1000000
    for i in range(2400):
        day = date(2025, 1, 1) + timedelta(days=i * 365 // 2400)
        amount = 100 + (i * 37) % 1900
        credit = i % 20 == 0
        amount = 25000 if credit else amount
        balance += amount if credit else -amount
        rows.append((day, f"SYNTHETIC SHOP {i:04d}", amount, credit, balance))
    with (out / "busy-year.csv").open("w", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow(["Date", "Description", "Amount"])
        writer.writerows((day.isoformat(), text, ("" if credit else "-") + money(amount))
                         for day, text, amount, credit, _ in rows)
    pdf = canvas.Canvas(str(out / "busy-year.pdf"), pagesize=A4, invariant=1)
    for start in range(0, len(rows), 40):
        pdf.setFont("Helvetica", 10)
        for x, text in [(35,"Date"),(115,"Description"),(330,"Paid Out"),(405,"Paid In"),(480,"Balance")]:
            pdf.drawString(x, 795, text)
        for i, (day, text, amount, credit, current) in enumerate(rows[start:start+40]):
            y = 772 - i * 17
            pdf.drawString(35, y, day.strftime("%d/%m/%Y"))
            pdf.drawString(115, y, text)
            pdf.drawString(405 if credit else 330, y, money(amount))
            pdf.drawString(480, y, money(current))
        pdf.showPage()
    pdf.save()
    sha = lambda p: "sha256:" + hashlib.sha256(p.read_bytes()).hexdigest()
    expected = {"schema": "kettle/large-acquisition@1", "synthetic": True, "rows": len(rows), "pages": 60,
        "credits": sum(r[3] for r in rows), "debits": sum(not r[3] for r in rows),
        "net_pence": sum(r[2] if r[3] else -r[2] for r in rows),
        "scope": "Statement parsing, no model accuracy or duration prediction; letter pack still limited to three pages",
        "generator": sha(Path(__file__)), "files": {p.name: sha(p) for p in sorted(out.iterdir())}}
    (out / "expected.json").write_text(json.dumps(expected, indent=2)+"\n")


if __name__ == "__main__":
    generate(Path(sys.argv[1]))
