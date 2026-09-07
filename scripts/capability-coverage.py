#!/usr/bin/env python3
"""Report declared, executable inventory checks, not test-run or model verdicts.

Run the owning Rust tests to validate the declared outcomes. A check accepting
a supplied name or money token does not establish correct semantic selection.
"""
import json
import sys
from collections import Counter
from pathlib import Path

PROJECT = Path(__file__).resolve().parent.parent
ROOT = PROJECT / "evals/capabilities"
VERIFIER_COVERAGE = {
    "main-check",       # this inventory case has an owning executable check
    "not-exercised",    # a boundary exists, but this case has no executable check
    "model-judgement",
    "unsupported",
    "none",
}
MODEL_COVERAGE = {"not-measured", "measured"}
READING_EXPECTATIONS = {
    "supported", "supported-with-warning", "not-a-sum", "absent",
    "accepted-misread",
}
DATE_OWNER = ("crates/runner/tests/reading_vocabulary.rs",
              "every_surface_form_reads_as_the_table_says")
READING_OWNER = ("crates/runner/tests/inventory_verifier.rs",
                 "every_checked_case_comes_out_as_the_inventory_says")


def load():
    return [(p.name, json.loads(p.read_text()))
            for p in sorted(ROOT.glob("*-forms.json"))]


def case_check(case, project, problems):
    """Only the two inventory-consuming tests establish case-level coverage."""
    verifier = case["verifier"]
    spec = verifier.get("check")
    owner = None
    expectation = None
    if case["capability"] == "dates-and-periods":
        owner = DATE_OWNER
        if not isinstance(verifier.get("proposal"), dict):
            problems.append(f"{case['id']}: date check needs an authored proposal")
        expectation = ("resolved-date" if case["source_truth"].get("date")
                       else "no-derived-date")
    elif isinstance(spec, dict):
        owner = READING_OWNER
        expectation = spec.get("expected")
        if expectation not in READING_EXPECTATIONS:
            problems.append(f"{case['id']}: unknown reading expectation {expectation!r}")
        if spec.get("kind") not in {"Money", "Name"}:
            problems.append(f"{case['id']}: unsupported check kind")
        if not isinstance(spec.get("value"), str):
            problems.append(f"{case['id']}: check needs a reading value")
    test = verifier.get("test")
    if owner is None:
        if test is not None or verifier["coverage"] == "main-check":
            problems.append(f"{case['id']}: claimed check has no executable inventory adapter")
        return None
    if not isinstance(test, dict):
        problems.append(f"{case['id']}: executable case needs its owning test and scope")
        return None
    if (test.get("file"), test.get("name")) != owner:
        problems.append(f"{case['id']}: check names the wrong owning test")
    else:
        path = project / owner[0]
        if not path.is_file() or f"fn {owner[1]}(" not in path.read_text():
            problems.append(f"{case['id']}: owning test does not exist")
    if not isinstance(test.get("scope"), str) or not test["scope"].strip():
        problems.append(f"{case['id']}: check must state its scope")
    if verifier["coverage"] not in {"main-check", "unsupported"}:
        problems.append(f"{case['id']}: executable check has stale coverage")
    if expectation in {"accepted-misread", "not-a-sum"}:
        if verifier["coverage"] != "unsupported" or not verifier.get("known_gap"):
            problems.append(f"{case['id']}: a known limitation cannot be labelled supported")
    return {
        "id": case["id"],
        "test": test,
        "expected": expectation,
        "known_gap": verifier.get("known_gap"),
    }


def build_report(inventories, project=PROJECT):
    report = {"files": [], "totals": {}}
    all_ids = Counter()
    problems = []
    all_checks = []
    for name, inventory in inventories:
        cases = inventory["cases"]
        checks = []
        for case in cases:
            all_ids[case["id"]] += 1
            v = case["verifier"]["coverage"]
            m = case["model_reading"]["coverage"]
            if v not in VERIFIER_COVERAGE:
                problems.append(f"{case['id']}: unknown verifier coverage {v!r}")
            if m not in MODEL_COVERAGE:
                problems.append(f"{case['id']}: unknown model coverage {m!r}")
            if v in {"unsupported", "none"} and not case["verifier"].get("known_gap"):
                problems.append(f"{case['id']}: missing known gap")
            src = case["provenance"]["independent_source"]
            if src != "pending" and not (isinstance(src, dict) and src.get("locator") and src.get("quoted_rule")):
                problems.append(f"{case['id']}: independent_source needs a locator and quoted rule")
            check = case_check(case, project, problems)
            if check:
                checks.append(check)
        all_checks.extend(checks)
        report["files"].append({
            "file": name,
            "capability": inventory["capability"],
            "cases": len(cases),
            "source_outcomes": dict(Counter(c["source_truth"]["status"] for c in cases)),
            "verifier_coverage": dict(Counter(c["verifier"]["coverage"] for c in cases)),
            "verifier_cases_with_checks": len(checks),
            "verifier_cases_without_checks": len(cases) - len(checks),
            "verifier_check_expectations": dict(Counter(c["expected"] for c in checks)),
            "verifier_checks": checks,
            "model_reading_coverage": dict(Counter(c["model_reading"]["coverage"] for c in cases)),
            "independently_sourced": sum(c["provenance"]["independent_source"] != "pending" for c in cases),
            "known_verifier_gaps": [c["id"] for c in cases if c["verifier"]["known_gap"]],
        })
    duplicates = [i for i, n in all_ids.items() if n > 1]
    if duplicates:
        problems.append(f"duplicate case ids across files: {duplicates}")
    if problems:
        raise ValueError("\n".join(problems))
    cases = [c for _, inv in inventories for c in inv["cases"]]
    report["totals"] = {
        "cases": len(cases),
        "verifier_cases_with_checks": len(all_checks),
        "verifier_cases_without_checks": len(cases) - len(all_checks),
        "verifier_check_expectations": dict(Counter(c["expected"] for c in all_checks)),
        "verifier_checks_with_known_gaps": sum(bool(c["known_gap"]) for c in all_checks),
        "model_measured": sum(c["model_reading"]["coverage"] == "measured" for c in cases),
        "independently_sourced": sum(c["provenance"]["independent_source"] != "pending" for c in cases),
        "known_verifier_gaps": sum(bool(c["verifier"]["known_gap"]) for c in cases),
    }
    return report


def main():
    try:
        report = build_report(load())
    except ValueError as error:
        print(error, file=sys.stderr)
        return 1
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
