#!/usr/bin/env python3
"""Describe authored coverage; never infer a pass from the presence of a case.

Reads every `*-forms.json` inventory under evals/capabilities and reports, per
capability family, what the source truth says, how far the deterministic
verifier reaches, and whether any model has read the example. The three are
kept apart on purpose: a form the verifier cannot check, or nobody has
measured, is listed as exactly that, never as a pass.
"""
import json
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "evals/capabilities"

# The vocabulary each field may use. A value outside it is a defect in the
# inventory, not a new kind of coverage.
VERIFIER_COVERAGE = {
    "main-check",        # a deterministic check on main exercises this form
    "pending-v19-test",  # the proposed #628 boundary maps it; no test yet
    "model-judgement",   # only the model's closed answer decides it
    "unsupported",       # the verifier reaches it and refuses or misreads
    "none",              # no field carries it
}
MODEL_COVERAGE = {"not-measured", "measured"}
PASSING_VERIFIER = {"main-check"}


def load():
    files = sorted(ROOT.glob("*-forms.json"))
    inventories = [(path.name, json.loads(path.read_text())) for path in files]
    return inventories


def main() -> int:
    inventories = load()
    report = {"files": [], "totals": {}}
    all_ids = Counter()
    problems = []
    for name, inventory in inventories:
        cases = inventory["cases"]
        for case in cases:
            all_ids[case["id"]] += 1
            v = case["verifier"]["coverage"]
            m = case["model_reading"]["coverage"]
            if v not in VERIFIER_COVERAGE:
                problems.append(f"{case['id']}: verifier coverage {v!r} is not in the vocabulary")
            if m not in MODEL_COVERAGE:
                problems.append(f"{case['id']}: model coverage {m!r} is not in the vocabulary")
            src = case["provenance"]["independent_source"]
            if src != "pending" and not (isinstance(src, dict) and src.get("locator") and src.get("quoted_rule")):
                problems.append(f"{case['id']}: independent_source must be 'pending' or carry locator and quoted_rule")
        report["files"].append({
            "file": name,
            "capability": inventory["capability"],
            "cases": len(cases),
            "source_outcomes": dict(Counter(c["source_truth"]["status"] for c in cases)),
            "verifier_coverage": dict(Counter(c["verifier"]["coverage"] for c in cases)),
            "model_reading_coverage": dict(Counter(c["model_reading"]["coverage"] for c in cases)),
            "independently_sourced": sum(c["provenance"]["independent_source"] != "pending" for c in cases),
            "known_verifier_gaps": [c["id"] for c in cases if c["verifier"]["known_gap"]],
        })
    duplicates = [i for i, n in all_ids.items() if n > 1]
    if duplicates:
        problems.append(f"duplicate case ids across files: {duplicates}")
    cases = [c for _, inv in inventories for c in inv["cases"]]
    report["totals"] = {
        "cases": len(cases),
        "verifier_checked_on_main": sum(c["verifier"]["coverage"] in PASSING_VERIFIER for c in cases),
        "verifier_not_checked": sum(c["verifier"]["coverage"] not in PASSING_VERIFIER for c in cases),
        "model_measured": sum(c["model_reading"]["coverage"] == "measured" for c in cases),
        "independently_sourced": sum(c["provenance"]["independent_source"] != "pending" for c in cases),
        "known_verifier_gaps": sum(bool(c["verifier"]["known_gap"]) for c in cases),
    }
    print(json.dumps(report, indent=2))
    if problems:
        print("\n".join(problems), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
