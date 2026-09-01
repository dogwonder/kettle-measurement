#!/usr/bin/env python3
"""Read every letter of the study corpus against the bed's expected answer.

The same reason `audit.py` exists for the statements: a "clean" letter is
only clean once somebody has checked what the pipeline proposed against
what the letter asks. A participant who catches a natural error in a
clean control would otherwise be scored as a false alarm while being
right.

Writes `audit-letters.json` beside the corpus and prints the counts.
This reads shape — party, due date, count — and flags the rest for a
person; it does not decide whether a proposed ask is a fair reading.
"""
import json
import pathlib
import sys

here = pathlib.Path(__file__).parent
letters = sorted((here / "letters").glob("letter-*.json"))
if not letters:
    sys.exit("no letters in fixtures/study/letters — run the study_letters example first")

audit = {}
totals = {"letters": 0, "actions": 0, "expected": 0, "matched": 0, "extra": 0, "missed": 0, "date_differs": 0}
for path in letters:
    letter = json.loads(path.read_text())
    actions = letter["actions"]["actions"]
    expected = letter["expected"]
    notes = []
    used = set()
    matched = 0
    for exp in expected:
        # An action matches an expectation if it was asked by the same
        # party and lands on the same date (or both are undated).
        hit = None
        for i, act in enumerate(actions):
            if i in used:
                continue
            due = (act.get("export", {}).get("ics") or {}).get("date")
            if act["evidence"].get("asked_by") == exp["party"] and due == exp["due"]:
                hit = i
                break
        if hit is None:
            # Same party, different date: a date the pipeline got wrong.
            for i, act in enumerate(actions):
                if i in used:
                    continue
                if act["evidence"].get("asked_by") == exp["party"]:
                    due = (act.get("export", {}).get("ics") or {}).get("date")
                    notes.append(f"date differs: expected {exp['due']} for {exp['id']}, proposed {due} on '{act['title']}'")
                    totals["date_differs"] += 1
                    used.add(i)
                    hit = i
                    break
        if hit is None:
            notes.append(f"missed: {exp['id']} ({exp['deadline']}, due {exp['due']})")
            totals["missed"] += 1
        else:
            used.add(hit)
            matched += 1
    for i, act in enumerate(actions):
        if i not in used:
            notes.append(f"extra: '{act['title']}' — {act['evidence'].get('in_the_letter')} (not in the bed's expectations)")
            totals["extra"] += 1
    totals["letters"] += 1
    totals["actions"] += len(actions)
    totals["expected"] += len(expected)
    totals["matched"] += matched
    audit[letter["id"]] = {
        "source": letter["source"]["file"],
        "actions": len(actions),
        "expected": len(expected),
        "notes": notes,
        # A person's read of whether each note is an error the pipeline
        # made or a fair ask the bed did not list. Blank until read.
        "read_by": None,
        "clean": None,
    }

(here / "audit-letters.json").write_text(json.dumps({"totals": totals, "letters": audit}, indent=2) + "\n")
print(" ".join(f"{k} {v}" for k, v in totals.items()))
for id_, entry in audit.items():
    for note in entry["notes"]:
        print(f"{id_}: {note}")
