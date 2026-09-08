#!/usr/bin/env python3
"""Report scoped inventory checks and optionally replayed model attempts.

Run the owning Rust tests to validate the declared outcomes. A check accepting
a supplied name or money token does not establish correct semantic selection.
"""
import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
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
SCHEMA = "kettle/corpus-evaluation@1"
SCORING = "corpus-fields-v6"


def diagnostic_links(path, inventory_dir=ROOT):
    """Pin each selected case to the registry's actual document and snapshot."""
    corpus = json.loads(path.read_text())
    selection = corpus.get("selection", {})
    fields = set(selection.get("fields", []))
    if (selection.get("purpose") != "diagnostic"
            or selection.get("exposure") != "development"
            or not selection.get("id") or not fields
            or not fields <= {"kind", "party", "deadline", "amount"}):
        raise ValueError("coverage requires an exposed diagnostic selection")
    links = {}
    case_ids = set()
    for case in corpus["cases"]:
        if case["id"] in case_ids:
            raise ValueError("duplicate diagnostic case")
        case_ids.add(case["id"])
        for link in case.get("coverage", []):
            name = link["inventory_file"]
            if Path(name).name != name or name in {".", ".."}:
                raise ValueError("inventory links must name a direct child file")
            data = (inventory_dir / name).read_bytes()
            inventory = json.loads(data)
            source = next((c for c in inventory["cases"]
                           if c["id"] == link["inventory_id"]), None)
            if (not source or link["inventory_id"] in links
                    or "sha256:" + hashlib.sha256(data).hexdigest() != link["inventory_digest"]
                    or inventory["capability"] != link["capability"]
                    or source["model_reading"]["case_id"] != case["id"]
                    or source["model_reading"]["document"].split() != " ".join(case["passages"]).split()
                    or not link["fields"] or not link["scope"].strip()):
                raise ValueError(f"stale, duplicate or mismatched inventory link: {link['inventory_id']}")
            links[link["inventory_id"]] = {
                **link, "case": case["id"],
                "supported_scope": set(link["fields"]) <= fields,
            }
    if not links:
        raise ValueError("diagnostic selection has no inventory links")
    return corpus, links


def checked_evidence(corpus, links, report, recording):
    """Only consume a freshly recomputed report from the exact replay route.

    This is not a signature or an attestation of who produced a recording.
    The CLI checks requests on disk; this function bounds the coverage claim.
    """
    if (report.get("schema") != SCHEMA or report.get("scoring") != SCORING
            or report.get("answer_source") != "replay"
            or report.get("unscored_cases") != 0
            or report.get("selection") != corpus["selection"]
            or set(report.get("fields", [])) != set(corpus["selection"]["fields"])
            or not (report.get("model") or {}).get("weights_digest")
            or not report.get("runtime") or not report.get("sidecar")
            or not report.get("generation_machine")
            or not report.get("replay", {}).get("exact_requests")
            or report["replay"].get("legacy_prompt_only_requests") != 0):
        raise ValueError("compatible exact replay with identified model provenance is required")
    actual = {c["case"]: c for c in report["cases"]}
    if len(actual) != len(report["cases"]) or set(actual) != {c["id"] for c in corpus["cases"]}:
        raise ValueError("replay does not cover the current diagnostic selection")
    for case in corpus["cases"]:
        result = actual[case["id"]]
        if (result.get("coverage") != case.get("coverage", [])
                or result.get("score") is None or result.get("execution_error")
                or result.get("acquisition_errors") or result.get("attribution_errors") or not result.get("exchanges")
                or any(not e.get("generation") for e in result["exchanges"])):
            raise ValueError(f"{case['id']}: no complete compatible recorded attempt")
    return {ident: {
        "recording": str(recording), "case": link["case"],
        "fields": link["fields"], "scope": link["scope"],
        "model": report["model"], "scoring": report["scoring"],
        "corpus_digest": report["corpus_digest"],
        "score": actual[link["case"]]["score"],
    } for ident, link in links.items() if link["supported_scope"]}


def replay_evidence(path, corpus, links, recording, kettle, project=PROJECT):
    previous = json.loads((recording / "report.json").read_text())
    if (previous.get("answer_source") not in {"model", "replay"}
            or not (previous.get("model") or {}).get("weights_digest")):
        raise ValueError("controlled endpoints and no-model runs cannot establish model evidence")
    requests = list(recording.rglob("*.generation.json"))
    if not requests:
        raise ValueError("recording has no exact generation files")
    # The generic replay loader can describe a mixture of old anonymous
    # recordings and identified ones. That is insufficient for a positive
    # coverage claim: every contributing run must identify these weights.
    for request in requests:
        manifest = json.loads((request.parent.parent / "run.json").read_text())
        if request.parent.name != "raw" or manifest.get("model") != previous["model"]:
            raise ValueError("every recorded exchange must identify the reported model")
    pack = previous.get("pack", "")
    if not pack or Path(pack).name != pack or pack in {".", ".."}:
        raise ValueError("recording must identify a local pack")
    with tempfile.TemporaryDirectory(prefix="kettle-coverage-") as temporary:
        output = Path(temporary) / "checked"
        result = subprocess.run([
            str(kettle.resolve()), "corpus", "--corpus", str(path.resolve()),
            "--inventory-dir", str(project / "evals/capabilities"),
            "--pack-dir", str(project / "packs" / pack),
            "--replay", str(recording.resolve()), "--out", str(output),
        ], capture_output=True, text=True, check=False)
        if result.returncode:
            raise ValueError("recording is incompatible with the current diagnostic: " + result.stderr.strip())
        report = json.loads((output / "report.json").read_text())
        return checked_evidence(corpus, links, report, recording)


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


def build_report(inventories, project=PROJECT, links=None, evidence=None):
    links, evidence = links or {}, evidence or {}
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
            elif m == "measured" and case["id"] not in evidence:
                problems.append(
                    f"{case['id']}: no compatible recording was validated; "
                    "a measured label cannot establish model evidence"
                )
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
            "model_reading_coverage": dict(Counter("measured" if c["id"] in evidence else "not-measured" for c in cases)),
            "diagnostic_selected": [c["id"] for c in cases if c["id"] in links],
            "diagnostic_unsupported_scope": [c["id"] for c in cases if c["id"] in links and not links[c["id"]]["supported_scope"]],
            "model_evidence": {c["id"]: evidence[c["id"]] for c in cases if c["id"] in evidence},
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
        "model_measured": sum(c["id"] in evidence for c in cases),
        "diagnostic_selected": sum(c["id"] in links for c in cases),
        "diagnostic_unsupported_scope": sum(c["id"] in links and not links[c["id"]]["supported_scope"] for c in cases),
        "independently_sourced": sum(c["provenance"]["independent_source"] != "pending" for c in cases),
        "known_verifier_gaps": sum(bool(c["verifier"]["known_gap"]) for c in cases),
    }
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path, default=PROJECT / "evals/corpus/diagnostic-01.json")
    parser.add_argument("--recording", type=Path)
    parser.add_argument("--kettle", type=Path, default=PROJECT / "target/release/kettle")
    args = parser.parse_args()
    try:
        corpus, links = diagnostic_links(args.corpus)
        evidence = replay_evidence(args.corpus, corpus, links, args.recording, args.kettle) if args.recording else {}
        report = build_report(load(), links=links, evidence=evidence)
        report["selection"] = corpus["selection"]
    except (ValueError, OSError, KeyError, TypeError) as error:
        print(error, file=sys.stderr)
        return 1
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
