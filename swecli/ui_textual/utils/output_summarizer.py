"""Output summarizer for generating intelligent summaries of command output."""

from __future__ import annotations

import re
from typing import List, Optional, Tuple


def summarize_output(lines: List[str], output_type: str = "generic") -> str:
    """Generate an intelligent summary of output based on content patterns.

    Detects common output types (test results, package managers, etc.) and
    generates contextual summaries.

    Args:
        lines: Output lines to summarize.
        output_type: Hint for output type ('bash', 'tool_result', 'file_content', 'generic').

    Returns:
        Summary string like "5 passed, 2 failed" or "142 lines".
    """
    if not lines:
        return "empty output"

    text = "\n".join(lines)
    line_count = len(lines)

    # Try pattern-specific summaries in priority order
    summary = (
        _detect_pytest_summary(text)
        or _detect_jest_summary(text)
        or _detect_npm_summary(text)
        or _detect_pip_summary(text)
        or _detect_cargo_summary(text)
        or _detect_git_log_summary(lines)
        or _detect_git_status_summary(text)
        or _detect_make_summary(text)
        or _detect_error_summary(text, line_count)
    )

    if summary:
        return summary

    # Generic line count fallback
    return _format_line_count(line_count)


def _format_line_count(count: int) -> str:
    """Format line count as summary."""
    if count == 1:
        return "1 line"
    return f"{count} lines"


def _detect_pytest_summary(text: str) -> Optional[str]:
    """Detect pytest output and extract pass/fail counts.

    Matches patterns like:
    - "5 passed"
    - "5 passed, 2 failed"
    - "5 passed, 1 warning"
    - "===== 5 passed in 1.23s ====="
    """
    # Match pytest summary line with counts
    match = re.search(
        r"(?:=+\s*)?(\d+)\s+passed(?:,?\s*(\d+)\s+failed)?(?:,?\s*(\d+)\s+(?:warning|error)s?)?",
        text,
        re.IGNORECASE,
    )
    if match:
        passed = int(match.group(1))
        failed = int(match.group(2)) if match.group(2) else 0
        warnings = int(match.group(3)) if match.group(3) else 0

        parts = [f"{passed} passed"]
        if failed:
            parts.append(f"{failed} failed")
        if warnings:
            parts.append(f"{warnings} warnings")
        return ", ".join(parts)

    # Also check for "FAILED" or "PASSED" markers
    if "FAILED" in text or "ERRORS" in text:
        failed_count = len(re.findall(r"FAILED\s+", text))
        if failed_count > 0:
            return f"{failed_count} failed"

    return None


def _detect_jest_summary(text: str) -> Optional[str]:
    """Detect Jest/Mocha test output.

    Matches patterns like:
    - "Tests: 5 passed, 2 failed"
    - "5 passing"
    - "2 failing"
    """
    # Jest summary
    match = re.search(r"Tests:\s*(\d+)\s+passed(?:,\s*(\d+)\s+failed)?", text, re.IGNORECASE)
    if match:
        passed = int(match.group(1))
        failed = int(match.group(2)) if match.group(2) else 0
        if failed:
            return f"{passed} passed, {failed} failed"
        return f"{passed} passed"

    # Mocha style
    passing = re.search(r"(\d+)\s+passing", text, re.IGNORECASE)
    failing = re.search(r"(\d+)\s+failing", text, re.IGNORECASE)
    if passing or failing:
        parts = []
        if passing:
            parts.append(f"{int(passing.group(1))} passed")
        if failing:
            parts.append(f"{int(failing.group(1))} failed")
        return ", ".join(parts)

    return None


def _detect_npm_summary(text: str) -> Optional[str]:
    """Detect npm install output.

    Matches patterns like:
    - "added 142 packages"
    - "removed 5 packages"
    - "up to date"
    """
    if "up to date" in text.lower():
        return "up to date"

    match = re.search(r"added\s+(\d+)\s+packages?", text, re.IGNORECASE)
    if match:
        count = int(match.group(1))
        return f"{count} packages installed"

    match = re.search(r"removed\s+(\d+)\s+packages?", text, re.IGNORECASE)
    if match:
        count = int(match.group(1))
        return f"{count} packages removed"

    return None


