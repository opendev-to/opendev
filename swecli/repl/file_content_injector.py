"""File content injection for @ mentions with structured XML tags."""

from __future__ import annotations

import os
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any, Optional

if TYPE_CHECKING:
    from swecli.models.config import AppConfig


@dataclass
class InjectionResult:
    """Result of file content injection."""

    text_content: str
    """XML-tagged content for text injection."""

    image_blocks: list[dict] = field(default_factory=list)
    """Multimodal image blocks for API (base64 encoded)."""

    errors: list[str] = field(default_factory=list)
    """Error messages for failed references."""


class FileContentInjector:
    """Handles @ mention file content injection with structured XML tags.

    Supports:
    - Text files: Injected with <file_content> tag
    - Large files: Truncated with head/tail in <file_truncated> tag
    - Directories: Tree listing in <directory_listing> tag
    - PDFs: Extracted text in <pdf_content> tag
    - Images: Multimodal blocks for vision models

    Example:
        injector = FileContentInjector(file_ops, config, working_dir)
        result = injector.inject_content("analyze @main.py and @src/")
        # result.text_content contains XML-tagged file contents
        # result.image_blocks contains base64-encoded images for multimodal
    """

    # Safe text extensions to auto-inject
    SAFE_TEXT_EXTENSIONS = {
        ".py", ".js", ".ts", ".jsx", ".tsx", ".java", ".go", ".rs",
        ".c", ".cpp", ".h", ".hpp", ".cs", ".rb", ".php", ".swift",
        ".md", ".txt", ".json", ".yaml", ".yml", ".toml", ".xml",
        ".html", ".css", ".scss", ".less", ".sh", ".bash", ".zsh",
        ".gitignore", ".dockerignore", ".env.example",
    }

    # Special filenames (no extension but still text)
    SAFE_FILENAMES = {
        "Dockerfile", "Makefile", "Rakefile", "Gemfile", "Procfile",
        "README", "LICENSE", "CHANGELOG", "CONTRIBUTING", "AUTHORS",
    }

    # Image extensions for multimodal
    IMAGE_EXTENSIONS = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"}

    # Thresholds
    MAX_FILE_SIZE = 50 * 1024  # 50KB
    MAX_LINES = 1000
    HEAD_LINES = 100
    TAIL_LINES = 50
    MAX_DIR_DEPTH = 3
    MAX_DIR_ITEMS = 50

    # Extension to language mapping for syntax highlighting hints
    LANG_MAP = {
        ".py": "python", ".js": "javascript", ".ts": "typescript",
        ".jsx": "jsx", ".tsx": "tsx", ".java": "java", ".go": "go",
        ".rs": "rust", ".c": "c", ".cpp": "cpp", ".h": "c", ".hpp": "cpp",
        ".cs": "csharp", ".rb": "ruby", ".php": "php", ".swift": "swift",
        ".md": "markdown", ".json": "json", ".yaml": "yaml", ".yml": "yaml",
        ".toml": "toml", ".xml": "xml", ".html": "html", ".css": "css",
        ".scss": "scss", ".less": "less", ".sh": "bash", ".bash": "bash",
        ".zsh": "zsh", ".sql": "sql", ".graphql": "graphql",
    }

    def __init__(
        self,
        file_ops: Any,
        config: Optional["AppConfig"],
        working_dir: Path,
    ):
        """Initialize the file content injector.

        Args:
            file_ops: File operations interface (must have read_file method)
            config: Application configuration (for VLM availability check)
            working_dir: Working directory for resolving relative paths
        """
        self.file_ops = file_ops
        self.config = config
        self.working_dir = Path(working_dir).resolve()

    def inject_content(self, query: str) -> InjectionResult:
        """Extract @ references and inject file contents.

        Args:
            query: User query potentially containing @ file references

        Returns:
            InjectionResult with text content and optional image blocks
        """
        refs = self._extract_refs(query)

        if not refs:
            return InjectionResult(text_content="")

        text_parts: list[str] = []
        image_blocks: list[dict] = []
        errors: list[str] = []

        for ref_str, path in refs:
            try:
                if not path.exists():
                    text_parts.append(
                        f'<file_error path="{ref_str}" reason="File not found" />'
                    )
                    errors.append(f"File not found: {ref_str}")
                    continue

                if path.is_dir():
                    text_parts.append(self._process_directory(path, ref_str))
                elif path.suffix.lower() == ".pdf":
                    text_parts.append(self._process_pdf(path, ref_str))
                elif path.suffix.lower() in self.IMAGE_EXTENSIONS:
                    tag, block = self._process_image(path, ref_str)
                    text_parts.append(tag)
                    if block:
                        image_blocks.append(block)
                elif self._is_text_file(path):
                    text_parts.append(self._process_text_file(path, ref_str))
                else:
                    text_parts.append(
                        f'<file_error path="{ref_str}" reason="Unsupported file type" />'
                    )

            except PermissionError:
                text_parts.append(
                    f'<file_error path="{ref_str}" reason="Permission denied" />'
                )
                errors.append(f"Permission denied: {ref_str}")
            except Exception as e:
                text_parts.append(
                    f'<file_error path="{ref_str}" reason="{str(e)}" />'
                )
                errors.append(f"Error reading {ref_str}: {e}")

        return InjectionResult(
            text_content="\n\n".join(text_parts),
            image_blocks=image_blocks,
            errors=errors,
        )

    def _extract_refs(self, query: str) -> list[tuple[str, Path]]:
        """Extract file references from query.

        Supports:
        - Unquoted paths: @main.py, @src/utils.py
        - Quoted paths: @"path with spaces/file.py"

        Excludes email addresses like user@example.com.

        Args:
            query: User query string

        Returns:
            List of (original_ref, resolved_path) tuples, deduplicated
        """
        refs: list[str] = []
        seen: set[str] = set()

        # Pattern 1: Quoted paths @"path with spaces/file.py"
        for match in re.finditer(r'@"([^"]+)"', query):
            ref = match.group(1)
            if ref not in seen:
                refs.append(ref)
                seen.add(ref)

        # Pattern 2: Unquoted paths
        # Match @ followed by path-like characters, but only if @ is:
        # - At start of string, or
        # - Preceded by whitespace or punctuation (not alphanumeric)
        # This avoids matching emails like user@example.com
        for match in re.finditer(r'(?:^|(?<=\s)|(?<=[^\w]))@([a-zA-Z0-9_./\-]+)', query):
            ref = match.group(1)
            if ref not in seen:
                refs.append(ref)
                seen.add(ref)

        return [(r, self._resolve_path(r)) for r in refs]

    def _resolve_path(self, ref: str) -> Path:
        """Resolve a file reference to an absolute path.

        Args:
            ref: File reference (relative or absolute)

        Returns:
            Resolved absolute Path
        """
        path = Path(ref).expanduser()
        if not path.is_absolute():
            path = self.working_dir / path
        return path.resolve()

    def _is_text_file(self, path: Path) -> bool:
        """Check if a file is a text file that can be injected.

        First checks known safe extensions, then falls back to binary detection
        for unknown extensions.

        Args:
            path: Path to the file

        Returns:
            True if the file should be treated as text
        """
        ext = path.suffix.lower()
        name = path.name

        # Known safe extensions
        if ext in self.SAFE_TEXT_EXTENSIONS or name in self.SAFE_FILENAMES:
            return True

        # Known binary extensions - skip detection
        binary_extensions = {
            ".exe", ".dll", ".so", ".dylib", ".bin", ".dat",
            ".zip", ".tar", ".gz", ".bz2", ".xz", ".7z", ".rar",
            ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".ico", ".svg",
            ".mp3", ".mp4", ".avi", ".mov", ".wav", ".flac",
            ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
            ".pyc", ".pyo", ".class", ".o", ".obj",
            ".woff", ".woff2", ".ttf", ".otf", ".eot",
            ".sqlite", ".db", ".sqlite3",
        }
        if ext in binary_extensions:
            return False

        # For unknown extensions, try to detect if it's text
        return self._detect_text_file(path)

    def _detect_text_file(self, path: Path, sample_size: int = 8192) -> bool:
        """Detect if a file is text by checking for binary content.

        Args:
            path: Path to the file
            sample_size: Number of bytes to sample

        Returns:
            True if the file appears to be text
        """
        try:
            with open(path, "rb") as f:
                sample = f.read(sample_size)

            if not sample:
                return True  # Empty file is "text"

            # Check for null bytes (strong binary indicator)
            if b"\x00" in sample:
                return False

            # Check for high ratio of non-printable characters
            # Text files should be mostly printable ASCII or valid UTF-8
            try:
                sample.decode("utf-8")
                return True
            except UnicodeDecodeError:
                pass

            # Try other common encodings
            for encoding in ["latin-1", "cp1252"]:
                try:
                    sample.decode(encoding)
                    # Check if result looks like text (mostly printable)
                    text = sample.decode(encoding)
                    printable = sum(1 for c in text if c.isprintable() or c in "\n\r\t")
                    if printable / len(text) > 0.85:  # 85% printable threshold
                        return True
                except (UnicodeDecodeError, ZeroDivisionError):
                    pass

            return False

        except (OSError, IOError):
            return False

    def _get_language(self, path: Path) -> str:
        """Get the language identifier for syntax highlighting.

        Args:
            path: Path to the file

        Returns:
            Language string or empty string if unknown
        """
        ext = path.suffix.lower()
        return self.LANG_MAP.get(ext, "")

    def _format_size(self, size: int) -> str:
        """Format a file size in human-readable format.

        Args:
            size: Size in bytes

        Returns:
            Formatted string like "1.2KB" or "3.4MB"
        """
        if size < 1024:
            return f"{size}B"
        elif size < 1024 * 1024:
            return f"{size / 1024:.1f}KB"
        else:
            return f"{size / (1024 * 1024):.1f}MB"

    def _process_text_file(self, path: Path, ref_str: str) -> str:
        """Process a text file and return XML-tagged content.

        Args:
            path: Absolute path to the file
            ref_str: Original reference string for display

        Returns:
            XML-tagged file content
        """
        try:
            content = self.file_ops.read_file(str(path))
        except Exception:
            # Fallback to direct read if file_ops fails
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                content = f.read()

        lines = content.splitlines()
        line_count = len(lines)
        size = len(content)

        # Check if file needs truncation
        if size > self.MAX_FILE_SIZE or line_count > self.MAX_LINES:
            return self._process_large_file(path, ref_str, content, lines)

        language = self._get_language(path)
        lang_attr = f' language="{language}"' if language else ""

        # Include absolute path so model knows exactly where file is
        abs_path = str(path)

        from swecli.core.agents.prompts.reminders import get_reminder

        warning = get_reminder("file_exists_warning")
        return (
            f'<file_content path="{ref_str}" absolute_path="{abs_path}" exists="true"{lang_attr}>\n'
            f'{warning}\n'
            f'{content}\n'
            f'</file_content>'
        )

    def _process_large_file(
        self,
        path: Path,
        ref_str: str,
        content: str,
        lines: list[str],
    ) -> str:
        """Process a large file with head/tail truncation.

        Args:
            path: Absolute path to the file
            ref_str: Original reference string
            content: Full file content
            lines: Content split into lines

        Returns:
            XML-tagged truncated content
        """
        total_lines = len(lines)
        head = lines[:self.HEAD_LINES]
        tail = lines[-self.TAIL_LINES:]
        omitted = total_lines - self.HEAD_LINES - self.TAIL_LINES

        language = self._get_language(path)
        lang_attr = f' language="{language}"' if language else ""
        abs_path = str(path)

        head_content = "\n".join(head)
        tail_content = "\n".join(tail)

        from swecli.core.agents.prompts.reminders import get_reminder

        warning = get_reminder("file_exists_warning")
        return (
            f'<file_truncated path="{ref_str}" absolute_path="{abs_path}" exists="true" total_lines="{total_lines}"{lang_attr}>\n'
            f'{warning}\n'
            f'=== HEAD (lines 1-{self.HEAD_LINES}) ===\n'
            f'{head_content}\n\n'
            f'=== TRUNCATED ({omitted} lines omitted) ===\n\n'
            f'=== TAIL (lines {total_lines - self.TAIL_LINES + 1}-{total_lines}) ===\n'
            f'{tail_content}\n'
            f'</file_truncated>'
        )

    def _process_directory(self, path: Path, ref_str: str) -> str:
        """Process a directory and return tree-style listing.

        Args:
            path: Absolute path to the directory
            ref_str: Original reference string

        Returns:
            XML-tagged directory listing
        """
        # Import GitIgnoreParser for filtering
        try:
            from swecli.ui_textual.autocomplete_internal.gitignore import GitIgnoreParser
            gitignore = GitIgnoreParser(self.working_dir)
        except ImportError:
            gitignore = None

        def should_skip(item_path: Path) -> bool:
            """Check if an item should be skipped."""
            if gitignore and gitignore.is_ignored(item_path):
                return True
            # Also check always-ignore directories
            if item_path.is_dir():
                from swecli.ui_textual.autocomplete_internal.gitignore import GitIgnoreParser
                if item_path.name in GitIgnoreParser.ALWAYS_IGNORE_DIRS:
                    return True
            return False

        def build_tree(dir_path: Path, prefix: str = "", depth: int = 0) -> list[str]:
            """Recursively build directory tree."""
            if depth > self.MAX_DIR_DEPTH:
                return [f"{prefix}└── ..."]

            try:
                items = sorted(dir_path.iterdir(), key=lambda p: (not p.is_dir(), p.name.lower()))
            except PermissionError:
                return [f"{prefix}└── [permission denied]"]
            except OSError as e:
                return [f"{prefix}└── [error: {e}]"]

            # Filter and limit items
            items = [i for i in items if not should_skip(i)][:self.MAX_DIR_ITEMS]

            tree_lines: list[str] = []
            for i, item in enumerate(items):
                is_last = i == len(items) - 1
                connector = "└── " if is_last else "├── "
                new_prefix = prefix + ("    " if is_last else "│   ")

                if item.is_dir():
                    tree_lines.append(f"{prefix}{connector}{item.name}/")
                    tree_lines.extend(build_tree(item, new_prefix, depth + 1))
                else:
                    try:
                        size = self._format_size(item.stat().st_size)
                        tree_lines.append(f"{prefix}{connector}{item.name} ({size})")
                    except OSError:
                        tree_lines.append(f"{prefix}{connector}{item.name}")

            return tree_lines

        tree = build_tree(path)
        item_count = len([line for line in tree if not line.endswith("...")])

        return (
            f'<directory_listing path="{ref_str}" count="{item_count}">\n'
            f'{path.name}/\n'
            + "\n".join(tree) + "\n"
            f'</directory_listing>'
        )

    def _process_pdf(self, path: Path, ref_str: str) -> str:
        """Process a PDF file and return extracted text.

        Args:
            path: Absolute path to the PDF
            ref_str: Original reference string

        Returns:
            XML-tagged PDF content
        """
        try:
            from swecli.core.context_engineering.tools.implementations.pdf_tool import PDFTool
            pdf_tool = PDFTool(self.working_dir)
            result = pdf_tool.extract_text(str(path))

            if not result["success"]:
                return f'<file_error path="{ref_str}" reason="{result["error"]}" />'

            pages = result.get("page_count", "?")
            content = result.get("content", "")

            return (
                f'<pdf_content path="{ref_str}" pages="{pages}">\n'
                f'{content}\n'
                f'</pdf_content>'
            )

        except ImportError:
            return (
                f'<file_error path="{ref_str}" '
                f'reason="PDF support not available. Install pypdf." />'
            )
        except Exception as e:
            return f'<file_error path="{ref_str}" reason="PDF extraction failed: {e}" />'

    def _process_image(self, path: Path, ref_str: str) -> tuple[str, Optional[dict]]:
        """Process an image file for multimodal injection.

        Args:
            path: Absolute path to the image
            ref_str: Original reference string

        Returns:
            Tuple of (text_tag, image_block_or_none)
        """
        try:
            from swecli.core.context_engineering.tools.implementations.vlm_tool import VLMTool

            vlm = VLMTool(self.config, self.working_dir)

            if not vlm.is_available():
                return (
                    f'<file_error path="{ref_str}" reason="Vision model not configured. '
                    f'Use /models to set a vision model to analyze images." />',
                    None,
                )

            base64_data = vlm.encode_image(str(path))
            if not base64_data:
                return (
                    f'<file_error path="{ref_str}" reason="Failed to read image file" />',
                    None,
                )

            ext = path.suffix.lower()
            mime_types = {
                ".png": "image/png",
                ".jpg": "image/jpeg",
                ".jpeg": "image/jpeg",
                ".gif": "image/gif",
                ".webp": "image/webp",
                ".bmp": "image/bmp",
            }
            mime_type = mime_types.get(ext, "image/png")

            tag = (
                f'<image path="{ref_str}" type="{mime_type}">\n'
                f'[Image attached as multimodal content]\n'
                f'</image>'
            )

            image_block = {
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": mime_type,
                    "data": base64_data,
                },
            }

            return tag, image_block

        except ImportError:
            return (
                f'<file_error path="{ref_str}" reason="VLM tool not available" />',
                None,
            )
        except Exception as e:
            return (
                f'<file_error path="{ref_str}" reason="Image processing failed: {e}" />',
                None,
            )
