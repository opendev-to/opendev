"""Tool for editing existing files with 9-pass fuzzy matching."""

import logging
import re
import shutil
from difflib import SequenceMatcher
from pathlib import Path
from typing import Optional, TYPE_CHECKING

from swecli.models.config import AppConfig
from swecli.models.operation import EditResult, Operation
from swecli.core.context_engineering.tools.implementations.base import BaseTool
from swecli.core.context_engineering.tools.implementations.diff_preview import DiffPreview, Diff

if TYPE_CHECKING:
    from swecli.core.runtime.task_monitor import TaskMonitor

_LOG = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# 9-pass replacer chain (inspired by OpenCode's edit.ts)
# ---------------------------------------------------------------------------

def _similarity(a: str, b: str) -> float:
    """Compute Levenshtein-like similarity ratio between two strings."""
    return SequenceMatcher(None, a, b).ratio()


def _normalize_line_endings(s: str) -> str:
    return s.replace("\r\n", "\n").replace("\r", "\n")


def _extract_actual_lines(original_lines: list[str], start: int, count: int) -> str:
    """Extract *count* lines from original starting at *start*, joined with newline."""
    end = min(start + count, len(original_lines))
    return "\n".join(original_lines[start:end])


class _Replacer:
    """Base class for a single replacer strategy."""

    name: str = "base"

    def find(self, original: str, old_content: str) -> Optional[str]:
        """Return the actual substring in *original* that matches *old_content*, or None."""
        raise NotImplementedError


class _SimpleReplacer(_Replacer):
    """Pass 1: exact string match."""

    name = "simple"

    def find(self, original: str, old_content: str) -> Optional[str]:
        if old_content in original:
            return old_content
        return None


class _LineTrimmedReplacer(_Replacer):
    """Pass 2: match with each line trimmed (preserve original indentation)."""

    name = "line_trimmed"

    def find(self, original: str, old_content: str) -> Optional[str]:
        old_lines = [ln.strip() for ln in old_content.split("\n")]
        if not old_lines or not any(old_lines):
            return None

        original_lines = original.split("\n")

        for i, line in enumerate(original_lines):
            if line.strip() == old_lines[0]:
                if i + len(old_lines) > len(original_lines):
                    continue
                match = True
                for j, old_ln in enumerate(old_lines):
                    if original_lines[i + j].strip() != old_ln:
                        match = False
                        break
                if match:
                    actual = "\n".join(original_lines[i : i + len(old_lines)])
                    if actual in original:
                        return actual
        return None


class _BlockAnchorReplacer(_Replacer):
    """Pass 3: first/last lines must match exactly (trimmed); middle lines use similarity."""

    name = "block_anchor"
    THRESHOLD_SINGLE = 0.0
    THRESHOLD_MULTI = 0.3

    def find(self, original: str, old_content: str) -> Optional[str]:
        old_lines = old_content.split("\n")
        if len(old_lines) < 3:
            return None  # Need at least first, middle, last

        first_trimmed = old_lines[0].strip()
        last_trimmed = old_lines[-1].strip()
        middle_old = [ln.strip() for ln in old_lines[1:-1]]

        original_lines = original.split("\n")
        candidates: list[tuple[int, float]] = []  # (start_index, avg_similarity)

        for i, line in enumerate(original_lines):
            if line.strip() != first_trimmed:
                continue
            # Search for matching last line within a reasonable window
            window_end = min(i + len(old_lines) * 2, len(original_lines))
            for end_idx in range(i + len(old_lines) - 1, window_end):
                if end_idx >= len(original_lines):
                    break
                if original_lines[end_idx].strip() != last_trimmed:
                    continue
                # Check middle lines similarity
                middle_orig = [ln.strip() for ln in original_lines[i + 1 : end_idx]]
                if not middle_old and not middle_orig:
                    candidates.append((i, 1.0))
                    continue
                if not middle_old or not middle_orig:
                    continue
                sim = _similarity("\n".join(middle_old), "\n".join(middle_orig))
                candidates.append((i, sim))

        if not candidates:
            return None

        threshold = self.THRESHOLD_SINGLE if len(candidates) == 1 else self.THRESHOLD_MULTI
        # Pick best candidate above threshold
        best = max(candidates, key=lambda c: c[1])
        if best[1] < threshold:
            return None

        start = best[0]
        # Find the end index again for the best match
        for end_idx in range(start + len(old_lines) - 1, min(start + len(old_lines) * 2, len(original_lines))):
            if end_idx >= len(original_lines):
                break
            if original_lines[end_idx].strip() == last_trimmed:
                actual = "\n".join(original_lines[start : end_idx + 1])
                if actual in original:
                    return actual
        return None


