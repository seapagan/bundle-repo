"""Tests for Cargo package analysis and publishability reporting."""

from __future__ import annotations

import importlib.util
import io
import json
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).parents[2]
SCRIPT = ROOT / ".github" / "scripts" / "package_report.py"
SPEC = importlib.util.spec_from_file_location("package_report", SCRIPT)
assert SPEC and SPEC.loader
REPORT = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = REPORT
SPEC.loader.exec_module(REPORT)


def write_archive(
    path: Path,
    files: tuple[str, ...] = REPORT.REQUIRED_PATHS,
    *,
    root: str = "bundle_repo-0.6.0",
    directories: tuple[str, ...] = (),
) -> None:
    """Create a small deterministic Cargo-like archive fixture."""
    with tarfile.open(path, mode="w:gz") as archive:
        for name in files:
            payload = f"fixture for {name}\n".encode()
            info = tarfile.TarInfo(f"{root}/{name}")
            info.size = len(payload)
            info.mtime = 0
            archive.addfile(info, io.BytesIO(payload))
        for name in directories:
            info = tarfile.TarInfo(f"{root}/{name}")
            info.type = tarfile.DIRTYPE
            info.mtime = 0
            archive.addfile(info)


class PackageAnalysisTests(unittest.TestCase):
    """Exercise archive measurement, structure, and content policy."""

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.archive = self.root / "bundle_repo-0.6.0.crate"

    def analyse(self, files: tuple[str, ...] = REPORT.REQUIRED_PATHS) -> object:
        """Write and analyse one controlled package."""
        write_archive(self.archive, files)
        return REPORT.analyse_archive(self.archive, 1_000_000)

    def test_measures_exact_archive_bytes(self) -> None:
        result = self.analyse()

        self.assertEqual(result.size_bytes, self.archive.stat().st_size)

    def test_below_limit_has_headroom(self) -> None:
        result = self.analyse()

        self.assertTrue(result.size_ok)
        self.assertEqual(result.headroom_bytes, 1_000_000 - result.size_bytes)
        self.assertEqual(result.exceeded_by_bytes, 0)

    def test_exactly_at_limit_passes(self) -> None:
        write_archive(self.archive)
        size = self.archive.stat().st_size

        result = REPORT.analyse_archive(self.archive, size)

        self.assertTrue(result.size_ok)
        self.assertEqual(result.headroom_bytes, 0)

    def test_above_limit_records_exceeded_bytes(self) -> None:
        write_archive(self.archive)
        size = self.archive.stat().st_size

        result = REPORT.analyse_archive(self.archive, size - 7)

        self.assertFalse(result.size_ok)
        self.assertEqual(result.exceeded_by_bytes, 7)
        self.assertEqual(result.headroom_bytes, 0)

    def test_required_content_succeeds(self) -> None:
        result = self.analyse()

        self.assertTrue(result.contents_ok)
        self.assertEqual(result.missing_paths, ())

    def test_missing_required_content_is_reported(self) -> None:
        missing = "README-cratesio.md"
        files = tuple(path for path in REPORT.REQUIRED_PATHS if path != missing)

        result = self.analyse(files)

        self.assertFalse(result.contents_ok)
        self.assertEqual(result.missing_paths, (missing,))

    def test_forbidden_content_is_reported(self) -> None:
        result = self.analyse(
            REPORT.REQUIRED_PATHS + ("docs/index.md", ".github/workflows/test.yml")
        )

        self.assertEqual(
            result.forbidden_paths,
            (".github/workflows/test.yml", "docs/index.md"),
        )

    def test_empty_forbidden_directory_is_reported(self) -> None:
        write_archive(self.archive, directories=("docs",))

        result = REPORT.analyse_archive(self.archive)

        self.assertEqual(result.forbidden_paths, ("docs",))

    def test_archive_root_is_removed_from_policy_paths(self) -> None:
        write_archive(
            self.archive,
            ("./.cargo_vcs_info.json",) + REPORT.REQUIRED_PATHS[1:],
            root="renamed-package-1.2.3",
        )

        result = REPORT.analyse_archive(self.archive)

        self.assertEqual(result.archive_root, "renamed-package-1.2.3")
        self.assertNotIn(".cargo_vcs_info.json", result.missing_paths)

    def test_multiple_archive_roots_are_rejected(self) -> None:
        with tarfile.open(self.archive, mode="w:gz") as archive:
            for name in ("first/Cargo.toml", "second/Cargo.lock"):
                info = tarfile.TarInfo(name)
                archive.addfile(info, io.BytesIO())

        with self.assertRaisesRegex(REPORT.ArchiveError, "one archive root"):
            REPORT.analyse_archive(self.archive)

    def test_duplicate_archive_paths_are_rejected(self) -> None:
        with tarfile.open(self.archive, mode="w:gz") as archive:
            for _ in range(2):
                info = tarfile.TarInfo("package/Cargo.toml")
                archive.addfile(info, io.BytesIO())

        with self.assertRaisesRegex(REPORT.ArchiveError, "duplicate"):
            REPORT.analyse_archive(self.archive)

    def test_unsafe_archive_path_is_rejected(self) -> None:
        with tarfile.open(self.archive, mode="w:gz") as archive:
            info = tarfile.TarInfo("package/../Cargo.toml")
            archive.addfile(info, io.BytesIO())

        with self.assertRaisesRegex(REPORT.ArchiveError, "unsafe"):
            REPORT.analyse_archive(self.archive)

    def test_malformed_archive_is_rejected(self) -> None:
        self.archive.write_bytes(b"not a gzip tar archive")

        with self.assertRaisesRegex(REPORT.ArchiveError, "cannot read"):
            REPORT.analyse_archive(self.archive)

    def test_unreadable_archive_is_rejected(self) -> None:
        with self.assertRaisesRegex(REPORT.ArchiveError, "cannot measure"):
            REPORT.analyse_archive(self.root / "missing.crate")


