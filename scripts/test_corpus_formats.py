import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("formats", Path(__file__).with_name("corpus-formats.py"))
formats = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(formats)
BUNDLE = formats.PROJECT / "evals/corpus/formats-01"


class PairedAssets(unittest.TestCase):
    def test_committed_bundle_keeps_all_cases_and_page_groups(self):
        generation, corpus = formats.validate(BUNDLE)
        self.assertEqual(len(corpus["cases"]), 2)
        self.assertEqual([len(p["pages"]) for p in generation["page_plans"].values()], [1, 3])
        self.assertFalse(generation["independent"])

    def test_missing_changed_or_extra_assets_refuse_comparison(self):
        for mutation in ("missing", "changed", "extra"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temp:
                bundle = Path(temp) / "bundle"
                shutil.copytree(BUNDLE, bundle)
                path = bundle / "format01-three-pages/page-3.txt"
                if mutation == "missing":
                    path.unlink()
                elif mutation == "changed":
                    path.write_text("An altered instruction.")
                else:
                    (bundle / "extra.txt").write_text("An untracked page.")
                with self.assertRaisesRegex(ValueError, "changed"):
                    formats.validate(bundle)

    def test_reordered_text_pages_cannot_be_relabelled_as_equivalent(self):
        import hashlib
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp) / "bundle"
            shutil.copytree(BUNDLE, bundle)
            path = bundle / "text.bindings.json"
            bindings = json.loads(path.read_text())
            bindings["format01-three-pages"]["inputs"]["letter"].reverse()
            path.write_text(json.dumps(bindings))
            # Even updating the byte digest cannot turn reordered pages into
            # the authored page plan.
            generation = json.loads((bundle / "generation.json").read_text())
            generation["files"][path.name] = "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()
            (bundle / "generation.json").write_text(json.dumps(generation))
            with self.assertRaisesRegex(ValueError, "content/order"):
                formats.validate(bundle)
