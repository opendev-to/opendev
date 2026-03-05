"""File finding functionality for autocomplete."""

import os
from pathlib import Path
from typing import List


class FileFinder:
    """Handles file discovery for autocomplete."""

    def __init__(self, working_dir: Path):
        """Initialize file finder.

        Args:
            working_dir: Working directory for file searches
        """
        self.working_dir = working_dir

        # Common directories to exclude
        self.exclude_dirs = {
            ".git",
            ".hg",
            ".svn",
            "__pycache__",
            "node_modules",
            ".venv",
            "venv",
            ".pytest_cache",
            ".mypy_cache",
            ".tox",
            "dist",
            "build",
            ".eggs",
            "*.egg-info",
        }

    def find_files(self, query: str, max_results: int = 50) -> List[Path]:
        """Find files matching query.

        Args:
            query: Search query
            max_results: Maximum number of results

        Returns:
            List of matching file paths
        """
        matches = []
        query_lower = query.lower()

        try:
            # Walk through directory
            for root, dirs, files in os.walk(self.working_dir):
                # Filter out excluded directories
                dirs[:] = [d for d in dirs if d not in self.exclude_dirs]

                # Check if we have enough matches
                if len(matches) >= max_results:
                    break

                root_path = Path(root)

                # Check files
                for file in files:
                    file_path = root_path / file

                    # Get relative path for matching
                    try:
                        rel_path = file_path.relative_to(self.working_dir)
                        rel_path_str = str(rel_path).lower()
                    except ValueError:
                        continue

                    # Match against query
                    if not query_lower or query_lower in rel_path_str:
                        matches.append(file_path)

                        if len(matches) >= max_results:
                            break

        except (PermissionError, OSError):
            # Handle permission errors gracefully
            pass

        # Sort matches by relevance (shorter paths first, then alphabetically)
        matches.sort(key=lambda p: (len(str(p)), str(p)))

        return matches[:max_results]

    def format_file_size(self, size: int) -> str:
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

    def get_relative_path(self, file_path: Path) -> Path:
        """Get relative path from working directory.

        Args:
            file_path: File path to make relative

        Returns:
            Relative path
        """
        try:
            return file_path.relative_to(self.working_dir)
        except ValueError:
            return file_path