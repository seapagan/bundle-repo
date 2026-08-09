"""Tests for Cargo package analysis and publishability reporting."""

from __future__ import annotations

import importlib.util
import io
import json
import subprocess
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

DEFAULT_DISTRIBUTION_PATHS = (
    "resources/tokenizers/model.json",
    "src/main.rs",
)


def write_archive(
    path: Path,
    files: tuple[str, ...],
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


def initialise_repository(
    root: Path,
    tracked: tuple[str, ...] = DEFAULT_DISTRIBUTION_PATHS,
    *,
    untracked: tuple[str, ...] = (),
) -> None:
    """Create a repository index with controlled tracked and local files."""
    subprocess.run(
        ("git", "init", "--quiet", str(root)),
        check=True,
        capture_output=True,
    )
    for name in tracked + untracked:
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f"fixture for {name}\n", encoding="utf-8")
    if tracked:
        subprocess.run(
            ("git", "-C", str(root), "add", "--", *tracked),
            check=True,
            capture_output=True,
        )


class PackageAnalysisTests(unittest.TestCase):
    """Exercise archive measurement, structure, and content policy."""

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.archive = self.root / "bundle_repo-0.6.0.crate"
        self.repository = self.root / "repository"
        initialise_repository(self.repository)
        self.required = REPORT.required_paths(self.repository)

    def analyse(self, files: tuple[str, ...] | None = None) -> object:
        """Write and analyse one controlled package."""
        write_archive(self.archive, self.required if files is None else files)
        return REPORT.analyse_archive(
            self.archive,
            1_000_000,
            self.repository,
        )

    def test_measures_exact_archive_bytes(self) -> None:
        result = self.analyse()

        self.assertEqual(result.size_bytes, self.archive.stat().st_size)

    def test_below_limit_has_headroom(self) -> None:
        result = self.analyse()

        self.assertTrue(result.size_ok)
        self.assertEqual(result.headroom_bytes, 1_000_000 - result.size_bytes)
        self.assertEqual(result.exceeded_by_bytes, 0)

    def test_exactly_at_limit_passes(self) -> None:
        write_archive(self.archive, self.required)
        size = self.archive.stat().st_size

        result = REPORT.analyse_archive(self.archive, size, self.repository)

        self.assertTrue(result.size_ok)
        self.assertEqual(result.headroom_bytes, 0)

    def test_above_limit_records_exceeded_bytes(self) -> None:
        write_archive(self.archive, self.required)
        size = self.archive.stat().st_size

        result = REPORT.analyse_archive(self.archive, size - 7, self.repository)

        self.assertFalse(result.size_ok)
        self.assertEqual(result.exceeded_by_bytes, 7)
        self.assertEqual(result.headroom_bytes, 0)

    def test_required_content_succeeds(self) -> None:
        result = self.analyse()

        self.assertTrue(result.contents_ok)
        self.assertEqual(result.missing_paths, ())
        self.assertIn("src/main.rs", result.required_paths)
        self.assertIn("resources/tokenizers/model.json", result.required_paths)

    def test_missing_required_content_is_reported(self) -> None:
        missing = "README-cratesio.md"
        files = tuple(path for path in self.required if path != missing)

        result = self.analyse(files)

        self.assertFalse(result.contents_ok)
        self.assertEqual(result.missing_paths, (missing,))

    def test_missing_dynamically_discovered_content_is_reported(self) -> None:
        added = "resources/tokenizers/new-model.json"
        path = self.repository / added
        path.write_text("new model\n", encoding="utf-8")
        subprocess.run(
            ("git", "-C", str(self.repository), "add", "--", added),
            check=True,
            capture_output=True,
        )

        result = self.analyse()

        self.assertFalse(result.contents_ok)
        self.assertEqual(result.missing_paths, (added,))

    def test_allowed_extra_archive_content_is_not_rejected(self) -> None:
        result = self.analyse(self.required + ("package-metadata.txt",))

        self.assertTrue(result.contents_ok)
        self.assertNotIn("package-metadata.txt", result.forbidden_paths)

    def test_forbidden_content_is_reported(self) -> None:
        result = self.analyse(
            self.required
            + (
                "docs/index.md",
                ".github/workflows/test.yml",
                "tests/crate/cli.rs",
                ".vscode/settings.json",
                "README.md",
                "Makefile.toml",
            )
        )

        self.assertEqual(
            result.forbidden_paths,
            (
                ".github/workflows/test.yml",
                ".vscode/settings.json",
                "Makefile.toml",
                "README.md",
                "docs/index.md",
                "tests/crate/cli.rs",
            ),
        )

    def test_forbidden_policy_covers_repository_only_roots(self) -> None:
        expected = {
            ".github",
            ".vscode",
            "docs",
            "tests",
            ".gitattributes",
            ".gitignore",
            ".markdownlint.yaml",
            ".rustfmt.toml",
            "CHANGELOG.md",
            "Makefile.toml",
            "README.md",
            "TODO.md",
            "clippy.toml",
            "deny.toml",
            "renovate.json",
        }

        self.assertEqual(set(REPORT.FORBIDDEN_PREFIXES), expected)

    def test_empty_forbidden_directory_is_reported(self) -> None:
        write_archive(self.archive, self.required, directories=("docs",))

        result = REPORT.analyse_archive(
            self.archive,
            repository=self.repository,
        )

        self.assertEqual(result.forbidden_paths, ("docs",))

    def test_archive_root_is_removed_from_policy_paths(self) -> None:
        write_archive(
            self.archive,
            ("./.cargo_vcs_info.json",) + self.required[1:],
            root="renamed-package-1.2.3",
        )

        result = REPORT.analyse_archive(
            self.archive,
            repository=self.repository,
        )

        self.assertEqual(result.archive_root, "renamed-package-1.2.3")
        self.assertNotIn(".cargo_vcs_info.json", result.missing_paths)

    def test_multiple_archive_roots_are_rejected(self) -> None:
        with tarfile.open(self.archive, mode="w:gz") as archive:
            for name in ("first/Cargo.toml", "second/Cargo.lock"):
                info = tarfile.TarInfo(name)
                archive.addfile(info, io.BytesIO())

        with self.assertRaisesRegex(REPORT.ArchiveError, "one archive root"):
            REPORT.analyse_archive(self.archive, repository=self.repository)

    def test_duplicate_archive_paths_are_rejected(self) -> None:
        with tarfile.open(self.archive, mode="w:gz") as archive:
            for _ in range(2):
                info = tarfile.TarInfo("package/Cargo.toml")
                archive.addfile(info, io.BytesIO())

        with self.assertRaisesRegex(REPORT.ArchiveError, "duplicate"):
            REPORT.analyse_archive(self.archive, repository=self.repository)

    def test_unsafe_archive_path_is_rejected(self) -> None:
        with tarfile.open(self.archive, mode="w:gz") as archive:
            info = tarfile.TarInfo("package/../Cargo.toml")
            archive.addfile(info, io.BytesIO())

        with self.assertRaisesRegex(REPORT.ArchiveError, "unsafe"):
            REPORT.analyse_archive(self.archive, repository=self.repository)

    def test_unsupported_archive_member_is_rejected(self) -> None:
        with tarfile.open(self.archive, mode="w:gz") as archive:
            info = tarfile.TarInfo("package/src/link.rs")
            info.type = tarfile.SYMTYPE
            info.linkname = "main.rs"
            archive.addfile(info)

        with self.assertRaisesRegex(REPORT.ArchiveError, "unsupported"):
            REPORT.analyse_archive(self.archive, repository=self.repository)

    def test_malformed_archive_is_rejected(self) -> None:
        self.archive.write_bytes(b"not a gzip tar archive")

        with self.assertRaisesRegex(REPORT.ArchiveError, "cannot read"):
            REPORT.analyse_archive(self.archive, repository=self.repository)

    def test_unreadable_archive_is_rejected(self) -> None:
        with self.assertRaisesRegex(REPORT.ArchiveError, "cannot measure"):
            REPORT.analyse_archive(
                self.root / "missing.crate",
                repository=self.repository,
            )


