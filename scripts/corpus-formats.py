#!/usr/bin/env python3
"""Validate paired assets and exercise the real acquisition routes without weights."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

PROJECT = Path(__file__).resolve().parents[1]
ARMS = ("text", "pdf", "photos")


def validate(bundle):
    generation = json.loads((bundle / "generation.json").read_text())
    if (generation.get("schema_version") != 1 or generation.get("purpose") != "acquisition-diagnostic"
            or generation.get("exposure") != "development" or generation.get("synthetic") is not True
            or generation.get("independent") is not False):
        raise ValueError("expected an exposed synthetic acquisition diagnostic")
    actual = {str(p.relative_to(bundle)) for p in bundle.rglob("*") if p.is_file() and p != bundle / "generation.json"}
    if actual != set(generation["files"]):
        raise ValueError("paired asset set changed; regenerate before evaluating")
    for name, expected in generation["files"].items():
        path = bundle / name
        if path.resolve().is_relative_to(bundle.resolve()) is False or path.is_symlink():
            raise ValueError("asset path escapes the bundle")
        if "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError(f"paired asset changed: {name}")
    corpus = json.loads((bundle / "corpus.json").read_text())
    case_ids = {c["id"] for c in corpus["cases"]}
    if len(case_ids) != len(corpus["cases"]) or set(generation["page_plans"]) != case_ids:
        raise ValueError("page plans must cover every case exactly once")
    for arm in ARMS:
        bindings = json.loads((bundle / f"{arm}.bindings.json").read_text())
        if set(bindings) != case_ids:
            raise ValueError(f"{arm}: missing or extra case binding")
        for case in corpus["cases"]:
            files = bindings[case["id"]]["inputs"]["letter"]
            pages = generation["page_plans"][case["id"]]["pages"]
            if [i for page in pages for i in page] != list(range(len(case["passages"]))):
                raise ValueError("page plan lost or reordered source passages")
            if len(files) != (1 if arm == "pdf" else len(pages)) or any(f not in generation["files"] for f in files):
                raise ValueError(f"{arm}: input count/files disagree with authored pages")
            if arm == "text":
                for file, page in zip(files, pages):
                    if (bundle / file).read_text().split() != " ".join(case["passages"][i] for i in page).split():
                        raise ValueError("text binding disagrees with authored page content/order")
    return generation, corpus


def run(bundle, out, kettle, sidecars):
    generation, corpus = validate(bundle)
    out.mkdir(parents=True, exist_ok=False)
    results = {}
    for arm in ARMS:
        destination = out / arm
        command = [str(kettle.resolve()), "corpus", "--corpus", str((bundle / "corpus.json").resolve()),
                   "--bindings", str((bundle / f"{arm}.bindings.json").resolve()),
                   "--pack-dir", str(PROJECT / "packs/app.kttl.letter-to-actions"),
                   "--sidecars-dir", str(sidecars.resolve()), "--no-model", "--out", str(destination.resolve())]
        result = subprocess.run(command, capture_output=True, text=True, check=False)
        path = destination / "report.json"
        if not path.is_file():
            raise ValueError(f"{arm}: diagnostic failed before recording a report: {result.stderr.strip()}")
        report = json.loads(path.read_text())
        if result.returncode not in (0, 2) or report.get("answer_source") != "deterministic-floor":
            raise ValueError("acquisition check must retain a deterministic floor report")
        results[arm] = {"report": f"{arm}/report.json", "unscored_cases": report["unscored_cases"],
                       "cases": [{"case": c["case"], "acquisition_errors": c["acquisition_errors"],
                                  "execution_error": c["execution_error"], "scored": c["score"] is not None,
                                  "pages_with_text": sorted({s["page"] for s in c["segments"]})} for c in report["cases"]]}
    report = {"schema": "kettle/acquisition-diagnostic@1", "selection": corpus["selection"],
              "generation_digest": "sha256:" + hashlib.sha256((bundle / "generation.json").read_bytes()).hexdigest(),
              "page_plans": generation["page_plans"], "answer_source": "deterministic-floor", "model_measured": 0,
              "arms": results}
    (out / "formats-report.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, default=PROJECT / "evals/corpus/formats-01")
    parser.add_argument("--out", type=Path, help="new directory; omitted validates assets only")
    parser.add_argument("--kettle", type=Path, default=PROJECT / "target/debug/kettle")
    parser.add_argument("--sidecars-dir", type=Path, default=PROJECT / "sidecars")
    args = parser.parse_args()
    try:
        if args.out:
            result = run(args.bundle, args.out, args.kettle, args.sidecars_dir)
            print(json.dumps(result, indent=2))
            return 2 if any(a["unscored_cases"] for a in result["arms"].values()) else 0
        generation, corpus = validate(args.bundle)
        print(f"Validated {len(corpus['cases'])} cases across {len(ARMS)} formats; {len(generation['files'])} pinned assets; no model measurement.")
        return 0
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(error, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
