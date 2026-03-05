"""Tool for opening URLs in the default web browser."""

from __future__ import annotations

import webbrowser
from pathlib import Path
from typing import Any, Dict

from swecli.models.config import AppConfig


class OpenBrowserTool:
    """Tool for opening URLs in the default web browser.

    This tool allows the agent to automatically open web pages, which is useful
    for web development workflows where the agent creates a web app and wants
    to show it to the user.
    """

    def __init__(self, config: AppConfig, working_dir: Path):
        """Initialize the open browser tool.

        Args:
            config: Configuration object
            working_dir: Working directory path
        """
        self.config = config
        self.working_dir = working_dir

    def execute(self, url: str, **kwargs) -> Dict[str, Any]:
        """Open a URL or local file in the default web browser.

        Args:
            url: The URL or file path to open
            **kwargs: Additional arguments (ignored)

        Returns:
            Result dictionary with success status and message
        """
        try:
            # Handle local file paths
            if not (url.startswith("http://") or url.startswith("https://") or url.startswith("file://")):
                # Check if it's a local file path
                file_path = Path(url)

                # If relative path, resolve it against working directory
                if not file_path.is_absolute():
                    file_path = self.working_dir / file_path

                # Check if file exists
                if file_path.exists() and file_path.is_file():
                    # Convert to file:// URL
                    url = file_path.as_uri()
                else:
                    # Handle localhost URLs
                    if url.startswith("localhost:"):
                        url = f"http://{url}"
                    elif url.startswith(":"):
                        url = f"http://localhost{url}"
                    else:
                        # Not a valid file path or recognizable URL format
                        return {
                            "success": False,
                            "error": f"Invalid URL format: {url}. Must be a valid URL (http://, https://, file://) or existing file path",
                        }

            # Open in browser
            webbrowser.open(url)

            return {
                "success": True,
                "message": f"Opened {url} in your default browser",
                "url": url,
            }

        except Exception as e:
            return {
                "success": False,
                "error": f"Failed to open browser: {str(e)}",
            }
