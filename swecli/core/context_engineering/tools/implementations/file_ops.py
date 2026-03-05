"""File operation tools for reading, searching, and navigating codebases."""

import re
import subprocess
from pathlib import Path
from typing import Optional

from swecli.models.config import AppConfig

# Default directories/patterns to exclude from search
# Covers 20+ programming languages and ecosystems
DEFAULT_SEARCH_EXCLUDES = [
    # ===== Package/Dependency Directories =====
    "node_modules",        # JavaScript/TypeScript (npm, yarn, pnpm, bun)
    "bower_components",    # Bower (legacy JS)
    "jspm_packages",       # JSPM
    "vendor",              # Go, PHP (Composer), Ruby (Bundler)
    "Pods",                # Swift/Objective-C (CocoaPods)
    ".bundle",             # Ruby Bundler
    "packages",            # Dart/Flutter, .NET
    ".pub-cache",          # Dart pub
    ".pub",                # Dart pub
    "deps",                # Elixir Mix
    ".nuget",              # .NET NuGet
    ".m2",                 # Java Maven, Clojure

    # ===== Virtual Environments =====
    ".venv",               # Python (standard)
    "venv",                # Python (common)
    "env",                 # Python (common)
    ".env",                # Python/Node env dirs
    "ENV",                 # Python
    ".virtualenvs",        # virtualenvwrapper
    ".conda",              # Conda environments

    # ===== Build Output Directories =====
    "build",               # Universal (C/C++, Python, Gradle, etc.)
    "dist",                # Universal (JS, Python, Haskell)
    "out",                 # TypeScript, Android, general
    "target",              # Rust (Cargo), Java (Maven), Scala (sbt), Clojure
    "bin",                 # .NET, Go, general compiled output
    "obj",                 # .NET intermediate
    "lib",                 # Compiled libraries
    "_build",              # Elixir, Erlang
    "ebin",                # Erlang compiled
    "dist-newstyle",       # Haskell Cabal
    ".build",              # Swift Package Manager
    "DerivedData",         # Xcode
    "CMakeFiles",          # CMake build artifacts
    ".cmake",              # CMake cache

    # ===== Framework-Specific Build =====
    ".next",               # Next.js
    ".nuxt",               # Nuxt.js
    ".angular",            # Angular CLI
    ".svelte-kit",         # SvelteKit
    ".vuepress",           # VuePress
    ".gatsby-cache",       # Gatsby
    ".parcel-cache",       # Parcel bundler
    ".turbo",              # Turborepo
    "dist_electron",       # Electron

    # ===== Cache Directories =====
    ".cache",              # Universal cache
    "__pycache__",         # Python bytecode
    ".pytest_cache",       # Pytest
    ".mypy_cache",         # Mypy type checker
    ".ruff_cache",         # Ruff linter
    ".hypothesis",         # Hypothesis testing
    ".tox",                # Tox testing
    ".nox",                # Nox testing
    ".eslintcache",        # ESLint
    ".stylelintcache",     # Stylelint
    ".gradle",             # Gradle
    ".dart_tool",          # Dart
    ".mix",                # Elixir
    ".cpcache",            # Clojure
    ".lsp",                # Clojure LSP

    # ===== IDE/Editor Directories =====
    ".idea",               # JetBrains IDEs
    ".vscode",             # VS Code
    ".vscode-test",        # VS Code extension testing
    ".vs",                 # Visual Studio
    ".metadata",           # Eclipse
    ".settings",           # Eclipse
    "xcuserdata",          # Xcode user data
    ".netbeans",           # NetBeans

    # ===== Version Control =====
    ".git",                # Git
    ".svn",                # Subversion
    ".hg",                 # Mercurial

    # ===== Coverage/Testing Output =====
    "coverage",            # Universal coverage
    "htmlcov",             # Python coverage HTML
    ".nyc_output",         # NYC (Istanbul) coverage

    # ===== Language-Specific Metadata =====
    ".eggs",               # Python eggs
    ".Rproj.user",         # R Studio
    ".julia",              # Julia packages
    "_opam",               # OCaml
    ".cabal-sandbox",      # Haskell Cabal sandbox
    ".stack-work",         # Haskell Stack
    "blib",                # Perl build

    # ===== Generated/Minified Files (glob patterns) =====
    "*.min.js",            # Minified JavaScript
    "*.min.css",           # Minified CSS
    "*.bundle.js",         # Bundled JavaScript
    "*.chunk.js",          # Webpack chunks
    "*.map",               # Source maps
    "*.pyc",               # Python compiled
    "*.pyo",               # Python optimized
    "*.class",             # Java compiled
    "*.o",                 # C/C++ object files
    "*.so",                # Shared libraries
    "*.dylib",             # macOS dynamic libraries
    "*.dll",               # Windows DLLs
    "*.exe",               # Windows executables
    "*.beam",              # Erlang/Elixir compiled
    "*.hi",                # Haskell interface
    "*.dyn_hi",            # Haskell dynamic interface
    "*.dyn_o",             # Haskell dynamic object
    "*.egg-info",          # Python egg info
]


