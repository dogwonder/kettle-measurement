"""Coverage claims must not confuse executable regressions with capability."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "coverage_report", Path(__file__).with_name("capability-coverage.py")
)
coverage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(coverage)


class CoverageClaims(unittest.TestCase):
    def setUp(self):
        self.inventories = copy.deepcopy(coverage.load())

    def case(self, case_id):
        return next(c for _, inventory in self.inventories
                    for c in inventory["cases"] if c["id"] == case_id)

    def one_case_report(self, case):
        return coverage.build_report([("case.json", {
            "capability": case["capability"], "cases": [case],
        })])

    def test_a_passing_misread_regression_is_reported_as_a_gap(self):
        # The account reference parses as money, which is the defect.
        case = self.case("party-form-012")
        report = self.one_case_report(case)
        self.assertEqual(report["totals"]["verifier_cases_with_checks"], 1)
        self.assertEqual(report["totals"]["verifier_check_expectations"],
                         {"accepted-misread": 1})
        self.assertEqual(report["totals"]["verifier_checks_with_known_gaps"], 1)
        self.assertEqual(report["totals"]["model_measured"], 0)
        case["verifier"]["coverage"] = "main-check"
        with self.assertRaisesRegex(ValueError, "cannot be labelled supported"):
            coverage.build_report(self.inventories)

    def test_a_boundary_claim_without_a_case_adapter_is_not_a_check(self):
        case = self.case("format-form-008")
        report = self.one_case_report(case)
        self.assertEqual(report["totals"]["verifier_cases_with_checks"], 0)
        self.assertEqual(report["totals"]["verifier_cases_without_checks"], 1)
        case["verifier"]["coverage"] = "main-check"
        with self.assertRaisesRegex(ValueError, "no executable inventory adapter"):
            coverage.build_report(self.inventories)

    def test_dates_count_their_actual_checks_without_model_evidence(self):
        dates = [(name, inv) for name, inv in self.inventories
                 if name == "date-forms.json"]
        totals = coverage.build_report(dates)["totals"]
        self.assertEqual(totals["verifier_cases_with_checks"], len(dates[0][1]["cases"]))
        self.assertGreater(totals["verifier_check_expectations"]["no-derived-date"], 0)
        self.assertEqual(totals["model_measured"], 0)

    def test_an_unrelated_test_cannot_back_a_case_check(self):
        self.case("date-form-001")["verifier"]["test"] = copy.deepcopy(
            self.case("money-form-001")["verifier"]["test"]
        )
        with self.assertRaisesRegex(ValueError, "wrong owning test"):
            coverage.build_report(self.inventories)

    def test_stale_pending_status_is_refused(self):
        self.case("date-form-001")["verifier"]["coverage"] = "pending-v19-test"
        with self.assertRaisesRegex(ValueError, "unknown verifier coverage"):
            coverage.build_report(self.inventories)

    def test_a_measured_label_cannot_invent_recorded_model_evidence(self):
        case = self.case("date-form-001")
        case["model_reading"]["coverage"] = "measured"
        # Even a plausible locator is not proof that the recording asked
        # this case with a compatible request and identified weights.
        for recording in (None, "evals/runs/plausible-recording"):
            case["model_reading"]["recording"] = recording
            with self.assertRaisesRegex(ValueError, "cannot establish model evidence"):
                coverage.build_report(self.inventories)


class RecordedDiagnostics(unittest.TestCase):
    def setUp(self):
        self.path = coverage.PROJECT / "evals/corpus/diagnostic-01.json"
        self.corpus, self.links = coverage.diagnostic_links(self.path)
        # A fabricated report tests validation only; it is never published as
        # evidence. Command-level Rust tests separately exercise real replay.
        self.report = {
            "schema": coverage.SCHEMA, "scoring": coverage.SCORING,
            "answer_source": "replay", "unscored_cases": 0,
            "selection": self.corpus["selection"],
            "fields": self.corpus["selection"]["fields"],
            "model": {"weights_digest": "sha256:test-weights"},
            "runtime": {"test": True}, "sidecar": {"test": True},
            "generation_machine": {"test": True}, "corpus_digest": "test-corpus",
            "replay": {"exact_requests": 33, "legacy_prompt_only_requests": 0},
            "cases": [{"case": c["id"], "coverage": c["coverage"],
                       "score": {"missed_raw": 1, "whole_raw": 0},
                       "exchanges": [{"generation": {"test": True}}]}
                      for c in self.corpus["cases"]],
        }

    def evidence(self):
        return coverage.checked_evidence(self.corpus, self.links, self.report, Path("test-only"))

    def test_selection_and_unsupported_fields_do_not_count_as_measurements(self):
        totals = coverage.build_report(coverage.load(), links=self.links)["totals"]
        self.assertEqual(totals["diagnostic_selected"], 33)
        self.assertEqual(totals["diagnostic_unsupported_scope"], 4)
        self.assertEqual(totals["model_measured"], 0)

    def test_bad_answers_are_measured_attempts_without_promoting_unsupported_scope(self):
        evidence = self.evidence()
        self.assertEqual(len(evidence), 29)
        self.assertEqual(evidence["date-form-001"]["score"]["whole_raw"], 0)
        self.assertNotIn("time-form-001", evidence)
        self.assertNotIn("party-form-009", evidence)
        self.assertNotIn("party-form-011", evidence)
        self.assertEqual(coverage.build_report(coverage.load(), evidence=evidence)["totals"]["model_measured"], 29)

    def test_missing_or_incompatible_evidence_never_counts(self):
        for key, bad in [("model", None), ("runtime", None), ("generation_machine", None),
                         ("sidecar", None), ("scoring", "old"), ("unscored_cases", 1),
                         ("answer_source", "controlled-endpoint"),
                         ("replay", {"exact_requests": 0, "legacy_prompt_only_requests": 33})]:
            with self.subTest(key=key), patch.dict(self.report, {key: bad}):
                with self.assertRaises(ValueError):
                    self.evidence()
        for key, bad in [("exchanges", []), ("coverage", []), ("score", None),
                         ("execution_error", "missing answer"), ("acquisition_errors", ["unmapped"]),
                         ("attribution_errors", ["overlapping ask kinds"]),
                         ("exchanges", [{"generation": None}])]:
            with self.subTest(key=key), patch.dict(self.report["cases"][0], {key: bad}):
                with self.assertRaises(ValueError):
                    self.evidence()
        self.report["cases"].pop()
        with self.assertRaisesRegex(ValueError, "does not cover"):
            self.evidence()

    def test_inventory_and_document_mutations_require_regeneration(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for path in coverage.ROOT.glob("*-forms.json"):
                (root / path.name).write_bytes(path.read_bytes())
            with (root / "date-forms.json").open("a") as out:
                out.write("\n")
            with self.assertRaisesRegex(ValueError, "stale"):
                coverage.diagnostic_links(self.path, root)
            altered = copy.deepcopy(self.corpus)
            altered["cases"][0]["passages"][2] += " A changed instruction."
            path = root / "corpus.json"
            path.write_text(json.dumps(altered))
            with self.assertRaisesRegex(ValueError, "mismatched"):
                coverage.diagnostic_links(path)

    def test_exact_replay_refusal_is_propagated_and_mocks_never_reach_replay(self):
        with tempfile.TemporaryDirectory() as temp:
            recording = Path(temp)
            previous = {"answer_source": "controlled-endpoint", "model": None}
            (recording / "report.json").write_text(json.dumps(previous))
            with patch.object(coverage.subprocess, "run") as run:
                with self.assertRaisesRegex(ValueError, "cannot establish"):
                    coverage.replay_evidence(self.path, self.corpus, self.links, recording, Path("kettle"))
                run.assert_not_called()
                previous.update(answer_source="model", model={"weights_digest": "test"}, pack="app.kttl.letter-to-actions")
                (recording / "report.json").write_text(json.dumps(previous))
                raw = recording / "case-0000/raw"
                raw.mkdir(parents=True)
                (raw / "0001-question.generation.json").write_text("{}")
                manifest = raw.parent / "run.json"
                manifest.write_text("{}")
                with self.assertRaisesRegex(ValueError, "every recorded exchange"):
                    coverage.replay_evidence(self.path, self.corpus, self.links, recording, Path("kettle"))
                run.assert_not_called()
                manifest.write_text(json.dumps({"model": previous["model"]}))
                run.return_value.returncode = 2
                run.return_value.stderr = "request/schema mismatch"
                with self.assertRaisesRegex(ValueError, "request/schema mismatch"):
                    coverage.replay_evidence(self.path, self.corpus, self.links, recording, Path("kettle"))
                args = run.call_args.args[0]
                self.assertIn("--replay", args)
                self.assertNotIn("--model", args)


if __name__ == "__main__":
    unittest.main()
