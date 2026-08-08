"""Tests for advisory-quality platform aggregation."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[2]
SCRIPT = ROOT / ".github" / "scripts" / "advisory_quality_report.py"
SPEC = importlib.util.spec_from_file_location("advisory_quality_report", SCRIPT)
assert SPEC and SPEC.loader
REPORT = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = REPORT
SPEC.loader.exec_module(REPORT)

TOO_MANY_LINES_DIAGNOSTIC = """\
warning: this function has too many lines (68/60)
  --> src/text_processing.rs:101:1
   |
101 | fn classify_and_decode(
   | ^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: https://rust-lang.github.io/rust-clippy/index.html#too_many_lines
"""
TOO_MANY_ARGUMENTS_DIAGNOSTIC = """\
warning: this function has too many arguments (9/8)
  --> tests\\crate\\cli.rs:42:1
   |
42 | fn parse_options(
   | ^^^^^^^^^^^^^^^^
   |
   = note: requested on the command line with `-W clippy::too-many-arguments`
"""
LINT_MARKER_CASES = (
    (
        "clippy::too_many_lines",
        "this function has too many lines (68/60)",
        "= help: https://rust-lang.github.io/rust-clippy/#too_many_lines",
    ),
    (
        "clippy::too_many_lines",
        "this function has too many lines (68/60)",
        "= note: requested with `-W clippy::too-many-lines`",
    ),
    (
        "clippy::too_many_arguments",
        "this function has too many arguments (9/8)",
        "= help: https://rust-lang.github.io/rust-clippy/#too_many_arguments",
    ),
    (
        "clippy::too_many_arguments",
        "this function has too many arguments (9/8)",
        "= note: requested with `-W clippy::too-many-arguments`",
    ),
)


def finding(
    path: str = "src/text_processing.rs",
    line: int = 101,
    function: str = "classify_and_decode",
    *,
    lint: str = "clippy::too_many_lines",
    observed: int = 68,
    threshold: int = 60,
    unit: str = "lines",
) -> object:
    """Build a controlled maintainability finding."""
    return REPORT.Finding(
        path=path,
        line=line,
        function=function,
        lint=lint,
        observed=observed,
        threshold=threshold,
        unit=unit,
        message=None,
    )


class ParsingTests(unittest.TestCase):
    """Exercise raw Clippy diagnostics through the public parsing pipeline."""

    def test_extracts_both_configured_lints(self) -> None:
        output = TOO_MANY_LINES_DIAGNOSTIC + TOO_MANY_ARGUMENTS_DIAGNOSTIC

        self.assertEqual(
            REPORT.extract_findings(output),
            [
                finding(),
                finding(
                    "tests/crate/cli.rs",
                    42,
                    "parse_options",
                    lint="clippy::too_many_arguments",
                    observed=9,
                    threshold=8,
                    unit="arguments",
                ),
            ],
        )

    def test_both_marker_forms_identify_each_lint(self) -> None:
        for lint, warning, marker_line in LINT_MARKER_CASES:
            with self.subTest(marker=marker_line):
                diagnostic = (
                    f"warning: {warning}\n"
                    "  --> src/example.rs:5:1\n"
                    "5 | fn example() {}\n"
                    f"   {marker_line}\n"
                )

                findings = REPORT.extract_findings(diagnostic)

                self.assertEqual(len(findings), 1)
                self.assertEqual(findings[0].lint, lint)

    def test_extract_findings_removes_ansi_sequences(self) -> None:
        coloured = TOO_MANY_LINES_DIAGNOSTIC.replace(
            "warning:",
            "\x1b[1m\x1b[33mwarning\x1b[0m\x1b[1m:\x1b[0m",
            1,
        )

        self.assertEqual(REPORT.extract_findings(coloured), [finding()])

    def test_missing_location_and_function_use_fallbacks(self) -> None:
        diagnostic = """\
warning: this function has too many lines (68/60)
   = note: requested on the command line with `-W clippy::too-many-lines`
"""

        findings = REPORT.extract_findings(diagnostic)

        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0].location, "location not reported")
        self.assertEqual(findings[0].function, "function not reported")

    def test_unrelated_warning_is_ignored(self) -> None:
        diagnostic = """\
warning: unused variable: `value`
  --> src/main.rs:10:9
   |
10 |     let value = 1;
   |         ^^^^^ help: prefix it with an underscore: `_value`
"""

        self.assertEqual(REPORT.extract_findings(diagnostic), [])

    def test_duplicate_raw_diagnostics_are_deduplicated(self) -> None:
        output = TOO_MANY_LINES_DIAGNOSTIC + TOO_MANY_LINES_DIAGNOSTIC

        self.assertEqual(REPORT.extract_findings(output), [finding()])


class PlatformReportTests(unittest.TestCase):
    """Exercise the per-platform Markdown report."""

    def test_no_findings_report(self) -> None:
        report = REPORT.render_report("Linux", [])

        self.assertIn("### Linux", report)
        self.assertIn("✅ No advisory maintainability findings", report)

    def test_findings_report_retains_diagnostic_details(self) -> None:
        report = REPORT.render_report("Windows", [finding()])

        self.assertIn("### Windows", report)
        self.assertIn("`src/text_processing.rs:101`", report)
        self.assertIn("`classify_and_decode`", report)
        self.assertIn("`clippy::too_many_lines`", report)
        self.assertIn("68/60 lines", report)


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

    def test_displayed_thresholds_match_clippy_config(self) -> None:
        config = tomllib.loads((ROOT / "clippy.toml").read_text(encoding="utf-8"))
        comment = self.render([], [])

        self.assertIn(
            f"- function lines: {config['too-many-lines-threshold']}",
            comment,
        )
        self.assertIn(
            f"- arguments: {config['too-many-arguments-threshold']}",
            comment,
        )


if __name__ == "__main__":
    unittest.main()