class _WhitespaceNormalizedReplacer(_Replacer):
    """Pass 4: normalize all whitespace (collapse runs, strip lines)."""

    name = "whitespace_normalized"

    def _normalize(self, s: str) -> str:
        lines = s.split("\n")
        return "\n".join(re.sub(r"\s+", " ", ln).strip() for ln in lines)

    def find(self, original: str, old_content: str) -> Optional[str]:
        norm_old = self._normalize(old_content)
        original_lines = original.split("\n")
        old_line_count = len(old_content.split("\n"))

        for i in range(len(original_lines)):
            end = min(i + old_line_count + 2, len(original_lines))
            for j in range(i + old_line_count - 1, end + 1):
                if j > len(original_lines):
                    break
                candidate = "\n".join(original_lines[i:j])
                if self._normalize(candidate) == norm_old:
                    if candidate in original:
                        return candidate
        return None


class _IndentationFlexibleReplacer(_Replacer):
    """Pass 5: ignore indentation differences entirely."""

    name = "indentation_flexible"

    def find(self, original: str, old_content: str) -> Optional[str]:
        old_stripped = [ln.strip() for ln in old_content.split("\n") if ln.strip()]
        if not old_stripped:
            return None

        original_lines = original.split("\n")

        for i, line in enumerate(original_lines):
            if line.strip() != old_stripped[0]:
                continue
            # Try to match all stripped lines, skipping blank original lines
            matched_indices: list[int] = []
            j = 0
            for k in range(i, min(i + len(old_stripped) * 3, len(original_lines))):
                if j >= len(old_stripped):
                    break
                if not original_lines[k].strip():
                    continue  # Skip blank lines in original
                if original_lines[k].strip() == old_stripped[j]:
                    matched_indices.append(k)
                    j += 1
                else:
                    break

            if j == len(old_stripped) and matched_indices:
                start = matched_indices[0]
                end = matched_indices[-1] + 1
                actual = "\n".join(original_lines[start:end])
                if actual in original:
                    return actual
        return None


class _EscapeNormalizedReplacer(_Replacer):
    """Pass 6: unescape common escape sequences before matching."""

    name = "escape_normalized"

    _ESCAPES = {r"\\n": "\n", r"\\t": "\t", r"\\\\": "\\", r"\\\"": '"', r"\\'": "'"}

    def _unescape(self, s: str) -> str:
        result = s
        for escaped, unescaped in self._ESCAPES.items():
            result = result.replace(escaped, unescaped)
        return result

    def find(self, original: str, old_content: str) -> Optional[str]:
        unescaped = self._unescape(old_content)
        if unescaped == old_content:
            return None  # No escapes to normalize
        if unescaped in original:
            return unescaped
        return None


class _TrimmedBoundaryReplacer(_Replacer):
    """Pass 7: find trimmed boundaries and expand to full lines."""

    name = "trimmed_boundary"

    def find(self, original: str, old_content: str) -> Optional[str]:
        trimmed = old_content.strip()
        if trimmed == old_content:
            return None  # Nothing to trim

        if trimmed in original:
            return trimmed

        # Try line-level boundary expansion
        old_lines = old_content.split("\n")
        first_content = old_lines[0].strip()
        last_content = old_lines[-1].strip()

        if not first_content or not last_content:
            return None

        original_lines = original.split("\n")
        for i, line in enumerate(original_lines):
            if first_content not in line:
                continue
            end = min(i + len(old_lines) + 2, len(original_lines))
            for j in range(i + 1, end):
                if j >= len(original_lines):
                    break
                if last_content not in original_lines[j]:
                    continue
                candidate = "\n".join(original_lines[i : j + 1])
                if candidate in original:
                    return candidate
        return None


