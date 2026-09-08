"""Exercise the real scaffold, including safe refusal and standalone assets."""
from pathlib import Path
import json
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent


class ScaffoldTests(unittest.TestCase):
    def assert_common_project(self, destination, port):
        package = json.loads((destination / "package.json").read_text())
        self.assertTrue({"setup", "start", "test", "check", "test:browser"}.issubset(package["scripts"]))
        self.assertEqual(json.loads((destination / "scaffold.json").read_text())["port"], port)
        for name in ["playwright.config.js", "scripts/setup.sh", "scripts/start.sh",
                     "layout.html", "styles/_mini.scss", "tests/browser/layout.spec.js"]:
            self.assertEqual((destination / name).read_bytes(), (ROOT / "app/mini-templates/common" / name).read_bytes())
        self.assertTrue((destination / "tests/browser/flow.spec.js").is_file())
        page = destination / ("web/index.html" if port == 8788 else "index.html")
        html = page.read_text()
        self.assertNotIn("{{", html)
        self.assertIn('<main id="main" tabindex="-1">', html)
        self.assertIn('class="app-header"', html)
        self.assertIn('class="app-footer"', html)
        # A fresh scaffold must give an actionable setup message through npm,
        # without trying to download dependencies or load a model at startup.
        result = subprocess.run(["npm", "start"], cwd=destination, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Run ./scripts/setup.sh first.", result.stderr)

    @unittest.skipUnless((ROOT / "app/mini-templates/image-descriptions").is_dir(), "Product template is outside the public measurement projection")
    def test_generates_independent_project_and_refuses_overwrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "a mini with spaces"
            command = [str(ROOT / "scripts/new-mini"), "image-descriptions", "--runtime", "transformers", "--output", str(destination)]
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assert_common_project(destination, 8788)
            self.assertTrue((destination / "web/style.css").is_file())
            self.assertTrue((destination / "web/wordmark.svg").is_file())
            self.assertFalse((destination / "models").exists())
            self.assertFalse((destination / ".venv").exists())
            self.assertFalse(any(path.is_symlink() for path in destination.rglob("*")))
            css = (destination / "web/style.css").read_text()
            self.assertIn("--k-ground", css)
            self.assertIn("max-width:760px", css)
            self.assertNotRegex(css, r"url\([\s\"']*(?:https?:)?//")
            (destination / "README.md").write_text("User's work")
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual((destination / "README.md").read_text(), "User's work")

    def test_unsupported_runtime_creates_nothing(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "mini"
            result = subprocess.run([str(ROOT / "scripts/new-mini"), "image-descriptions", "--runtime", "llama", "--output", str(destination)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(destination.exists())

    @unittest.skipUnless((ROOT / "app/mini-templates/appointments").is_dir(), "Product template is outside the public measurement projection")
    def test_appointments_are_generated_beside_kettle_without_builds_or_weights(self):
        with tempfile.TemporaryDirectory() as temporary:
            kettle = Path(temporary) / "kettle"
            (kettle / "scripts").mkdir(parents=True)
            shutil.copy2(ROOT / "scripts/new-mini", kettle / "scripts/new-mini")
            (kettle / "app/mini-templates").mkdir(parents=True)
            (kettle / "app/mini-templates/appointments").symlink_to(ROOT / "app/mini-templates/appointments")
            (kettle / "app/mini-templates/common").symlink_to(ROOT / "app/mini-templates/common")
            command = [str(kettle / "scripts/new-mini"), "appointments"]
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            destination = kettle.parent / "kettle-mini-appointments"
            self.assert_common_project(destination, 8787)
            self.assertEqual(json.loads((destination / "package.json").read_text())["name"], "kettle-mini-appointments")
            self.assertTrue((destination / "fixtures/payment-reminder.jpg").is_file())
            self.assertTrue((destination / ".gitignore").is_file())
            for path in ["node_modules", "dist", "models", "bridge/target"]:
                self.assertFalse((destination / path).exists())
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)

    def test_mismatched_task_and_runtime_creates_nothing(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "mini"
            result = subprocess.run([str(ROOT / "scripts/new-mini"), "appointments", "--runtime", "transformers", "--output", str(destination)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(destination.exists())


if __name__ == "__main__":
    unittest.main()
