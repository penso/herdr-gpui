"""Bundled font terms must survive a crate's separate software-license report."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "font_notices", Path(__file__).resolve().parents[1] / "generate-notices.py"
)
notices = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(notices)


class FontNoticeTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.package = dict(
            id="font-crate", name="ratex-katex-fonts", version="0.1.15", license="MIT",
            source="registry+https://github.com/rust-lang/crates.io-index",
            manifest_path=str(self.root / "Cargo.toml"), license_file=None,
        )
        self.evidence = dict(
            crates=[dict(package=self.package, license="MIT")],
            licenses=[dict(name="MIT License", id="MIT", text="Synthetic crate code license\n",
                           source_path=None, used_by=[{"crate": self.package}])],
        )
        self.fonts = self.root / "fonts"
        self.fonts.mkdir()
        self.terms = {
            "OFL.txt": "Synthetic font copyright\nSIL OPEN FONT LICENSE Version 1.1\n",
            "FONT_NOTICE.txt": "Synthetic KaTeX font provenance: https://github.com/KaTeX/KaTeX\n",
        }
        for name, text in self.terms.items():
            (self.fonts / name).write_text(text)
        (self.root / "LICENSE").write_text("Synthetic crate code license\n")

    def test_report_preserves_both_font_files_and_distinguishes_their_license(self):
        # Only reviewed text files are collected, never binary font contents.
        (self.fonts / "KaTeX_Main-Regular.ttf").write_bytes(b"\xff\x00not text")
        report = notices.render(self.evidence, b"locked dependency graph")
        for name, text in self.terms.items():
            self.assertIn(f"--- fonts/{name} ---\n{text}", report)
        self.assertIn("Bundled KaTeX fonts: SIL Open Font License 1.1 (OFL-1.1); crate code: MIT.", report)
        self.assertIn("https://crates.io/api/v1/crates/ratex-katex-fonts/0.1.15/download", report)
        self.assertNotIn(str(self.root), report)

    def test_each_missing_font_notice_prevents_any_report_output(self):
        (self.root / "Cargo.lock").write_text("locked dependencies")
        output = self.root / "report.txt"
        for name, text in self.terms.items():
            with self.subTest(name=name):
                (self.fonts / name).unlink()
                responses = [
                    notices.subprocess.CompletedProcess([], 0, notices.ABOUT_VERSION + "\n"),
                    notices.subprocess.CompletedProcess([], 0, notices.json.dumps(self.evidence).encode()),
                ]
                with patch.object(notices, "ROOT", self.root), \
                     patch("sys.argv", ["generate-notices.py", str(output)]), \
                     patch.object(notices.subprocess, "run", side_effect=responses):
                    with self.assertRaises(FileNotFoundError):
                        notices.main()
                self.assertFalse(output.exists())
                (self.fonts / name).write_text(text)

    def test_empty_required_font_notice_is_rejected(self):
        for name, text in self.terms.items():
            with self.subTest(name=name):
                (self.fonts / name).write_text(" \n")
                with self.assertRaisesRegex(ValueError, "empty license/notice"):
                    notices.render(self.evidence, b"lock")
                (self.fonts / name).write_text(text)

    def test_font_notice_cannot_escape_the_dependency_directory(self):
        notice = self.fonts / "FONT_NOTICE.txt"
        notice.unlink()
        notice.symlink_to(Path(__file__).resolve())
        with self.assertRaisesRegex(ValueError, "license path escapes package"):
            notices.render(self.evidence, b"lock")

    def test_new_font_crate_version_requires_notice_review(self):
        self.package["version"] = "0.1.16"
        with self.assertRaisesRegex(ValueError, "KaTeX font notices need version review"):
            notices.render(self.evidence, b"lock")
