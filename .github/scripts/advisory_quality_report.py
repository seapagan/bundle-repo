#!/usr/bin/env python3
"""Extract advisory maintainability findings from Clippy output."""

from __future__ import annotations

import argparse
import posixpath
import re
from dataclasses import dataclass
from pathlib import Path

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

    lint: str
    location: str
    function: str
    detail: str


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
        location = f"{path}:{location_match.group(2)}"
    else:
        location = "location not reported"

    function = function_match.group(1) if function_match else "function not reported"
    if ratio_match:
        unit = "lines" if lint.endswith("too_many_lines") else "arguments"
        detail = f"{ratio_match.group(1)}/{ratio_match.group(2)} {unit}"
    else:
        detail = lines[0].removeprefix("warning: ")

    return Finding(lint, location, function, detail)


def extract_findings(output: str) -> list[Finding]:
    """Extract and deduplicate configured maintainability findings."""
    clean_output = ANSI_ESCAPE.sub("", output)
    findings = {
        finding
        for block in warning_blocks(clean_output)
        if (finding := parse_finding(block)) is not None
    }
    return sorted(findings, key=lambda finding: (finding.location, finding.lint))


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


def main() -> None:
    """Write a Markdown report and print its advisory finding count."""
    parser = argparse.ArgumentParser()
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("platform")
    args = parser.parse_args()

    findings = extract_findings(args.input.read_text(encoding="utf-8"))
    args.output.write_text(
        render_report(args.platform, findings),
        encoding="utf-8",
    )
    print(len(findings))


if __name__ == "__main__":
    main()
