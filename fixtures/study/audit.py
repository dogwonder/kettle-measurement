#!/usr/bin/env python3
"""Audit the ten study reports against the statements that produced them (#431).

Decided 25 August 2026: the study's reports are *genuine* pipeline
output, not authored, because any error Kettle makes on its own tells
the study more than a corpus with none. The price of that decision is
this file. A "clean" report is only clean once somebody has read it
against its statement — a participant who catches a natural error in a
clean control would otherwise be scored as a false alarm while being
right, which inverts the measure the clean pair exists to give.

So every finding is checked against what the generator planted, and
every planted commitment is checked for a finding. What comes out is
three lists per report — matched, missed, unexpected — and a corpus
where each report is labelled with what it actually contains.

A commitment paid fewer than three times is not a recurrence and its
absence is not a miss: one yearly payment is a fact about the year, not
a series. Those are counted separately as `not-a-series`.

    python3 fixtures/study/audit.py
"""

import json
from pathlib import Path

import importlib.util

HERE = Path(__file__).parent

spec = importlib.util.spec_from_file_location("make", HERE / "make-statements.py")
make = importlib.util.module_from_spec(spec)
spec.loader.exec_module(make)

MIN_PAYMENTS_FOR_A_SERIES = 3

# Read by hand, 25 August 2026, against `classify.schema.json`'s own
# enum. Every one of these is a label the pipeline chose when a plainly
# better member of its own vocabulary was available — so they are the
# pipeline's natural errors, not disagreements about taste.
#
# They matter to the study for one reason: a participant who flags one
# of these in a *clean* report is right, and would otherwise be scored
# as a false alarm. Recorded here so scoring can tell the two apart.
LABEL_ERRORS = {
    "report-01": [("Castle Hill Lettings", "other", "housing", "rent", "medium")],
    "report-04": [
        ("Briarwood Halls", "other", "housing", "student halls", "medium"),
        ("Campus Sport", "other", "fitness", "a sport membership", "medium"),
    ],
    "report-06": [("Card Fees", "other", "finance", "card processing fees", "medium")],
    "report-07": [("Meadowbank Lettings", "other", "housing", "rent", "medium")],
    "report-10": [
        ("Borough Council Tax", "other", "housing", "council tax", "medium"),
        (
            "Pharmacy Delivery Service",
            "food_drink",
            "other",
            "a pharmacy is not food or drink — and this one is high confidence",
            "high",
        ),
    ],
}


def planted(statement) -> list[dict]:
    """What the generator put in, as a list of expected series."""
    out = []
    for item in statement["commitments"]:
        payments = len(item["months"])
        amount = item["amount"]
        rise = item["rise"]
        current = rise[1] if rise is not None else amount
        period = (
            "monthly"
            if item["months"] == make.MONTHLY
            else "quarterly"
            if item["months"] == make.QUARTERLY
            else "yearly"
            if item["months"] == make.YEARLY
            else "irregular"
        )
        out.append(
            {
                "descriptor": item["descriptor"],
                "payments": payments,
                "period": period,
                "current": current,
                "rise": None if rise is None else {"month": rise[0], "to": rise[1], "from": amount},
                "variants": list((item["variants"] or {}).values()),
            }
        )
    return out


def words(text: str) -> set[str]:
    """Distinctive words, punctuation stripped.

    Stripping matters: the model normalises `DISNEY PLUS` to `Disney+`,
    and an audit that compared raw tokens would report a miss and an
    unexpected finding for the same correctly-read series — a defect in
    the auditor read as two defects in the pipeline.
    """
    cleaned = "".join(c if c.isalnum() else " " for c in text.upper())
    return {w for w in cleaned.split() if len(w) > 2}


def matches(expected: dict, merchant: str) -> bool:
    """A finding is this commitment if they share a distinctive word."""
    candidates = [expected["descriptor"], *expected["variants"]]
    found = words(merchant)
    return any(words(candidate) & found for candidate in candidates)


def audit() -> None:
    totals = {"matched": 0, "missed": 0, "unexpected": 0, "wrong": 0, "not-a-series": 0}
    labels = {}

    for index, statement in enumerate(make.STATEMENTS, start=1):
        report_path = HERE / f"report-{index:02d}.json"
        if not report_path.exists():
            print(f"report-{index:02d}: missing")
            continue
        report = json.loads(report_path.read_text(encoding="utf-8"))
        findings = report.get("recurring", [])
        expected = planted(statement)

        notes: list[str] = []
        claimed = set()

        for want in expected:
            hit = next(
                (f for i, f in enumerate(findings) if i not in claimed and matches(want, f["merchant"])),
                None,
            )
            if hit is not None:
                claimed.add(findings.index(hit))
            if hit is None:
                if want["payments"] < MIN_PAYMENTS_FOR_A_SERIES:
                    totals["not-a-series"] += 1
                else:
                    totals["missed"] += 1
                    notes.append(f"MISSED {want['descriptor']} ({want['payments']} payments)")
                continue

            totals["matched"] += 1
            problems = []
            if abs(float(hit["amount_current"]) - want["current"]) > 0.005:
                problems.append(f"amount {hit['amount_current']} not {want['current']:.2f}")
            if want["period"] != "irregular" and hit["period"] != want["period"]:
                problems.append(f"period {hit['period']} not {want['period']}")
            if want["rise"] is None and hit.get("price_rise") is not None:
                problems.append("a price rise that was never planted")
            if want["rise"] is not None:
                rise = hit.get("price_rise")
                if rise is None:
                    problems.append(f"no price rise, planted {want['rise']['to']:.2f} in month {want['rise']['month']}")
                else:
                    if abs(float(rise["to"]) - want["rise"]["to"]) > 0.005:
                        problems.append(f"rise to {rise['to']} not {want['rise']['to']:.2f}")
                    if int(rise["month"].split("-")[1]) != want["rise"]["month"]:
                        problems.append(f"rise in {rise['month']} not month {want['rise']['month']}")
            if problems:
                totals["wrong"] += 1
                notes.append(f"WRONG {hit['merchant']}: " + "; ".join(problems))

        for i, finding in enumerate(findings):
            if i not in claimed:
                totals["unexpected"] += 1
                notes.append(
                    f"UNEXPECTED {finding['merchant']} "
                    f"{finding['period']} {finding['amount_current']}"
                )

        name = f"report-{index:02d}"
        for merchant, chose, better, why, confidence in LABEL_ERRORS.get(name, []):
            notes.append(
                f"LABEL {merchant}: category {chose}, better {better} ({why}), "
                f"shown at {confidence} confidence"
            )
            totals["label"] = totals.get("label", 0) + 1
        labels[name] = notes
        state = "clean" if not notes else f"{len(notes)} note(s)"
        print(f"report-{index:02d} ({statement['note'][:38]:<38}) {len(findings)} findings — {state}")
        for note in notes:
            print(f"    {note}")

    print()
    print(
        f"matched {totals['matched']}  wrong {totals['wrong']}  "
        f"missed {totals['missed']}  unexpected {totals['unexpected']}  "
        f"not-a-series {totals['not-a-series']}  label {totals.get('label', 0)}"
    )
    (HERE / "audit.json").write_text(json.dumps(labels, indent=1) + "\n", encoding="utf-8")


if __name__ == "__main__":
    audit()