class _ContextAwareReplacer(_Replacer):
    """Pass 8: use surrounding context blocks for matching."""

    name = "context_aware"

    def find(self, original: str, old_content: str) -> Optional[str]:
        old_lines = old_content.split("\n")
        if len(old_lines) < 2:
            return None

        original_lines = original.split("\n")

        # Use first and last non-empty lines as context anchors
        first_ctx = next((ln.strip() for ln in old_lines if ln.strip()), None)
        last_ctx = next((ln.strip() for ln in reversed(old_lines) if ln.strip()), None)
        if not first_ctx or not last_ctx:
            return None

        # Find all positions of first anchor
        starts: list[int] = []
        for i, line in enumerate(original_lines):
            if first_ctx in line.strip():
                starts.append(i)

        if not starts:
            return None

        best_match: Optional[str] = None
        best_sim = 0.0

        for start in starts:
            # Search for end anchor
            for end in range(start + 1, min(start + len(old_lines) * 2, len(original_lines))):
                if last_ctx in original_lines[end].strip():
                    candidate = "\n".join(original_lines[start : end + 1])
                    sim = _similarity(old_content.strip(), candidate.strip())
                    if sim > best_sim and sim > 0.5:
                        best_sim = sim
                        best_match = candidate
                    break  # Only check first end anchor per start

        if best_match and best_match in original:
            return best_match
        return None


class _MultiOccurrenceReplacer(_Replacer):
    """Pass 9: find all exact matches when there are multiple (for match_all mode)."""

    name = "multi_occurrence"

    def find(self, original: str, old_content: str) -> Optional[str]:
        # This pass is a last-resort that tries trimmed exact match
        trimmed = old_content.strip()
        if not trimmed:
            return None

        # Find the trimmed content in original lines
        original_lines = original.split("\n")
        trimmed_lines = trimmed.split("\n")

        for i in range(len(original_lines) - len(trimmed_lines) + 1):
            candidate_lines = original_lines[i : i + len(trimmed_lines)]
            if all(
                a.strip() == b.strip()
                for a, b in zip(candidate_lines, trimmed_lines)
            ):
                candidate = "\n".join(candidate_lines)
                if candidate in original:
                    return candidate
        return None


# Ordered chain of replacers (tried in sequence)
_REPLACER_CHAIN: list[_Replacer] = [
    _SimpleReplacer(),
    _LineTrimmedReplacer(),
    _BlockAnchorReplacer(),
    _WhitespaceNormalizedReplacer(),
    _IndentationFlexibleReplacer(),
    _EscapeNormalizedReplacer(),
    _TrimmedBoundaryReplacer(),
    _ContextAwareReplacer(),
    _MultiOccurrenceReplacer(),
]