class FileOperations:
    """Tools for file operations."""

    def __init__(self, config: AppConfig, working_dir: Path):
        """Initialize file operations.

        Args:
            config: Application configuration
            working_dir: Working directory for operations
        """
        self.config = config
        self.working_dir = working_dir

    def _is_excluded_path(self, file_path: str) -> bool:
        """Check if path contains any excluded directory or matches excluded patterns."""
        path_obj = Path(file_path)
        path_parts = path_obj.parts

        for exclude in DEFAULT_SEARCH_EXCLUDES:
            if exclude.startswith("*"):
                # Glob pattern like *.min.js
                if path_obj.match(exclude):
                    return True
            elif exclude in path_parts:
                return True
        return False

    # Truncation constants
    DEFAULT_MAX_LINES = 2000
    MAX_LINE_LENGTH = 2000
    MAX_LIST_ENTRIES = 500

    @staticmethod
    def _is_binary_file(path: Path) -> bool:
        """Check if a file is binary by looking for null bytes in the first 8192 bytes."""
        try:
            with open(path, "rb") as f:
                chunk = f.read(8192)
            return b"\x00" in chunk
        except OSError:
            return False

    def read_file(
        self,
        file_path: str,
        line_start: Optional[int] = None,
        line_end: Optional[int] = None,
        offset: Optional[int] = None,
        max_lines: Optional[int] = None,
    ) -> str:
        """Read a file's contents with line-numbered output and truncation.

        Args:
            file_path: Path to the file (relative or absolute).
            line_start: Optional starting line number (1-indexed). Alias for offset.
            line_end: Optional ending line number (1-indexed, inclusive).
            offset: Optional 1-indexed line number to start reading from.
            max_lines: Maximum number of lines to return (default 2000).

        Returns:
            File contents in ``cat -n`` format with line numbers.

        Raises:
            FileNotFoundError: If file doesn't exist.
            PermissionError: If file read is not permitted.
        """
        path = self._resolve_path(file_path)

        # Check permissions
        if not self.config.permissions.file_read.is_allowed(str(path)):
            raise PermissionError(f"Reading {path} is not permitted")

        if not path.exists():
            raise FileNotFoundError(f"File not found: {path}")

        # Detect binary files
        if self._is_binary_file(path):
            return f"Error: {path} appears to be a binary file. Cannot display binary content."

        # Determine effective start/limit
        effective_offset = offset or line_start or 1
        if effective_offset < 1:
            effective_offset = 1

        effective_max = max_lines if max_lines is not None else self.DEFAULT_MAX_LINES
        if line_end is not None:
            effective_max = line_end - effective_offset + 1

        with open(path, "r", encoding="utf-8", errors="ignore") as f:
            all_lines = f.readlines()

        total_lines = len(all_lines)
        start_idx = effective_offset - 1
        end_idx = min(start_idx + effective_max, total_lines)
        selected = all_lines[start_idx:end_idx]

        # Format as cat -n style with line numbers
        output_parts: list[str] = []
        for i, line in enumerate(selected, start=effective_offset):
            text = line.rstrip("\n")
            # Truncate long lines
            if len(text) > self.MAX_LINE_LENGTH:
                text = text[: self.MAX_LINE_LENGTH] + "... (line truncated)"
            output_parts.append(f"  {i}\t{text}")

        result = "\n".join(output_parts)

        # Add truncation message if we didn't show everything
        if end_idx < total_lines:
            result += (
                f"\n... (truncated: showing lines {effective_offset}-{end_idx}"
                f" of {total_lines}. Use offset/max_lines to see more.)"
            )

        return result

    def glob_files(
        self,
        pattern: str,
        max_results: int = 100,
        base_path: Optional[Path] = None,
    ) -> list[str]:
        """Find files matching a glob pattern.

        Args:
            pattern: Glob pattern (e.g., "**/*.py", "src/**/*.ts")
            max_results: Maximum number of results to return
            base_path: Optional base directory to run the glob from

        Returns:
            List of matching file paths (relative to working_dir)
        """
        matches = []
        search_root = base_path or self.working_dir
        try:
            iterator = search_root.glob(pattern)
        except NotImplementedError:
            return [f"Error: Non-relative pattern '{pattern}' is not supported"]
        except Exception as e:
            return [f"Error: {str(e)}"]

        for path in iterator:
            if path.is_file():
                matches.append(self._format_display_path(path))
                if len(matches) >= max_results:
                    break

        return matches

    def _format_display_path(self, path: Path) -> str:
        """Return a human-friendly representation of a path."""
        try:
            return str(path.relative_to(self.working_dir))
        except ValueError:
            return str(path)

    # Maximum character count for search output
    MAX_SEARCH_OUTPUT_CHARS = 30_000

    def grep_files(
        self,
        pattern: str,
        path: Optional[str] = None,
        context_lines: int = 0,
        max_results: int = 50,
        case_insensitive: bool = False,
    ) -> list[dict[str, any]]:
        """Search for pattern in files.

        Args:
            pattern: Regex pattern to search for
            path: Optional path/directory to search in (relative to working_dir)
            context_lines: Number of context lines to include
            max_results: Maximum number of matches
            case_insensitive: Case insensitive search

        Returns:
            List of matches with file, line number, and content
        """
        matches = []

        try:
            # Use ripgrep if available for better performance
            # -F = fixed-strings (treat pattern as literal, not regex)
            cmd = ["rg", "--json", "-F", pattern]

            # Add default exclusions (ripgrep respects .gitignore, but this is a safety net)
            for exclude in DEFAULT_SEARCH_EXCLUDES:
                if exclude.startswith("*"):
                    cmd.extend(["--glob", f"!{exclude}"])
                else:
                    cmd.extend(["--glob", f"!{exclude}/**"])

            if case_insensitive:
                cmd.append("-i")
            if context_lines > 0:
                cmd.extend(["-C", str(context_lines)])

            # Add the search path if specified
            if path and path not in (".", "./"):
                search_path = self.working_dir / path
                cmd.append(str(search_path))
            # If path is "." or "./" or not specified, ripgrep uses cwd (which we set below)

            result = subprocess.run(
                cmd,
                cwd=self.working_dir,
                capture_output=True,
                text=True,
                timeout=10,
            )

            if result.returncode == 0:
                for line in result.stdout.strip().split("\n"):
                    if not line:
                        continue
                    try:
                        import json
                        data = json.loads(line)
                        if data["type"] == "match":
                            match_data = data["data"]
                            file_path = match_data["path"]["text"]
                            # Convert to absolute path
                            abs_path = str(self.working_dir / file_path)
                            matches.append({
                                "file": abs_path,
                                "line": match_data["line_number"],
                                "content": match_data["lines"]["text"].strip(),
                            })
                            if len(matches) >= max_results:
                                break
                    except:
                        continue

        except (subprocess.TimeoutExpired, FileNotFoundError):
            # Fallback to Python-based search if rg is not available
            matches = self._python_grep(pattern, path, max_results, case_insensitive)

        return matches

    def _python_grep(
        self, pattern: str, search_path: Optional[str],
        max_results: int, case_insensitive: bool
    ) -> list[dict[str, any]]:
        """Fallback grep implementation using Python."""
        matches = []
        flags = re.IGNORECASE if case_insensitive else 0
        # Escape pattern for literal matching (consistent with ripgrep -F)
        regex = re.compile(re.escape(pattern), flags)

        # Determine search root and glob pattern
        if search_path in (None, ".", "./"):
            # Search from working_dir with all files
            search_root = self.working_dir
            glob_pattern = "**/*"
        else:
            # Check if it's an absolute path
            search_path_obj = Path(search_path)
            if search_path_obj.is_absolute():
                if search_path_obj.is_dir():
                    search_root = search_path_obj
                    glob_pattern = "**/*"
                elif search_path_obj.is_file():
                    # Single file search
                    search_root = search_path_obj.parent
                    glob_pattern = search_path_obj.name
                else:
                    return matches  # Path doesn't exist
            else:
                # Relative path - resolve from working_dir
                resolved = self.working_dir / search_path
                if resolved.is_dir():
                    search_root = resolved
                    glob_pattern = "**/*"
                elif resolved.is_file():
                    search_root = resolved.parent
                    glob_pattern = resolved.name
                else:
                    # Treat as glob pattern
                    search_root = self.working_dir
                    glob_pattern = search_path

        for path in search_root.glob(glob_pattern):
            if not path.is_file():
                continue

            # Skip excluded paths
            if self._is_excluded_path(str(path)):
                continue

            try:
                with open(path, "r", encoding="utf-8", errors="ignore") as f:
                    for line_num, line in enumerate(f, 1):
                        if regex.search(line):
                            # Return absolute path for consistency with ripgrep output
                            matches.append({
                                "file": str(path),
                                "line": line_num,
                                "content": line.strip(),
                            })
                            if len(matches) >= max_results:
                                return matches
            except Exception:
                continue

        return matches

    def list_directory(self, path: str = ".", max_depth: int = 2) -> str:
        """List directory contents as a tree.

        Args:
            path: Directory path (relative or absolute)
            max_depth: Maximum depth to traverse

        Returns:
            Directory tree as string
        """
        dir_path = self._resolve_path(path)

        if not dir_path.exists():
            return f"Directory not found: {dir_path}"

        if not dir_path.is_dir():
            return f"Not a directory: {dir_path}"

        return self._build_tree(dir_path, max_depth=max_depth)

    def _build_tree(self, path: Path, prefix: str = "", max_depth: int = 2,
                    current_depth: int = 0) -> str:
        """Build a tree representation of directory structure."""
        if current_depth >= max_depth:
            return ""

        lines = []
        try:
            items = sorted(path.iterdir(), key=lambda p: (not p.is_dir(), p.name))
            # Filter out common ignore patterns
            items = [
                item for item in items
                if not any(
                    pattern in item.name
                    for pattern in [
                        "__pycache__",
                        ".git",
                        "node_modules",
                        ".pytest_cache",
                        "*.pyc",
                    ]
                )
            ]

            for i, item in enumerate(items):
                is_last = i == len(items) - 1
                current_prefix = "└── " if is_last else "├── "
                next_prefix = "    " if is_last else "│   "

                lines.append(f"{prefix}{current_prefix}{item.name}")

                if item.is_dir():
                    subtree = self._build_tree(
                        item,
                        prefix + next_prefix,
                        max_depth,
                        current_depth + 1,
                    )
                    if subtree:
                        lines.append(subtree)

        except PermissionError:
            lines.append(f"{prefix}[Permission Denied]")

        return "\n".join(lines)

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

    def ast_grep(
        self,
        pattern: str,
        path: Optional[str] = None,
        lang: Optional[str] = None,
        max_results: int = 50,
    ) -> list[dict[str, any]]:
        """Search for AST patterns using ast-grep.

        ast-grep matches code structure, not text. Use $VAR wildcards to match
        any AST node (similar to regex .* but for syntax trees).

        Args:
            pattern: AST pattern with $VAR wildcards (e.g., '$A && $A()')
            path: Directory to search (relative to working_dir)
            lang: Language hint (auto-detected from file extension if not specified)
            max_results: Maximum matches to return

        Returns:
            List of matches with file, line, and matched code

        Raises:
            FileNotFoundError: If ast-grep (sg) is not installed
        """
        import json

        cmd = ["sg", "--json", "-p", pattern]

        if lang:
            cmd.extend(["-l", lang])

        search_path = str(self.working_dir / path) if path else str(self.working_dir)
        cmd.append(search_path)

        result = subprocess.run(
            cmd,
            cwd=self.working_dir,
            capture_output=True,
            text=True,
            timeout=30,
        )

        matches = []
        if result.returncode == 0 and result.stdout.strip():
            try:
                # ast-grep outputs a JSON array, not newline-delimited objects
                data = json.loads(result.stdout)
                if isinstance(data, list):
                    for item in data:
                        file_path = item.get("file", "")

                        # Skip excluded paths (ast-grep doesn't respect .gitignore)
                        if self._is_excluded_path(file_path):
                            continue

                        # Make path relative to working_dir for cleaner output
                        try:
                            rel_path = str(Path(file_path).relative_to(self.working_dir))
                        except ValueError:
                            rel_path = file_path

                        matches.append({
                            "file": rel_path,
                            "line": item.get("range", {}).get("start", {}).get("line", 0),
                            "content": item.get("text", "").strip(),
                        })

                        if len(matches) >= max_results:
                            break
            except json.JSONDecodeError:
                pass  # Invalid JSON, return empty matches

        return matches
