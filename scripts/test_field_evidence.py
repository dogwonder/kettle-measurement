"""Private field evidence crosses the command boundary only as closed aggregates."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/field-evidence.py"


class FieldEvidence(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.run = self.root / "sensitive-letter.private"
        self.run.mkdir()
        self.secret = "PRIVATE-CANARY £412.39 7 March 2026 account AB123"
        (self.run / "results.json").write_text(json.dumps({
            "run": {"pack": "test-pack", "pack_version": "1", "file": self.secret},
            "obligations": [{"ask": self.secret}]}))
        (self.run / "manifest.json").write_text(json.dumps({"model": {"file": "test-model"}}))
        (self.run / "raw").mkdir()
        (self.run / "raw/001.response.json").write_text(self.secret)
        self.provenance = self.root / "provenance.private.json"
        self.provenance.write_text(json.dumps({"model": "test-model", "pack": "test-pack@1",
                                               "runtime": "test-runtime", "basis": "synthetic test"}))
        self.record = self.root / "observation.private.json"

    def call(self, *args):
        return subprocess.run(["python3", str(SCRIPT), *map(str, args)], capture_output=True, text=True)

    def initialise(self):
        result = self.call("init", "--run-dir", self.run, "--provenance", self.provenance,
                           "--format", "pdf", "--structure", "table", "--coverage", "amount-selection",
                           "--out", self.record)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(self.record.read_text())

    def save(self, data):
        self.record.write_text(json.dumps(data))

    def test_field_export_contains_counts_and_shape_metadata_but_no_document_content(self):
        data = self.initialise()
        data["adjudications"][0].update(outcome="wrong", note=self.secret)
        data["adjudications"].append({"id": "missing-1", "target": "missing", "outcome": "incomplete",
                                      "note": self.secret, "reproducer": None, "issue": None})
        self.save(data)
        result = self.call("export", "--record", self.record)
        self.assertEqual(result.returncode, 0, result.stderr)
        export = json.loads(result.stdout)
        self.assertEqual(export["outcomes"]["wrong"], 1)
        self.assertEqual(export["outcomes"]["incomplete"], 1)
        self.assertEqual(export["missing_claims"], 1)
        self.assertEqual(export["discoveries_without_reproducer"], 2)
        self.assertEqual(export["formats"], {"pdf": 1})
        for secret in [self.secret, "£412.39", "7 March", "AB123", str(self.run), "test-model", "test-runtime", "001.response"]:
            self.assertNotIn(secret, result.stdout)
        registry_path = self.root / "registry.private.json"
        registry = self.call("registry", "--record", self.record, "--out", registry_path)
        self.assertEqual(registry.returncode, 0, registry.stderr)
        discoveries = json.loads(registry_path.read_text())["discoveries"]
        self.assertEqual(len(discoveries), 2)
        self.assertTrue(all(d["needs_issue"] and d["needs_reproducer"] for d in discoveries))

    def test_no_adjudication_is_not_correctness_and_all_outcomes_remain_distinct(self):
        data = self.initialise()
        export = json.loads(self.call("export", "--record", self.record).stdout)
        self.assertEqual(export["unreviewed"], 1)
        self.assertEqual(sum(export["outcomes"].values()), 0)
        data["adjudications"] = [{"id": str(i), "target": "asserted", "outcome": outcome,
                                 "note": self.secret, "reproducer": None, "issue": None}
                                for i, outcome in enumerate(["correct", "wrong", "incomplete", "unsupported", "uncheckable"])]
        self.save(data)
        export = json.loads(self.call("export", "--record", self.record).stdout)
        self.assertEqual(list(export["outcomes"].values()), [1] * 5)

    def test_refuses_public_private_record_and_replacement(self):
        args = ["init", "--run-dir", self.run, "--provenance", self.provenance,
                "--format", "pdf", "--structure", "table", "--coverage", "amount-selection"]
        self.assertNotEqual(self.call(*args, "--out", self.root / "public.json").returncode, 0)
        self.initialise()
        before = self.record.read_bytes()
        self.assertNotEqual(self.call(*args, "--out", self.record).returncode, 0)
        self.assertEqual(before, self.record.read_bytes())

    def test_export_refuses_free_text_shapes_and_changed_run_evidence(self):
        data = self.initialise()
        data["shape"]["structure"] = self.secret
        self.save(data)
        result = self.call("export", "--record", self.record)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn(self.secret, result.stderr)
        data["shape"]["structure"] = "table"
        self.save(data)
        (self.run / "raw/001.response.json").write_text("changed")
        self.assertNotEqual(self.call("export", "--record", self.record).returncode, 0)

    def test_duplicate_observations_and_missing_correct_claims_are_refused(self):
        data = self.initialise()
        self.assertNotEqual(self.call("export", "--record", self.record, "--record", self.record).returncode, 0)
        data["adjudications"][0].update(target="missing", outcome="correct")
        self.save(data)
        self.assertNotEqual(self.call("export", "--record", self.record).returncode, 0)


if __name__ == "__main__":
    unittest.main()