class EditTool(BaseTool):
    """Tool for editing existing files with diff preview."""

    @property
    def name(self) -> str:
        """Tool name."""
        return "edit_file"

    @property
    def description(self) -> str:
        """Tool description."""
        return "Edit an existing file with search and replace"

    def _find_content(self, original: str, old_content: str) -> tuple[bool, str]:
        """Find content in file using 9-pass fuzzy matching chain.

        Tries 9 different matching strategies in order of strictness:
        1. SimpleReplacer - exact string match
        2. LineTrimmedReplacer - match trimmed lines
        3. BlockAnchorReplacer - anchor first/last lines, similarity for middle
        4. WhitespaceNormalizedReplacer - normalize all whitespace
        5. IndentationFlexibleReplacer - ignore indentation differences
        6. EscapeNormalizedReplacer - unescape common escape sequences
        7. TrimmedBoundaryReplacer - find trimmed boundaries
        8. ContextAwareReplacer - use surrounding context for matching
        9. MultiOccurrenceReplacer - trimmed exact match as last resort

        Args:
            original: The original file content
            old_content: The content to find

        Returns:
            (found, actual_content) - actual_content is what should be replaced
        """
        original = _normalize_line_endings(original)
        old_content = _normalize_line_endings(old_content)

        for replacer in _REPLACER_CHAIN:
            result = replacer.find(original, old_content)
            if result is not None:
                if replacer.name != "simple":
                    _LOG.debug("Edit matched via %s replacer", replacer.name)
                return (True, result)

        return (False, old_content)

    def __init__(self, config: AppConfig, working_dir: Path):
        """Initialize edit tool.

        Args:
            config: Application configuration
            working_dir: Working directory for operations
        """
        self.config = config
        self.working_dir = working_dir
        self.diff_preview = DiffPreview()

    def edit_file(
        self,
        file_path: str,
        old_content: str,
        new_content: str,
        match_all: bool = False,
        dry_run: bool = False,
        backup: bool = True,
        operation: Optional[Operation] = None,
        task_monitor: Optional["TaskMonitor"] = None,
    ) -> EditResult:
        """Edit file by replacing old_content with new_content.

        Args:
            file_path: Path to file to edit
            old_content: Content to find and replace
            new_content: New content to insert
            match_all: Replace all occurrences (default: first only)
            dry_run: If True, don't actually modify file
            backup: Create backup before editing
            operation: Operation object for tracking
            task_monitor: Optional task monitor for interrupt checking

        Returns:
            EditResult with operation details

        Raises:
            FileNotFoundError: If file doesn't exist
            PermissionError: If edit is not permitted
            ValueError: If old_content not found or not unique
        """
        # Check for interrupt before starting
        if task_monitor and task_monitor.should_interrupt():
            error = "Interrupted before edit"
            if operation:
                operation.mark_failed(error)
            return EditResult(
                success=False,
                file_path=file_path,
                lines_added=0,
                lines_removed=0,
                error=error,
                operation_id=operation.id if operation else None,
                interrupted=True,
            )

        # Resolve path
        path = self._resolve_path(file_path)

        # Check if file exists
        if not path.exists():
            error = f"File not found: {path}"
            if operation:
                operation.mark_failed(error)
            return EditResult(
                success=False,
                file_path=str(path),
                lines_added=0,
                lines_removed=0,
                error=error,
                operation_id=operation.id if operation else None,
            )

        # Check write permissions
        if not self.config.permissions.file_write.is_allowed(str(path)):
            error = f"Editing {path} is not permitted by configuration"
            if operation:
                operation.mark_failed(error)
            return EditResult(
                success=False,
                file_path=str(path),
                lines_added=0,
                lines_removed=0,
                error=error,
                operation_id=operation.id if operation else None,
            )

        try:
            # Read original content
            with open(path, "r", encoding="utf-8") as f:
                original = f.read()

            # Find old_content with fuzzy matching fallback
            found, actual_old_content = self._find_content(original, old_content)
            if not found:
                error = f"Content not found in file: {old_content[:50]}..."
                if operation:
                    operation.mark_failed(error)
                return EditResult(
                    success=False,
                    file_path=str(path),
                    lines_added=0,
                    lines_removed=0,
                    error=error,
                    operation_id=operation.id if operation else None,
                )

            # Use the actual content found in file for subsequent operations
            old_content = actual_old_content

            # Check if old_content is unique (if not match_all)
            count = original.count(old_content)
            if not match_all and count > 1:
                # Find line numbers of each occurrence to help LLM provide more context
                occurrences = []
                search_pos = 0
                for _ in range(count):
                    pos = original.find(old_content, search_pos)
                    if pos == -1:
                        break
                    line_num = original[:pos].count("\n") + 1
                    occurrences.append(line_num)
                    search_pos = pos + 1

                locations = ", ".join(f"line {n}" for n in occurrences)
                error = f"Content appears {count} times at {locations}. Provide more surrounding context in old_content to uniquely identify which occurrence to edit."
                if operation:
                    operation.mark_failed(error)
                return EditResult(
                    success=False,
                    file_path=str(path),
                    lines_added=0,
                    lines_removed=0,
                    error=error,
                    operation_id=operation.id if operation else None,
                )

            # Perform replacement
            if match_all:
                modified = original.replace(old_content, new_content)
            else:
                modified = original.replace(old_content, new_content, 1)

            # Calculate diff statistics and textual diff
            diff = Diff(str(path), original, modified)
            stats = diff.get_stats()
            diff_text = diff.generate_unified_diff(context_lines=3)

            # Dry run - don't actually write
            if dry_run:
                return EditResult(
                    success=True,
                    file_path=str(path),
                    lines_added=stats["lines_added"],
                    lines_removed=stats["lines_removed"],
                    diff=diff_text,
                    operation_id=operation.id if operation else None,
                )

            # Mark operation as executing
            if operation:
                operation.mark_executing()

            # Create backup if requested
            backup_path = None
            if backup and self.config.operation.backup_before_edit:
                backup_path = str(path) + ".bak"
                shutil.copy2(path, backup_path)

            # Write modified content
            with open(path, "w", encoding="utf-8") as f:
                f.write(modified)

            # Mark operation as successful
            if operation:
                operation.mark_success()

            return EditResult(
                success=True,
                file_path=str(path),
                lines_added=stats["lines_added"],
                lines_removed=stats["lines_removed"],
                backup_path=backup_path,
                diff=diff_text,
                operation_id=operation.id if operation else None,
            )

        except Exception as e:
            error = f"Failed to edit file: {str(e)}"
            if operation:
                operation.mark_failed(error)
            return EditResult(
                success=False,
                file_path=str(path),
                lines_added=0,
                lines_removed=0,
                error=error,
                operation_id=operation.id if operation else None,
            )

    def edit_lines(
        self,
        file_path: str,
        line_start: int,
        line_end: int,
        new_content: str,
        dry_run: bool = False,
        backup: bool = True,
        operation: Optional[Operation] = None,
    ) -> EditResult:
        """Edit specific lines in a file.

        Args:
            file_path: Path to file
            line_start: Starting line (1-indexed, inclusive)
            line_end: Ending line (1-indexed, inclusive)
            new_content: New content for those lines
            dry_run: If True, don't actually modify file
            backup: Create backup before editing
            operation: Operation object for tracking

        Returns:
            EditResult with operation details
        """
        path = self._resolve_path(file_path)

        # Check if file exists
        if not path.exists():
            error = f"File not found: {path}"
            if operation:
                operation.mark_failed(error)
            return EditResult(
                success=False,
                file_path=str(path),
                lines_added=0,
                lines_removed=0,
                error=error,
                operation_id=operation.id if operation else None,
            )

        try:
            # Read file
            with open(path, "r", encoding="utf-8") as f:
                lines = f.readlines()

            # Validate line numbers
            if line_start < 1 or line_end > len(lines) or line_start > line_end:
                error = f"Invalid line range: {line_start}-{line_end} (file has {len(lines)} lines)"
                if operation:
                    operation.mark_failed(error)
                return EditResult(
                    success=False,
                    file_path=str(path),
                    lines_added=0,
                    lines_removed=0,
                    error=error,
                    operation_id=operation.id if operation else None,
                )

            # Build old and new content
            original = "".join(lines)
            old_lines = lines[line_start - 1 : line_end]
            old_content = "".join(old_lines)

            # Replace lines
            new_lines = (
                lines[: line_start - 1]
                + [new_content if not new_content.endswith("\n") else new_content]
                + lines[line_end:]
            )

            if not new_content.endswith("\n") and line_end < len(lines):
                new_lines[line_start - 1] += "\n"

            modified = "".join(new_lines)

            # Use the main edit_file method
            return self.edit_file(
                file_path=file_path,
                old_content=old_content,
                new_content=new_content if new_content.endswith("\n") else new_content + "\n",
                match_all=False,
                dry_run=dry_run,
                backup=backup,
                operation=operation,
            )

        except Exception as e:
            error = f"Failed to edit lines: {str(e)}"
            if operation:
                operation.mark_failed(error)
            return EditResult(
                success=False,
                file_path=str(path),
                lines_added=0,
                lines_removed=0,
                error=error,
                operation_id=operation.id if operation else None,
            )

    def execute(self, **kwargs) -> EditResult:
        """Execute the tool.

        Args:
            **kwargs: Arguments for edit_file

        Returns:
            EditResult
        """
        return self.edit_file(**kwargs)

    def preview_edit(self, file_path: str, old_content: str, new_content: str) -> None:
        """Preview an edit operation.

        Args:
            file_path: Path to file
            old_content: Content to replace
            new_content: New content
        """
        path = self._resolve_path(file_path)

        # Read original
        with open(path, "r", encoding="utf-8") as f:
            original = f.read()

        # Generate modified
        modified = original.replace(old_content, new_content, 1)

        # Display diff
        self.diff_preview.preview_edit(str(path), original, modified)

    def _resolve_path(self, path: str) -> Path:
        """Resolve a path relative to working directory.

        Args:
            path: Path string (relative or absolute)

        Returns:
            Resolved Path object
        """
        p = Path(path)
        if p.is_absolute():
            return p
        return (self.working_dir / p).resolve()
