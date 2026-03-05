"""Web screenshot tool using Crawl4AI for high-quality full-page captures."""

import asyncio
import logging
import subprocess
import sys
import tempfile
from base64 import b64decode
from pathlib import Path
from typing import Optional, Dict, Any

from swecli.models.config import AppConfig

logger = logging.getLogger(__name__)

_browsers_installed = False


def _ensure_browsers_installed() -> None:
    """Install Playwright Chromium if not already installed."""
    global _browsers_installed
    if _browsers_installed:
        return
    logger.info("Installing Playwright Chromium browser...")
    try:
        subprocess.run(
            ["playwright", "install", "chromium"],
            capture_output=True,
            text=True,
            timeout=120,
            check=True,
        )
    except FileNotFoundError:
        # playwright CLI not on PATH — try python -m playwright
        subprocess.run(
            [sys.executable, "-m", "playwright", "install", "chromium"],
            capture_output=True,
            text=True,
            timeout=120,
            check=True,
        )
    _browsers_installed = True
    logger.info("Playwright Chromium installed successfully.")


class WebScreenshotTool:
    """Tool for capturing full-page screenshots and PDFs of web pages using Crawl4AI."""

    def __init__(self, config: AppConfig, working_dir: Path):
        """Initialize web screenshot tool.

        Args:
            config: Application configuration
            working_dir: Working directory for resolving relative paths
        """
        self.config = config
        self.working_dir = working_dir
        self.screenshot_dir = Path(tempfile.gettempdir()) / "swecli_web_screenshots"
        self.screenshot_dir.mkdir(parents=True, exist_ok=True)

    def is_crawl4ai_available(self) -> bool:
        """Check if Crawl4AI is installed.

        Returns:
            True if Crawl4AI is available, False otherwise
        """
        try:
            from crawl4ai import AsyncWebCrawler
            return True
        except ImportError:
            return False

    def _normalize_url(self, url: str) -> str:
        """Normalize URL to ensure it has proper format.

        Args:
            url: URL to normalize

        Returns:
            Normalized URL with proper protocol and slashes
        """
        url = url.strip()

        # Fix common malformations: https:/domain.com → https://https://domain.com
        if url.startswith("https:/") and not url.startswith("https://"):
            url = url.replace("https:/", "https://", 1)
        elif url.startswith("http:/") and not url.startswith("http://"):
            url = url.replace("http:/", "http://", 1)

        # Add https:// if no protocol specified
        if not url.startswith(("http://", "https://")):
            url = f"https://{url}"

        return url

    def capture_web_screenshot(
        self,
        url: str,
        output_path: Optional[str] = None,
        capture_pdf: bool = False,
        timeout_ms: int = 90000,
        viewport_width: int = 1920,
        viewport_height: int = 1080,
    ) -> Dict[str, Any]:
        """Capture a full-page screenshot (and optionally PDF) of a web page using Crawl4AI.

        Args:
            url: URL of the web page to capture
            output_path: Path to save screenshot (relative to working_dir or absolute).
                        If None, saves to temp directory with auto-generated name.
            capture_pdf: If True, also capture a PDF version of the page
            timeout_ms: Maximum time to wait for page load (milliseconds)
            viewport_width: Browser viewport width in pixels
            viewport_height: Browser viewport height in pixels

        Returns:
            Dictionary with success, screenshot_path, pdf_path (if requested), and optional error
        """
        # Normalize URL to fix common malformations
        url = self._normalize_url(url)

        # Check if Crawl4AI is available
        if not self.is_crawl4ai_available():
            return {
                "success": False,
                "error": (
                    "Crawl4AI is not installed. Install it with:\n"
                    "  pip install crawl4ai\n"
                    "  crawl4ai-setup"
                ),
                "screenshot_path": None,
                "pdf_path": None,
            }

        try:
            from crawl4ai import AsyncWebCrawler, BrowserConfig, CrawlerRunConfig, CacheMode

            # Determine output paths
            if output_path:
                screenshot_path = Path(output_path)
                if not screenshot_path.is_absolute():
                    screenshot_path = self.working_dir / screenshot_path
            else:
                # Auto-generate filename from URL
                from urllib.parse import urlparse
                parsed = urlparse(url)
                domain = parsed.netloc.replace(":", "_").replace("/", "_")
                timestamp = Path(tempfile.mktemp()).name  # Get unique ID
                filename = f"{domain}_{timestamp}.png"
                screenshot_path = self.screenshot_dir / filename

            # Ensure parent directory exists
            screenshot_path.parent.mkdir(parents=True, exist_ok=True)

            # Generate PDF path if requested
            pdf_path = None
            if capture_pdf:
                pdf_path = screenshot_path.with_suffix('.pdf')

            # Run async crawler in sync context
            result = asyncio.run(self._async_capture(
                url=url,
                screenshot_path=screenshot_path,
                pdf_path=pdf_path,
                timeout_ms=timeout_ms,
                viewport_width=viewport_width,
                viewport_height=viewport_height,
            ))

            return result

        except ImportError:
            return {
                "success": False,
                "error": (
                    "Crawl4AI is not installed. Install it with:\n"
                    "  pip install crawl4ai\n"
                    "  crawl4ai-setup"
                ),
                "url": url,  # Include normalized URL even on error
                "screenshot_path": None,
                "pdf_path": None,
            }
        except KeyboardInterrupt:
            return {
                "success": False,
                "error": "Screenshot capture cancelled by user",
                "url": url,
                "screenshot_path": None,
                "pdf_path": None,
            }
        except Exception as e:
            error_msg = str(e)
            # Suppress noisy Playwright errors during shutdown
            if "Target page, context or browser has been closed" in error_msg:
                return {
                    "success": False,
                    "error": "Screenshot capture cancelled (browser closed)",
                    "url": url,
                    "screenshot_path": None,
                    "pdf_path": None,
                }
            # Auto-install Playwright browsers on first use
            if "Executable doesn't exist" in error_msg:
                try:
                    _ensure_browsers_installed()
                    result = asyncio.run(self._async_capture(
                        url=url,
                        screenshot_path=screenshot_path,
                        pdf_path=pdf_path,
                        timeout_ms=timeout_ms,
                        viewport_width=viewport_width,
                        viewport_height=viewport_height,
                    ))
                    return result
                except Exception as retry_err:
                    return {
                        "success": False,
                        "error": f"Browser auto-install failed: {retry_err}",
                        "url": url,
                        "screenshot_path": None,
                        "pdf_path": None,
                    }
            return {
                "success": False,
                "error": f"Failed to capture screenshot: {error_msg}",
                "url": url,  # Include normalized URL even on error
                "screenshot_path": None,
                "pdf_path": None,
            }

    async def _async_capture(
        self,
        url: str,
        screenshot_path: Path,
        pdf_path: Optional[Path],
        timeout_ms: int,
        viewport_width: int,
        viewport_height: int,
    ) -> Dict[str, Any]:
        """Async helper to capture screenshot and PDF using Crawl4AI.

        Args:
            url: URL to capture
            screenshot_path: Path to save screenshot
            pdf_path: Optional path to save PDF
            timeout_ms: Timeout in milliseconds
            viewport_width: Viewport width
            viewport_height: Viewport height

        Returns:
            Dictionary with success status and paths
        """
        from crawl4ai import AsyncWebCrawler, BrowserConfig, CrawlerRunConfig, CacheMode

        # Configure browser
        browser_config = BrowserConfig(
            headless=True,
            viewport_width=viewport_width,
            viewport_height=viewport_height,
        )

        # Configure crawler run
        run_config = CrawlerRunConfig(
            cache_mode=CacheMode.BYPASS,
            screenshot=True,
            pdf=pdf_path is not None,
            page_timeout=timeout_ms,
            wait_until="networkidle",
        )

        try:
            async with AsyncWebCrawler(config=browser_config) as crawler:
                result = await crawler.arun(url=url, config=run_config)

                if not result.success:
                    return {
                        "success": False,
                        "error": result.error_message or "Failed to capture page",
                        "url": url,  # Include normalized URL even on error
                        "screenshot_path": None,
                        "pdf_path": None,
                    }

                response_data = {
                    "success": True,
                    "url": url,
                    "viewport": f"{viewport_width}x{viewport_height}",
                    "error": None,
                }

                # Save screenshot if available
                if result.screenshot:
                    screenshot_bytes = b64decode(result.screenshot)
                    with open(screenshot_path, "wb") as f:
                        f.write(screenshot_bytes)
                    response_data["screenshot_path"] = str(screenshot_path)
                    response_data["screenshot_size_kb"] = round(len(screenshot_bytes) / 1024, 1)
                else:
                    response_data["screenshot_path"] = None
                    response_data["warning"] = "Screenshot data not available"

                # Save PDF if requested and available
                if pdf_path and result.pdf:
                    with open(pdf_path, "wb") as f:
                        f.write(result.pdf)
                    response_data["pdf_path"] = str(pdf_path)
                    response_data["pdf_size_kb"] = round(len(result.pdf) / 1024, 1)
                elif pdf_path:
                    response_data["pdf_path"] = None
                    response_data["pdf_warning"] = "PDF data not available"
                else:
                    response_data["pdf_path"] = None

                return response_data

        except asyncio.CancelledError:
            return {
                "success": False,
                "error": "Screenshot capture cancelled by user",
                "url": url,
                "screenshot_path": None,
                "pdf_path": None,
            }
        except Exception as e:
            error_msg = str(e)
            # Suppress noisy Playwright errors during shutdown
            if "Target page, context or browser has been closed" in error_msg:
                return {
                    "success": False,
                    "error": "Screenshot capture cancelled (browser closed)",
                    "url": url,
                    "screenshot_path": None,
                    "pdf_path": None,
                }
            return {
                "success": False,
                "error": f"Crawl4AI error: {error_msg}",
                "url": url,  # Include normalized URL even on error
                "screenshot_path": None,
                "pdf_path": None,
            }

    def list_web_screenshots(self) -> Dict[str, Any]:
        """List all captured web screenshots in the temp directory.

        Returns:
            Dictionary with success, screenshots list, and optional error
        """
        try:
            screenshots = []
            if self.screenshot_dir.exists():
                for screenshot_file in sorted(
                    self.screenshot_dir.glob("*.png"),
                    key=lambda p: p.stat().st_mtime,
                    reverse=True,
                )[:10]:  # Show 10 most recent
                    stat = screenshot_file.stat()
                    screenshots.append({
                        "path": str(screenshot_file),
                        "name": screenshot_file.name,
                        "size_kb": round(stat.st_size / 1024, 1),
                        "modified": stat.st_mtime,
                    })

            return {
                "success": True,
                "screenshots": screenshots,
                "count": len(screenshots),
                "directory": str(self.screenshot_dir),
                "error": None,
            }
        except Exception as e:
            return {
                "success": False,
                "error": f"Failed to list screenshots: {str(e)}",
                "screenshots": [],
            }

    def clear_web_screenshots(self, keep_recent: int = 5) -> Dict[str, Any]:
        """Clear old web screenshots from temp directory.

        Args:
            keep_recent: Number of most recent screenshots to keep

        Returns:
            Dictionary with success, deleted count, and optional error
        """
        try:
            if not self.screenshot_dir.exists():
                return {
                    "success": True,
                    "deleted_count": 0,
                    "kept_count": 0,
                    "error": None,
                }

            # Get all screenshots sorted by modification time
            screenshots = sorted(
                self.screenshot_dir.glob("*.png"),
                key=lambda p: p.stat().st_mtime,
                reverse=True,
            )

            # Keep recent, delete old
            kept = screenshots[:keep_recent]
            to_delete = screenshots[keep_recent:]

            deleted_count = 0
            for screenshot_file in to_delete:
                try:
                    screenshot_file.unlink()
                    deleted_count += 1
                except Exception:
                    pass  # Continue deleting others

            return {
                "success": True,
                "deleted_count": deleted_count,
                "kept_count": len(kept),
                "error": None,
            }
        except Exception as e:
            return {
                "success": False,
                "error": f"Failed to clear screenshots: {str(e)}",
                "deleted_count": 0,
            }
