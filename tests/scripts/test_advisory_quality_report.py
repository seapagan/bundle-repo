"""Tests for advisory-quality platform aggregation."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = (
    Path(__file__).parents[2] / ".github" / "scripts" / "advisory_quality_report.py"
)
SPEC = importlib.util.spec_from_file_location("advisory_quality_report", SCRIPT)
assert SPEC and SPEC.loader
REPORT = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = REPORT
SPEC.loader.exec_module(REPORT)


def finding(
    path: str = "src/text_processing.rs",
    line: int = 101,
    function: str = "classify_and_decode",
) -> object:
    """Build a controlled maintainability finding."""
    return REPORT.Finding(
        path=path,
        line=line,
        function=function,
        lint="clippy::too_many_lines",
        observed=68,
        threshold=60,
        unit="lines",
        message=None,
    )


class CombinedCommentTests(unittest.TestCase):
    """Exercise deterministic grouping across structured platform results."""

    def render(self, linux: list[object], windows: list[object]) -> str:
        """Round-trip controlled platform findings through JSON files."""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            linux_path = root / "quality-linux.json"
            windows_path = root / "quality-windows.json"
            REPORT.write_platform_result(linux_path, "Linux", linux)
            REPORT.write_platform_result(windows_path, "Windows", windows)
            results = [
                REPORT.load_platform_result(linux_path),
                REPORT.load_platform_result(windows_path),
            ]
            return REPORT.render_combined_comment(results)

    def test_identical_findings_are_grouped_for_all_platforms(self) -> None:
        shared = finding()
        comment = self.render([shared], [shared])

        self.assertIn("### All platforms", comment)
        self.assertEqual(comment.count("`classify_and_decode`"), 1)
        self.assertIn("68/60 lines", comment)

    def test_linux_only_finding_has_platform_heading(self) -> None:
        comment = self.render([finding()], [])

        self.assertIn("### Linux only", comment)
        self.assertNotIn("### Windows only", comment)

    def test_windows_only_finding_has_platform_heading(self) -> None:
        comment = self.render([], [finding()])

        self.assertIn("### Windows only", comment)
        self.assertNotIn("### Linux only", comment)

    def test_shared_and_platform_findings_use_distinct_groups(self) -> None:
        shared = finding()
        linux = finding("src/linux.rs", 20, "linux_check")
        windows = finding("src/windows.rs", 30, "windows_check")
        comment = self.render([shared, linux], [shared, windows])

        self.assertIn("### All platforms", comment)
        self.assertIn("### Linux only", comment)
        self.assertIn("### Windows only", comment)
        self.assertEqual(comment.count("`classify_and_decode`"), 1)

    def test_no_findings_has_no_platform_sections(self) -> None:
        comment = self.render([], [])

        self.assertIn("## ✅ Advisory maintainability", comment)
        self.assertNotIn("\n### ", comment)

    def test_same_function_at_different_locations_is_not_merged(self) -> None:
        linux = finding("src/one.rs", 10, "shared_name")
        windows = finding("src/two.rs", 20, "shared_name")
        comment = self.render([linux], [windows])

        self.assertEqual(comment.count("`shared_name`"), 2)
        self.assertIn("`src/one.rs:10`", comment)
        self.assertIn("`src/two.rs:20`", comment)


if __name__ == "__main__":
    unittest.main()
