"""Coverage claims must not confuse executable regressions with capability."""
import copy
import importlib.util
from pathlib import Path
import unittest

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


if __name__ == "__main__":
    unittest.main()