def _detect_pip_summary(text: str) -> Optional[str]:
    """Detect pip install output.

    Matches patterns like:
    - "Successfully installed package-1.0"
    - "Requirement already satisfied"
    """
    if "Requirement already satisfied" in text:
        return "requirements satisfied"

    match = re.search(r"Successfully installed\s+(.+)", text)
    if match:
        packages = match.group(1).split()
        count = len(packages)
        if count == 1:
            return f"installed {packages[0]}"
        return f"{count} packages installed"

    return None


def _detect_cargo_summary(text: str) -> Optional[str]:
    """Detect Cargo (Rust) build/test output.

    Matches patterns like:
    - "test result: ok. 5 passed; 0 failed"
    - "Compiling project v0.1.0"
    - "Finished release [optimized] target(s)"
    """
    # Test results
    match = re.search(
        r"test result:\s*\w+\.\s*(\d+)\s+passed;\s*(\d+)\s+failed", text, re.IGNORECASE
    )
    if match:
        passed = int(match.group(1))
        failed = int(match.group(2))
        if failed:
            return f"{passed} passed, {failed} failed"
        return f"{passed} passed"

    # Build completion
    if "Finished" in text and ("release" in text.lower() or "debug" in text.lower()):
        return "build complete"

    return None


def _detect_git_log_summary(lines: List[str]) -> Optional[str]:
    """Detect git log output by counting commits.

    Looks for commit hash patterns.
    """
    commit_count = 0
    for line in lines:
        # Match commit hash at start of line (short or long format)
        if re.match(r"^[a-f0-9]{7,40}\s", line) or line.startswith("commit "):
            commit_count += 1

    if commit_count > 0:
        if commit_count == 1:
            return "1 commit"
        return f"{commit_count} commits"

    return None


def _detect_git_status_summary(text: str) -> Optional[str]:
    """Detect git status output.

    Summarizes modified/staged/untracked counts.
    """
    if "nothing to commit" in text.lower():
        return "clean working tree"

    modified = len(re.findall(r"^\s*M\s+", text, re.MULTILINE))
    staged = len(re.findall(r"^\s*[AMDRC]\s+", text, re.MULTILINE))
    untracked = len(re.findall(r"^\?\?\s+", text, re.MULTILINE))

    if modified or staged or untracked:
        parts = []
        if modified:
            parts.append(f"{modified} modified")
        if staged:
            parts.append(f"{staged} staged")
        if untracked:
            parts.append(f"{untracked} untracked")
        return ", ".join(parts)

    return None


def _detect_make_summary(text: str) -> Optional[str]:
    """Detect make/build output.

    Matches patterns like:
    - "make: Nothing to be done"
    - "Build succeeded"
    """
    if "Nothing to be done" in text:
        return "nothing to build"

    if "Build succeeded" in text.lower() or "build successful" in text.lower():
        return "build succeeded"

    if "error:" in text.lower() and "make" in text.lower():
        error_count = text.lower().count("error:")
        return f"{error_count} build errors"

    return None


def _detect_error_summary(text: str, line_count: int) -> Optional[str]:
    """Detect error patterns for generic error summary."""
    # Check for obvious error indicators
    error_keywords = ["error", "exception", "traceback", "failed", "fatal"]
    text_lower = text.lower()

    for keyword in error_keywords:
        if keyword in text_lower:
            error_lines = sum(1 for line in text.split("\n") if keyword in line.lower())
            if error_lines > 0:
                return f"{line_count} lines (errors detected)"

    return None


def get_expansion_hint() -> str:
    """Get the hint text for expanding collapsed output."""
    return "(ctrl+o to expand)"