class RepositoryDistributionTests(unittest.TestCase):
    """Exercise tracked source and resource discovery independently of Cargo."""

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.repository = Path(self.temporary.name) / "repository"
        initialise_repository(self.repository)

    def add_tracked_file(self, name: str) -> None:
        """Add a controlled path to the repository index."""
        path = self.repository / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f"fixture for {name}\n", encoding="utf-8")
        subprocess.run(
            ("git", "-C", str(self.repository), "add", "--", name),
            check=True,
            capture_output=True,
        )

    def test_static_required_paths_are_limited_to_special_files(self) -> None:
        self.assertEqual(
            REPORT.STATIC_REQUIRED_PATHS,
            (
                ".cargo_vcs_info.json",
                "Cargo.lock",
                "Cargo.toml",
                "Cargo.toml.orig",
                "LICENSE.txt",
                "README-cratesio.md",
            ),
        )

    def test_tracked_source_and_resources_are_required(self) -> None:
        self.assertEqual(
            REPORT.repository_distribution_paths(self.repository),
            DEFAULT_DISTRIBUTION_PATHS,
        )

    def test_new_resource_file_automatically_becomes_required(self) -> None:
        added = "resources/tokenizers/licenses/New-License.txt"
        self.add_tracked_file(added)

        self.assertIn(added, REPORT.required_paths(self.repository))

    def test_new_source_file_automatically_becomes_required(self) -> None:
        added = "src/new_module.rs"
        self.add_tracked_file(added)

        self.assertIn(added, REPORT.required_paths(self.repository))

    def test_results_are_sorted_deterministically(self) -> None:
        self.add_tracked_file("src/z.rs")
        self.add_tracked_file("resources/a.txt")

        first = REPORT.repository_distribution_paths(self.repository)
        second = REPORT.repository_distribution_paths(self.repository)

        self.assertEqual(first, tuple(sorted(first)))
        self.assertEqual(first, second)

    def test_paths_use_archive_forward_slashes(self) -> None:
        added = "resources/tokenizers/licenses/nested/License.txt"
        self.add_tracked_file(added)

        paths = REPORT.repository_distribution_paths(self.repository)

        self.assertIn(added, paths)
        self.assertNotIn("resources\\tokenizers\\licenses\\nested\\License.txt", paths)

    def test_untracked_local_files_do_not_become_required(self) -> None:
        noise = self.repository / "src" / "local_noise.rs"
        noise.write_text("local only\n", encoding="utf-8")

        self.assertNotIn(
            "src/local_noise.rs",
            REPORT.required_paths(self.repository),
        )

    def test_unsupported_tracked_entry_mode_fails_clearly(self) -> None:
        output = b"120000 " + (b"0" * 40) + b" 0\tsrc/link.rs\0"
        completed = subprocess.CompletedProcess((), 0, stdout=output, stderr=b"")

        with mock.patch.object(REPORT.subprocess, "run", return_value=completed):
            with self.assertRaisesRegex(
                REPORT.ArchiveError,
                "unsupported tracked distribution entry mode 120000: src/link.rs",
            ):
                REPORT.repository_distribution_paths(self.repository)


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
            required_paths=REPORT.required_paths(ROOT),
            missing_paths=missing,
            forbidden_prefixes=REPORT.FORBIDDEN_PREFIXES,
            forbidden_paths=forbidden,
        )

    def test_decimal_mb_rendering(self) -> None:
        self.assertEqual(REPORT.decimal_mb(9_500_000), "9.50 MB")
        self.assertEqual(REPORT.decimal_mb(707_364), "0.71 MB")

    def test_healthy_markdown_includes_size_headroom_and_percentage(self) -> None:
        markdown = REPORT.render_markdown(self.result())

        self.assertIn("## Package checks ✅", markdown)
        self.assertNotIn("## ✅ Package checks", markdown)
        self.assertIn("**8.79 MB** / **9.50 MB** limit", markdown)
        self.assertIn("Headroom: **0.71 MB (7.4%)**", markdown)
        self.assertIn("Required package contents verified.", markdown)

    def test_oversized_markdown_includes_exceeded_amount(self) -> None:
        markdown = REPORT.render_markdown(self.result(size=9_600_000))

        self.assertIn("## Package checks ❌", markdown)
        self.assertNotIn("## ❌ Package checks", markdown)
        self.assertIn("Exceeded by: **0.10 MB**", markdown)
        self.assertIn("exceeds the configured size ceiling", markdown)

    def test_content_failure_markdown_names_each_problem(self) -> None:
        markdown = REPORT.render_markdown(
            self.result(
                missing=("Cargo.lock",),
                forbidden=("docs/index.md",),
            )
        )

        self.assertIn("## Package checks ❌", markdown)
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
            required = REPORT.required_paths(ROOT)
            write_archive(archive, required[:-1])
            result = REPORT.analyse_archive(archive, archive.stat().st_size - 1)
            REPORT.write_outputs(result, structured, markdown)
            argv = [str(SCRIPT), "enforce", str(structured)]

            with mock.patch.object(sys, "argv", argv):
                status = REPORT.main()

            self.assertEqual(status, 1)
            self.assertIn("## Package checks ❌", markdown.read_text())
            self.assertIn("Missing required paths", markdown.read_text())


if __name__ == "__main__":
    unittest.main()