class ArchiveDiscoveryTests(unittest.TestCase):
    """Exercise deterministic discovery of the just-built archive."""

    def test_discovers_only_archive(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "package.crate"
            archive.touch()
            (root / "ignored.txt").touch()

            self.assertEqual(REPORT.discover_archive(root), archive)

    def test_ambiguous_archive_input_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "old.crate").touch()
            (root / "current.crate").touch()

            with self.assertRaisesRegex(REPORT.ArchiveError, "found 2"):
                REPORT.discover_archive(root)


class RenderingTests(unittest.TestCase):
    """Exercise deterministic human and machine-readable reports."""

    def result(
        self,
        *,
        size: int = 8_792_636,
        missing: tuple[str, ...] = (),
        forbidden: tuple[str, ...] = (),
    ) -> object:
        """Build controlled report data."""
        limit = REPORT.PACKAGE_LIMIT_BYTES
        return REPORT.PackageResult(
            archive="bundle_repo-0.6.0.crate",
            archive_root="bundle_repo-0.6.0",
            size_bytes=size,
            limit_bytes=limit,
            headroom_bytes=max(limit - size, 0),
            exceeded_by_bytes=max(size - limit, 0),
            required_paths=REPORT.REQUIRED_PATHS,
            missing_paths=missing,
            forbidden_prefixes=REPORT.FORBIDDEN_PREFIXES,
            forbidden_paths=forbidden,
        )

    def test_decimal_mb_rendering(self) -> None:
        self.assertEqual(REPORT.decimal_mb(9_500_000), "9.50 MB")
        self.assertEqual(REPORT.decimal_mb(707_364), "0.71 MB")

    def test_healthy_markdown_includes_size_headroom_and_percentage(self) -> None:
        markdown = REPORT.render_markdown(self.result())

        self.assertIn("## ✅ Package checks", markdown)
        self.assertIn("**8.79 MB** / **9.50 MB** limit", markdown)
        self.assertIn("Headroom: **0.71 MB (7.4%)**", markdown)
        self.assertIn("Required package contents verified.", markdown)

    def test_oversized_markdown_includes_exceeded_amount(self) -> None:
        markdown = REPORT.render_markdown(self.result(size=9_600_000))

        self.assertIn("## ❌ Package checks", markdown)
        self.assertIn("Exceeded by: **0.10 MB**", markdown)
        self.assertIn("exceeds the configured size ceiling", markdown)

    def test_content_failure_markdown_names_each_problem(self) -> None:
        markdown = REPORT.render_markdown(
            self.result(
                missing=("Cargo.lock",),
                forbidden=("docs/index.md",),
            )
        )

        self.assertIn("## ❌ Package checks", markdown)
        self.assertIn("Package-content policy failed.", markdown)
        self.assertIn("- `Cargo.lock`", markdown)
        self.assertIn("- `docs/index.md`", markdown)

    def test_combined_failure_reports_size_and_content(self) -> None:
        markdown = REPORT.render_markdown(
            self.result(size=9_500_001, missing=("src/main.rs",))
        )

        self.assertIn("Exceeded by: **0.00 MB**", markdown)
        self.assertIn("- `src/main.rs`", markdown)

    def test_structured_and_markdown_outputs_are_deterministic(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            result = self.result()
            first_json = root / "first.json"
            first_md = root / "first.md"
            second_json = root / "second.json"
            second_md = root / "second.md"

            REPORT.write_outputs(result, first_json, first_md)
            REPORT.write_outputs(result, second_json, second_md)

            self.assertEqual(first_json.read_bytes(), second_json.read_bytes())
            self.assertEqual(first_md.read_bytes(), second_md.read_bytes())
            self.assertTrue(json.loads(first_json.read_text())["policy_ok"])


class EnforcementTests(unittest.TestCase):
    """Confirm policy enforcement is separate from report generation."""

    def test_policy_failure_report_exists_before_enforcement_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "package.crate"
            structured = root / "report.json"
            markdown = root / "report.md"
            write_archive(archive, REPORT.REQUIRED_PATHS[:-1])
            result = REPORT.analyse_archive(archive, archive.stat().st_size - 1)
            REPORT.write_outputs(result, structured, markdown)
            argv = [str(SCRIPT), "enforce", str(structured)]

            with mock.patch.object(sys, "argv", argv):
                status = REPORT.main()

            self.assertEqual(status, 1)
            self.assertIn("## ❌ Package checks", markdown.read_text())
            self.assertIn("Missing required paths", markdown.read_text())


if __name__ == "__main__":
    unittest.main()
