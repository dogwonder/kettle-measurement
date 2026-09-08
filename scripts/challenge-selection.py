#!/usr/bin/env python3
"""Freeze a separately authored challenge identity and record its exposure.

This records an authoring declaration, not proof of independence. It never
runs a model or converts a challenge into a routine diagnostic.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sys

PROJECT = Path(__file__).resolve().parents[1]


def read_challenge(path, development=PROJECT / "evals/corpus"):
    data = path.read_bytes()
    corpus = json.loads(data)
    selection = corpus.get("selection", {})
    authoring = corpus.get("provenance", {}).get("authoring", {})
    sources = authoring.get("source_families", [])
    if (corpus.get("schema_version") != 1 or not corpus.get("cases")
            or selection.get("purpose") != "challenge" or selection.get("exposure") != "unexposed"
            or not selection.get("id") or not selection.get("fields")
            or not set(selection["fields"]) <= {"kind", "party", "deadline", "amount"}
            or corpus.get("provenance", {}).get("independent") is not True
            or not authoring.get("author") or authoring.get("relationship") != "separate-author"
            or not sources or any(not s.get("id") or not s.get("locator") or s.get("relationship") != "external-source" for s in sources)):
        raise ValueError("challenge needs an unexposed selection and a separate author's external-family declaration")
    family_ids = [s["id"] for s in sources]
    if len(set(family_ids)) != len(family_ids) or any(i.startswith("kettle-examples") for i in family_ids):
        raise ValueError("development-generator families cannot be declared independent")
    texts = [" ".join(" ".join(c["passages"]).lower().split()) for c in corpus["cases"]]
    ids = [c["id"] for c in corpus["cases"]]
    if len(set(ids)) != len(ids) or len(set(texts)) != len(texts):
        raise ValueError("challenge cases need distinct ids and documents")
    for candidate in development.rglob("*.json"):
        known = json.loads(candidate.read_text())
        if not isinstance(known, dict) or "cases" not in known or known.get("selection", {}).get("purpose") == "challenge":
            continue
        if known.get("selection", {}).get("id") == selection["id"]:
            raise ValueError("selection id already belongs to exposed development material")
        for case in known["cases"]:
            if case.get("id") in ids or " ".join(" ".join(case.get("passages", [])).lower().split()) in texts:
                raise ValueError("challenge reuses an exposed case; fresh wording/layout families are required")
    return corpus, "sha256:" + hashlib.sha256(data).hexdigest()


def now():
    return datetime.now(timezone.utc).isoformat()


def freeze(corpus_path, record_path, development=PROJECT / "evals/corpus"):
    record_path.parent.mkdir(parents=True, exist_ok=True)
    lock = record_path.parent / ".challenge-freeze.pending"
    # Serialise new records across the ledger, including different filenames.
    # A crash leaves the lock for investigation rather than a fresh identity.
    with lock.open("x"):
        try:
            return freeze_locked(corpus_path, record_path, development)
        finally:
            lock.unlink()


def freeze_locked(corpus_path, record_path, development):
    corpus, digest = read_challenge(corpus_path, development)
    if record_path.exists():
        raise FileExistsError(f"lifecycle record already exists: {record_path}")
    # Keep lifecycle records together. Renaming an exposed selection's record
    # must not accidentally make the same selection fresh within this ledger.
    for path in record_path.parent.glob("*.json"):
        previous = json.loads(path.read_text())
        if isinstance(previous, dict) and previous.get("schema") == "kettle/challenge-lifecycle@1":
            if previous.get("corpus_digest") == digest or previous.get("selection", {}).get("id") == corpus["selection"]["id"]:
                raise ValueError("selection already frozen in this ledger; retain its existing lifecycle and obtain a fresh challenge")
    record = {"schema": "kettle/challenge-lifecycle@1", "corpus_digest": digest,
              "selection": corpus["selection"], "authoring": corpus["provenance"]["authoring"],
              "cases": len(corpus["cases"]), "events": [{"event": "frozen", "at": now()}]}
    with record_path.open("x") as out:
        out.write(json.dumps(record, indent=2) + "\n")
    return record


def status(record):
    events = record.get("events", [])
    if (record.get("schema") != "kettle/challenge-lifecycle@1" or not events
            or events[0].get("event") != "frozen" or len(events) > 2
            or any(not e.get("at") for e in events)
            or (len(events) == 2 and (events[1].get("event") != "exposed"
                or not events[1].get("reason") or not events[1].get("evidence")))):
        raise ValueError("invalid challenge lifecycle")
    return "unexposed" if len(events) == 1 else "regression"


def check(corpus_path, record_path, development=PROJECT / "evals/corpus"):
    if record_path.with_name(record_path.name + ".pending").exists():
        raise ValueError("challenge lifecycle has a pending write; investigate before continuing")
    corpus, digest = read_challenge(corpus_path, development)
    record = json.loads(record_path.read_text())
    if (digest != record.get("corpus_digest") or corpus["selection"] != record.get("selection")
            or corpus["provenance"]["authoring"] != record.get("authoring")
            or len(corpus["cases"]) != record.get("cases")):
        raise ValueError("challenge content or selection changed after freezing")
    return status(record)


def expose(record_path, reason, evidence):
    if not reason.strip() or not evidence.strip():
        raise ValueError("exposure needs a reason and a decision/recording locator")
    temporary = record_path.with_name(record_path.name + ".pending")
    with temporary.open("x") as out:
        # Read under the same exclusive lock used by challenge execution.
        try:
            record = json.loads(record_path.read_text())
            if status(record) != "unexposed":
                raise ValueError("this selection is already regression material; a fresh challenge is required")
        except (ValueError, OSError):
            temporary.unlink()
            raise
        record["events"].append({"event": "exposed", "at": now(), "reason": reason, "evidence": evidence})
        out.write(json.dumps(record, indent=2) + "\n")
    temporary.replace(record_path)
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("freeze", "check"):
        command = sub.add_parser(name)
        command.add_argument("--corpus", type=Path, required=True)
        command.add_argument("--record", type=Path, required=True)
    command = sub.add_parser("expose")
    command.add_argument("--record", type=Path, required=True)
    command.add_argument("--reason", required=True)
    command.add_argument("--evidence", required=True)
    args = parser.parse_args()
    try:
        if args.command == "freeze":
            freeze(args.corpus, args.record)
            print("Frozen declared challenge identity; ordinary corpus diagnostics still refuse it.")
        elif args.command == "expose":
            expose(args.record, args.reason, args.evidence)
            print("Selection is now regression material; obtain a fresh challenge before another held-out claim.")
        else:
            current = check(args.corpus, args.record)
            print(current)
            return 0 if current == "unexposed" else 2
        return 0
    except (ValueError, OSError, KeyError, TypeError) as error:
        print(error, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
