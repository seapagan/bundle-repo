"""Extract advisory maintainability findings from Clippy output."""

from __future__ import annotations

import argparse
import json
import posixpath
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ANSI_ESCAPE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
LOCATION = re.compile(r"^\s*-->\s+(.+):(\d+):(\d+)\s*$")
FUNCTION = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(")
RATIO = re.compile(r"\((\d+)/(\d+)\)")
LINT_MARKERS = {
    "clippy::too_many_lines": ("#too_many_lines", "clippy::too-many-lines"),
    "clippy::too_many_arguments": (
        "#too_many_arguments",
        "clippy::too-many-arguments",
    ),
}


@dataclass(frozen=True)
class Finding:
    """One maintainability diagnostic rendered for Markdown."""

    path: str
    line: int | None
    function: str
    lint: str
    observed: int | None
    threshold: int | None
    unit: str | None
    message: str | None

    @property
    def location(self) -> str:
        """Return the normalized source location."""
        if self.line is None:
            return self.path
        return f"{self.path}:{self.line}"

    @property
    def detail(self) -> str:
        """Return the observed value and threshold when available."""
        if self.observed is not None and self.threshold is not None and self.unit:
            return f"{self.observed}/{self.threshold} {self.unit}"
        return self.message or "diagnostic detail not reported"

    def sort_key(self) -> tuple[str, int, str, str, int, int, str, str]:
        """Return a deterministic ordering key for reports and comments."""
        return (
            self.path,
            self.line if self.line is not None else -1,
            self.function,
            self.lint,
            self.observed if self.observed is not None else -1,
            self.threshold if self.threshold is not None else -1,
            self.unit or "",
            self.message or "",
        )

    def to_dict(self) -> dict[str, Any]:
        """Return a JSON-serializable diagnostic identity."""
        return {
            "path": self.path,
            "line": self.line,
            "function": self.function,
            "lint": self.lint,
            "observed": self.observed,
            "threshold": self.threshold,
            "unit": self.unit,
            "message": self.message,
        }


@dataclass(frozen=True)
class PlatformResult:
    """Structured advisory findings from one native platform."""

    platform: str
    findings: tuple[Finding, ...]


def warning_blocks(output: str) -> list[list[str]]:
    """Split compiler output into warning diagnostic blocks."""
    blocks: list[list[str]] = []
    current: list[str] = []
    for line in output.splitlines():
        if line.startswith("warning:"):
            if current:
                blocks.append(current)
            current = [line]
        elif current:
            current.append(line)
    if current:
        blocks.append(current)
    return blocks


def lint_for(block: str) -> str | None:
    """Return the advisory lint identified by a diagnostic block."""
    for lint, markers in LINT_MARKERS.items():
        if any(marker in block for marker in markers):
            return lint
    return None


def parse_finding(lines: list[str]) -> Finding | None:
    """Parse one advisory diagnostic without matching unrelated warnings."""
    block = "\n".join(lines)
    lint = lint_for(block)
    if lint is None:
        return None

    location_match = next(
        (match for line in lines if (match := LOCATION.match(line))),
        None,
    )
    function_match = FUNCTION.search(block)
    ratio_match = RATIO.search(lines[0])

    if location_match:
        path = posixpath.normpath(location_match.group(1).replace("\\", "/"))
        line = int(location_match.group(2))
    else:
        path = "location not reported"
        line = None

    function = function_match.group(1) if function_match else "function not reported"
    if ratio_match:
        unit = "lines" if lint.endswith("too_many_lines") else "arguments"
        observed = int(ratio_match.group(1))
        threshold = int(ratio_match.group(2))
        message = None
    else:
        observed = None
        threshold = None
        unit = None
        message = lines[0].removeprefix("warning: ")

    return Finding(
        path,
        line,
        function,
        lint,
        observed,
        threshold,
        unit,
        message,
    )


def extract_findings(output: str) -> list[Finding]:
    """Extract and deduplicate configured maintainability findings."""
    clean_output = ANSI_ESCAPE.sub("", output)
    findings = {
        finding
        for block in warning_blocks(clean_output)
        if (finding := parse_finding(block)) is not None
    }
    return sorted(findings, key=Finding.sort_key)


def render_report(platform: str, findings: list[Finding]) -> str:
    """Render one platform's advisory report."""
    lines = [f"### {platform}", ""]
    if not findings:
        lines.append("✅ No advisory maintainability findings")
        return "\n".join(lines) + "\n"

    count = len(findings)
    noun = "finding" if count == 1 else "findings"
    lines.extend([f"⚠️ {count} advisory maintainability {noun}", ""])
    for finding in findings:
        lines.append(
            f"- `{finding.location}` — `{finding.function}` — "
            f"`{finding.lint}` — {finding.detail}"
        )
    return "\n".join(lines) + "\n"


def write_platform_result(
    output: Path,
    platform: str,
    findings: list[Finding],
) -> None:
    """Write one platform's findings as deterministic JSON."""
    data = {
        "platform": platform,
        "findings": [finding.to_dict() for finding in findings],
    }
    output.write_text(
        json.dumps(data, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def load_platform_result(path: Path) -> PlatformResult:
    """Load one structured platform report."""
    data = json.loads(path.read_text(encoding="utf-8"))
    findings = tuple(Finding(**finding) for finding in data["findings"])
    return PlatformResult(data["platform"], findings)


def platform_heading(
    platforms: frozenset[str],
    all_platforms: frozenset[str],
) -> str:
    """Describe the exact native platform set for a finding group."""
    if platforms == all_platforms:
        return "All platforms"
    names = sorted(platforms, key=str.casefold)
    if len(names) == 1:
        return f"{names[0]} only"
    return " + ".join(names)


def render_combined_comment(results: list[PlatformResult]) -> str:
    """Render one sticky comment with findings grouped by platform set."""
    all_platforms = frozenset(result.platform for result in results)
    finding_platforms: dict[Finding, set[str]] = {}
    for result in results:
        for finding in result.findings:
            finding_platforms.setdefault(finding, set()).add(result.platform)

    grouped: dict[frozenset[str], list[Finding]] = {}
    for finding, platforms in finding_platforms.items():
        grouped.setdefault(frozenset(platforms), []).append(finding)

    icon = "⚠️" if finding_platforms else "✅"
    lines = [
        "<!-- bundlerepo-advisory-quality -->",
        "",
        f"## {icon} Advisory maintainability",
        "",
        "Clippy maintainability checks are advisory and do not block merging.",
    ]
    group_order = sorted(
        grouped,
        key=lambda platforms: (
            platforms != all_platforms,
            tuple(name.casefold() for name in sorted(platforms)),
        ),
    )
    for platforms in group_order:
        lines.extend(["", f"### {platform_heading(platforms, all_platforms)}", ""])
        for finding in sorted(grouped[platforms], key=Finding.sort_key):
            lines.append(
                f"- `{finding.location}` — `{finding.function}` — "
                f"`{finding.lint}` — {finding.detail}"
            )
    lines.extend(
        [
            "",
            "Thresholds:",
            "",
            "- function lines: 60",
            "- arguments: 8",
        ]
    )
    return "\n".join(lines) + "\n"


def parse_args() -> argparse.Namespace:
    """Parse platform-report or combined-comment command arguments."""
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)

    platform = commands.add_parser("platform")
    platform.add_argument("input", type=Path)
    platform.add_argument("markdown_output", type=Path)
    platform.add_argument("json_output", type=Path)
    platform.add_argument("platform")

    combine = commands.add_parser("combine")
    combine.add_argument("output", type=Path)
    combine.add_argument("inputs", type=Path, nargs="+")
    return parser.parse_args()


def main() -> None:
    """Write one platform result or an aggregated sticky comment."""
    args = parse_args()
    if args.command == "platform":
        findings = extract_findings(args.input.read_text(encoding="utf-8"))
        args.markdown_output.write_text(
            render_report(args.platform, findings),
            encoding="utf-8",
        )
        write_platform_result(args.json_output, args.platform, findings)
        print(len(findings))
        return

    results = [load_platform_result(path) for path in args.inputs]
    args.output.write_text(render_combined_comment(results), encoding="utf-8")


if __name__ == "__main__":
    main()
