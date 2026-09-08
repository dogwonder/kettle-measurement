"""Check replay evidence using synthetic reports, without a pod or model."""
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().with_name("corpus-pod.sh")
SELECTIONS = ("diagnostic-01", "product-regressions-01")


class ReplayCheckTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.arm = Path(self.temporary.name)
        self.report = {
            "cases": [{"case": "first", "score": {"correct": 1},
                       "exchanges": [{"request": "controlled request"}]}],
            "summary": {"correct": 1},
            "corpus_digest": "synthetic-corpus",
            "pipeline_digest": "synthetic-pipeline",
            "replay": {"exact_requests": 1, "legacy_prompt_only_requests": 0},
        }
        for selection in SELECTIONS:
            for suffix in ("", "-replay"):
                self.write(selection + suffix, self.report)

    def write(self, selection, report):
        directory = self.arm / selection
        directory.mkdir(exist_ok=True)
        (directory / "report.json").write_text(json.dumps(report))

    def check(self):
        return subprocess.run(
            ["bash", str(SCRIPT), "replay-check", str(self.arm)],
            capture_output=True, text=True,
        )

    def test_matching_reports_pass(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.count("exact, 1 requests"), 2)

    def test_missing_original_or_replay_report_fails(self):
        for selection in SELECTIONS:
            for suffix in ("", "-replay"):
                name = selection + suffix
                with self.subTest(report=name):
                    (self.arm / name / "report.json").unlink()
                    try:
                        self.assertNotEqual(self.check().returncode, 0)
                    finally:
                        self.write(name, self.report)

    def test_no_reports_fails(self):
        for path in self.arm.glob("*/report.json"):
            path.unlink()
        self.assertNotEqual(self.check().returncode, 0)

    def test_empty_reports_do_not_establish_replay(self):
        empty = dict(self.report, cases=[], summary={},
                     replay={"exact_requests": 0, "legacy_prompt_only_requests": 0})
        for selection in SELECTIONS:
            self.write(selection, empty)
            self.write(selection + "-replay", empty)
        self.assertNotEqual(self.check().returncode, 0)

    def test_same_scores_for_a_different_case_fail(self):
        changed = deepcopy(self.report)
        changed["cases"][0]["case"] = "another-case"
        self.write(SELECTIONS[0] + "-replay", changed)
        self.assertNotEqual(self.check().returncode, 0)

    def test_changed_scores_identity_or_request_accounting_fail(self):
        variants = []
        changed = deepcopy(self.report)
        changed["cases"][0]["score"]["correct"] = 0
        variants.append(changed)
        variants.append(dict(self.report, pipeline_digest="different-pipeline"))
        variants.append(dict(self.report, replay={"exact_requests": 0}))
        variants.append(dict(self.report, replay={
            "exact_requests": 1, "legacy_prompt_only_requests": 1,
        }))
        for changed in variants:
            with self.subTest(report=changed):
                self.write(SELECTIONS[0] + "-replay", changed)
                self.assertNotEqual(self.check().returncode, 0)


if __name__ == "__main__":
    unittest.main()
