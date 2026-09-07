import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("challenge", Path(__file__).with_name("challenge-selection.py"))
challenge = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(challenge)


class ChallengeLifecycle(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.corpus = self.root / "challenge.json"
        self.record = self.root / "lifecycle.json"
        # Synthetic contract-test data, not an actual independent challenge.
        self.data = {"schema_version": 1, "selection": {"id": "test-only-challenge", "purpose": "challenge",
                     "exposure": "unexposed", "fields": ["kind"]},
                     "cases": [{"id": "contract-only", "passages": ["Sentinel document used only by the lifecycle test."]}],
                     "provenance": {"independent": True, "authoring": {"author": "test fixture",
                     "relationship": "separate-author", "source_families": [{"id": "test-family", "locator": "test-only",
                     "relationship": "external-source"}]}}}
        self.write()

    def write(self):
        self.corpus.write_text(json.dumps(self.data))

    def test_freeze_pins_identity_and_exposure_is_irreversible(self):
        challenge.freeze(self.corpus, self.record)
        self.assertEqual(challenge.check(self.corpus, self.record), "unexposed")
        with self.assertRaises(FileExistsError):
            challenge.freeze(self.corpus, self.record)
        challenge.expose(self.record, "result informed a prompt edit", "test-run/decision")
        self.assertEqual(challenge.check(self.corpus, self.record), "regression")
        with self.assertRaisesRegex(ValueError, "already regression"):
            challenge.expose(self.record, "again", "test")
        with self.assertRaisesRegex(ValueError, "already frozen"):
            challenge.freeze(self.corpus, self.root / "renamed-record.json")

    def test_changes_to_truth_or_selection_invalidate_the_frozen_record(self):
        challenge.freeze(self.corpus, self.record)
        self.data["facts"] = [{"id": "changed-truth"}]
        self.write()
        with self.assertRaisesRegex(ValueError, "changed after freezing"):
            challenge.check(self.corpus, self.record)

    def test_development_examples_cannot_be_relabelled_as_fresh(self):
        original = copy.deepcopy(self.data)
        known = json.loads((challenge.PROJECT / "evals/corpus/diagnostic-01.json").read_text())
        self.data["cases"] = known["cases"][:1]
        self.write()
        with self.assertRaisesRegex(ValueError, "reuses an exposed case"):
            challenge.freeze(self.corpus, self.record)
        self.data = original
        self.data["provenance"]["authoring"]["source_families"][0]["id"] = "kettle-examples-variant"
        self.write()
        with self.assertRaisesRegex(ValueError, "cannot be declared independent"):
            challenge.freeze(self.corpus, self.record)

    def test_missing_provenance_and_incomplete_exposure_are_refused(self):
        self.data["provenance"]["authoring"]["relationship"] = "development-generator"
        self.write()
        with self.assertRaisesRegex(ValueError, "separate author"):
            challenge.freeze(self.corpus, self.record)
        with self.assertRaisesRegex(ValueError, "invalid challenge lifecycle"):
            challenge.status({"schema": "kettle/challenge-lifecycle@1", "events": [{"event": "frozen", "at": "test"}, {"event": "exposed", "at": "test"}]})
