#!/usr/bin/env python3
"""Adjudicate existing local runs; export only counts and closed shape categories.

Use the packaged app to run a private document. This workflow neither sends
documents elsewhere nor infers correctness from accepting a proposed action.
"""
import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import sys
import uuid

OUTCOMES = ("correct", "wrong", "incomplete", "unsupported", "uncheckable")
FORMATS = ("text", "pdf", "photo", "mixed")
STRUCTURES = ("prose", "table", "list", "multi-page", "mixed")
COVERAGE = ("ask-presence", "conditional-ask", "amount-selection", "date-reading",
            "party-reading", "evidence-location", "page-order", "acquisition", "other")


def read(path):
    return json.loads(path.read_text())


def private_path(path):
    if not path.name.endswith(".private.json") or path.is_symlink():
        raise ValueError("use a non-symlink *.private.json observation")


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            h.update(chunk)
    return "sha256:" + h.hexdigest()


def evidence(root):
    if not root.is_dir() or not (root / "results.json").is_file():
        raise ValueError("a completed run with results.json is required")
    files = {}
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise ValueError("run evidence must not contain symlinks")
        if path.is_file():
            files[str(path.relative_to(root))] = digest(path)
    return files


def initialise(args):
    private_path(args.out)
    root = args.run_dir.resolve()
    if args.out.resolve().is_relative_to(root):
        raise ValueError("keep adjudication outside the immutable run directory")
    files = evidence(root)
    result = read(root / "results.json")
    provenance = read(args.provenance)
    if any(not isinstance(provenance.get(k), str) or not provenance[k].strip()
           for k in ("model", "pack", "runtime", "basis")):
        raise ValueError("declare model, pack, runtime and the basis for that provenance")
    # The declared runtime supplements the desktop receipt, which does not
    # retain a full sidecar digest. Preserve both; do not fabricate a pin.
    manifest = read(root / "manifest.json") if (root / "manifest.json").exists() else None
    claims = result.get("obligations", result.get("findings", []))
    if not isinstance(claims, list):
        raise ValueError("expected a report's obligation or finding list")
    record = {"schema": "kettle/private-field-evidence@1", "id": str(uuid.uuid4()),
              "created_at": datetime.now(timezone.utc).isoformat(), "run_dir": str(root),
              "evidence": files, "provenance": {"declared": provenance,
                  "run": result.get("run"), "manifest": manifest},
              "shape": {"format": args.format, "structure": args.structure, "coverage": args.coverage},
              "adjudications": [{"id": f"asserted-{i}", "target": "asserted", "outcome": None,
                                  "note": "", "reproducer": None, "issue": None}
                                 for i in range(len(claims))]}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    # Exclusive creation and owner-only permissions apply before private bytes
    # are written, including on machines with a permissive umask.
    fd = os.open(args.out, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    with os.fdopen(fd, "w") as out:
        json.dump(record, out, indent=2)
        out.write("\n")


def validated(path):
    private_path(path)
    data = read(path)
    if data.get("schema") != "kettle/private-field-evidence@1" or not data.get("id"):
        raise ValueError("invalid private observation")
    shape = data["shape"]
    if (set(shape) != {"format", "structure", "coverage"} or shape["format"] not in FORMATS
            or shape["structure"] not in STRUCTURES or shape["coverage"] not in COVERAGE):
        raise ValueError("shape metadata must use the closed categories")
    if evidence(Path(data["run_dir"])) != data["evidence"]:
        raise ValueError("run evidence changed; retain the original run before adjudicating")
    ids = set()
    for item in data["adjudications"]:
        if (not isinstance(item.get("id"), str) or not item["id"] or item["id"] in ids
                or item.get("target") not in ("asserted", "missing")
                or item.get("outcome") not in (None, *OUTCOMES)):
            raise ValueError("adjudications require distinct ids, targets and explicit outcomes")
        ids.add(item["id"])
        if item["target"] == "missing" and item["outcome"] not in (None, "incomplete", "uncheckable"):
            raise ValueError("a missing claim can be incomplete or uncheckable, never a correct assertion")
        if not isinstance(item.get("note", ""), str):
            raise ValueError("adjudication notes must be text")
        for key in ("reproducer", "issue"):
            value = item.get(key)
            if value is not None and (not isinstance(value, str) or not value.strip()):
                raise ValueError("discovery links must be nonempty locators or null")
    return data


def aggregate(paths):
    outcomes = Counter({key: 0 for key in OUTCOMES})
    formats, structures, coverage = Counter(), Counter(), Counter()
    seen, runs = set(), set()
    unreviewed = missing = without_reproducer = without_issue = 0
    for path in paths:
        data = validated(path)
        run = str(Path(data["run_dir"]).resolve())
        if data["id"] in seen or run in runs:
            raise ValueError("duplicate observation/run would double count field evidence")
        seen.add(data["id"])
        runs.add(run)
        formats[data["shape"]["format"]] += 1
        structures[data["shape"]["structure"]] += 1
        coverage[data["shape"]["coverage"]] += 1
        for item in data["adjudications"]:
            outcome = item["outcome"]
            if outcome is None:
                unreviewed += 1
                continue
            outcomes[outcome] += 1
            missing += item["target"] == "missing"
            if outcome != "correct":
                without_reproducer += item.get("reproducer") is None
                without_issue += item.get("issue") is None
    # Construct a new object; never redact/copy arbitrary private dictionaries.
    return {"schema": "kettle/field-summary@1", "observations": len(seen),
            "outcomes": dict(outcomes), "unreviewed": unreviewed, "missing_claims": missing,
            "discoveries_without_reproducer": without_reproducer,
            "discoveries_without_issue": without_issue, "formats": dict(formats),
            "structures": dict(structures), "coverage": dict(coverage)}


def registry(paths, out):
    private_path(out)
    aggregate(paths)  # Same evidence validation and duplicate refusal as export.
    discoveries = []
    for path in paths:
        data = validated(path)
        for item in data["adjudications"]:
            if item["outcome"] not in (None, "correct"):
                discoveries.append({"observation": str(path.resolve()), "id": item["id"],
                    "coverage": data["shape"]["coverage"], "outcome": item["outcome"],
                    "reproducer": item.get("reproducer"), "issue": item.get("issue"),
                    "needs_reproducer": item.get("reproducer") is None,
                    "needs_issue": item.get("issue") is None})
    out.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(out, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    with os.fdopen(fd, "w") as stream:
        json.dump({"schema": "kettle/private-field-registry@1", "discoveries": discoveries}, stream, indent=2)
        stream.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    init = sub.add_parser("init", help="create a private draft beside an existing app run")
    init.add_argument("--run-dir", type=Path, required=True)
    init.add_argument("--provenance", type=Path, required=True)
    init.add_argument("--format", choices=FORMATS, required=True)
    init.add_argument("--structure", choices=STRUCTURES, required=True)
    init.add_argument("--coverage", choices=COVERAGE, required=True)
    init.add_argument("--out", type=Path, required=True)
    for name in ("export", "registry"):
        command = sub.add_parser(name, help="print counts only; discovery details stay in private observations")
        command.add_argument("--record", type=Path, action="append", required=True)
        if name == "registry":
            command.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "init":
            initialise(args)
            print("Private draft created. Review each important claim; add missing claims explicitly.")
        elif args.command == "registry":
            registry(args.record, args.out)
            print("Private discovery registry written; follow up missing reproducer and issue links locally.")
        else:
            print(json.dumps(aggregate(args.record), indent=2))
        return 0
    except (OSError, ValueError, KeyError, TypeError):
        # Private filenames, JSON decode excerpts and arbitrary enum values
        # must not leak through an error intended for a shareable command.
        print("Field evidence refused: check private record, closed categories, distinct adjudications and unchanged run evidence.", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
