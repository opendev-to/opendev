"""Utility functions for autocomplete system."""

import os
import time
from pathlib import Path
from typing import TYPE_CHECKING, List, Optional

if TYPE_CHECKING:
    from .gitignore import GitIgnoreParser


class FileFinder:
    """Utility class for finding files in directory trees with caching."""

    # Cache duration in seconds
    CACHE_TTL = 30.0
    # Maximum files to cache
    MAX_CACHE_SIZE = 5000

    # Tier 1: Always exclude (obviously generated, never source code)
    ALWAYS_EXCLUDE_DIRS = {
        # Version Control
        ".git", ".hg", ".svn", ".bzr", "_darcs", ".fossil",

        # OS Generated
        ".DS_Store", ".Spotlight-V100", ".Trashes",
        "Thumbs.db", "desktop.ini", "$RECYCLE.BIN",

        # Python
        "__pycache__", ".pytest_cache", ".mypy_cache",
        ".pytype", ".pyre", ".hypothesis", ".tox", ".nox",
        "cython_debug", ".eggs",

        # Node/JS
        "node_modules", ".npm", ".yarn", ".pnpm-store",
        ".next", ".nuxt", ".output", ".svelte-kit", ".angular",
        ".parcel-cache", ".turbo",

        # IDE/Editor
        ".idea", ".vscode", ".vs", ".settings",

        # Java/Kotlin
        ".gradle",

        # Elixir
        "_build", "deps", ".elixir_ls",

        # iOS
        "Pods", "DerivedData", "xcuserdata",

        # Ruby
        ".bundle",

        # Virtual Environments
        ".venv", "venv",

        # Misc caches
        ".cache", ".sass-cache", ".eslintcache",
        ".tmp", ".temp", "tmp", "temp",
    }

    # Tier 2: Likely exclude (common build output dirs)
    # These MIGHT be source in some projects, but usually aren't
    LIKELY_EXCLUDE_DIRS = {
        "dist", "build", "out", "bin", "obj",
        "target",  # Rust/Maven
        "coverage", "htmlcov", "cover",  # Test coverage
        "logs",
        "vendor",  # Go/PHP/Ruby
        "packages",  # .NET
        "bower_components",  # Legacy JS
    }

    # Combined fallback when no .gitignore exists
    DEFAULT_EXCLUDE_DIRS = ALWAYS_EXCLUDE_DIRS | LIKELY_EXCLUDE_DIRS

    def __init__(self, working_dir: Path):
        """Initialize file finder.

        Args:
            working_dir: Working directory to search in
        """
        self.working_dir = working_dir
        # Cache: list of (relative_path_str_lower, Path) tuples
        self._cache: Optional[List[tuple[str, Path]]] = None
        self._cache_time: float = 0.0
        self._cache_working_dir: Optional[Path] = None
        # GitIgnore parser (lazy loaded)
        self._gitignore_parser: Optional["GitIgnoreParser"] = None
        self._gitignore_loaded: bool = False

    def _is_cache_valid(self) -> bool:
        """Check if the cache is still valid."""
        if self._cache is None:
            return False
        if self._cache_working_dir != self.working_dir:
            return False
        if time.time() - self._cache_time > self.CACHE_TTL:
            return False
        return True

    def _get_gitignore_parser(self) -> Optional["GitIgnoreParser"]:
        """Get or create the GitIgnore parser (lazy loading).

        Returns:
            GitIgnoreParser instance or None if .gitignore doesn't exist
        """
        if not self._gitignore_loaded:
            self._gitignore_loaded = True
            # Check if .gitignore exists before loading the parser
            gitignore_path = self.working_dir / ".gitignore"
            if gitignore_path.exists():
                from .gitignore import GitIgnoreParser
                self._gitignore_parser = GitIgnoreParser(self.working_dir)
        return self._gitignore_parser

    def _should_skip_dir(self, dir_path: Path, dir_name: str) -> bool:
        """Check if a directory should be skipped during traversal.

        Args:
            dir_path: Full path to the directory
            dir_name: Name of the directory

        Returns:
            True if the directory should be skipped
        """
        parser = self._get_gitignore_parser()
        if parser:
            return parser.should_skip_dir(dir_path)
        # Fallback to default excludes if no .gitignore
        return dir_name in self.DEFAULT_EXCLUDE_DIRS

    def _should_skip_file(self, file_path: Path) -> bool:
        """Check if a file should be skipped.

        Args:
            file_path: Full path to the file

        Returns:
            True if the file should be skipped
        """
        parser = self._get_gitignore_parser()
        if parser:
            return parser.is_ignored(file_path)
        return False

    def _build_cache(self) -> None:
        """Build the file cache by walking the directory tree once."""
        cache: List[tuple[str, Path]] = []

        # Reset gitignore parser when rebuilding cache (it may have changed)
        self._gitignore_loaded = False
        self._gitignore_parser = None

        try:
            for root, dirs, files in os.walk(self.working_dir):
                root_path = Path(root)

                # Filter out excluded directories using gitignore or fallback
                dirs[:] = [
                    d for d in dirs
                    if not self._should_skip_dir(root_path / d, d)
                ]

                # Add directories
                for dirname in dirs:
                    dir_path = root_path / dirname
                    try:
                        rel_path = dir_path.relative_to(self.working_dir)
                        cache.append((str(rel_path).lower(), dir_path))
                    except ValueError:
                        continue

                # Add files (filter using gitignore)
                for filename in files:
                    file_path = root_path / filename

                    # Skip ignored files
                    if self._should_skip_file(file_path):
                        continue

                    try:
                        rel_path = file_path.relative_to(self.working_dir)
                        cache.append((str(rel_path).lower(), file_path))
                    except ValueError:
                        continue

                    # Limit cache size
                    if len(cache) >= self.MAX_CACHE_SIZE:
                        break

                if len(cache) >= self.MAX_CACHE_SIZE:
                    break

        except (PermissionError, OSError):
            pass

        # Sort by path length then alphabetically for consistent ordering
        cache.sort(key=lambda x: (len(x[0]), x[0]))

        self._cache = cache
        self._cache_time = time.time()
        self._cache_working_dir = self.working_dir

    def find_files(self, query: str, max_results: int = 50, include_dirs: bool = False) -> List[Path]:
        """Find files matching query using cached file list.

        Args:
            query: Search query
            max_results: Maximum number of results
            include_dirs: Whether to include directories in results

        Returns:
            List of matching file paths
        """
        # Rebuild cache if needed
        if not self._is_cache_valid():
            self._build_cache()

        if self._cache is None:
            return []

        query_lower = query.lower()
        matches: List[Path] = []

        for rel_path_lower, file_path in self._cache:
            # Filter directories if not requested
            if not include_dirs and file_path.is_dir():
                continue

            # Match query
            if not query_lower or query_lower in rel_path_lower:
                matches.append(file_path)
                if len(matches) >= max_results:
                    break

        return matches

    def invalidate_cache(self) -> None:
        """Invalidate the cache to force a refresh on next query."""
        self._cache = None


class FileSizeFormatter:
    """Utility class for formatting file sizes."""

    @staticmethod
    def format_size(size: int) -> str:
        """Format file size in human-readable format.

        Args:
            size: Size in bytes

        Returns:
            Formatted size string
        """
        if size < 1024:
            return f"{size} B"
        elif size < 1024 * 1024:
            return f"{size / 1024:.1f} KB"
        else:
            return f"{size / (1024 * 1024):.1f} MB"

    @staticmethod
    def get_file_size(file_path: Path) -> str:
        """Get formatted file size for a file.

        Args:
            file_path: Path to file

        Returns:
            Formatted size string or empty string if unavailable
        """
        try:
            size = file_path.stat().st_size
            return FileSizeFormatter.format_size(size)
        except (OSError, FileNotFoundError):
            return ""
