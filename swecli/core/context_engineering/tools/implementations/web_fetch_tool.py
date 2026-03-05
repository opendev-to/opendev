"""Web fetching tool for retrieving content from URLs using Crawl4AI."""

from __future__ import annotations

import asyncio
from pathlib import Path
from typing import Any, Optional, Sequence, TYPE_CHECKING

# Lazy import crawl4ai to avoid Pydantic deprecation warnings at startup
# These imports are done inside methods that use them
if TYPE_CHECKING:
    from crawl4ai import AsyncWebCrawler, BrowserConfig, CacheMode, CrawlerRunConfig

from swecli.core.context_engineering.tools.implementations.web_screenshot_tool import (
    _ensure_browsers_installed,
)
from swecli.models.config import AppConfig


class WebFetchTool:
    """Tool for fetching web content using Crawl4AI."""

    def __init__(self, config: AppConfig, working_dir: Path):
        """Initialize web fetch tool.

        Args:
            config: Application configuration
            working_dir: Working directory (not used but kept for consistency)
        """
        self.config = config
        self.working_dir = working_dir
        self.timeout = 30000  # 30 second timeout for page load

    def fetch_url(
        self,
        url: str,
        extract_text: bool = True,
        max_length: Optional[int] = 50000,
        deep_crawl: bool = False,
        crawl_strategy: str = "best_first",
        max_depth: int = 1,
        include_external: bool = False,
        max_pages: Optional[int] = None,
        allowed_domains: Optional[Sequence[str]] = None,
        blocked_domains: Optional[Sequence[str]] = None,
        url_patterns: Optional[Sequence[str]] = None,
        stream: bool = False,
    ) -> dict[str, any]:
        """Fetch content from a URL using Crawl4AI.

        Args:
            url: URL to fetch
            extract_text: If True, return markdown format. If False, return raw HTML
            max_length: Maximum content length (None for no limit)
            deep_crawl: If True, follow links using a deep crawl strategy
            crawl_strategy: One of bfs, dfs, best_first (best_first default)
            max_depth: Maximum depth to traverse when deep crawling
            include_external: Allow following external domains
            max_pages: Optional cap on total crawled pages
            allowed_domains: Optional list of domains to keep during deep crawl
            blocked_domains: Optional list of domains to skip during deep crawl
            url_patterns: Optional glob patterns to include
            stream: If True (and deep_crawl), stream results as they are discovered

        Returns:
            Dictionary with success, content, and optional error
        """
        # Run async fetch in sync context
        try:
            loop = asyncio.get_event_loop()
        except RuntimeError:
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)

        fetch_args = dict(
            url=url,
            extract_text=extract_text,
            max_length=max_length,
            deep_crawl=deep_crawl,
            crawl_strategy=crawl_strategy,
            max_depth=max_depth,
            include_external=include_external,
            max_pages=max_pages,
            allowed_domains=allowed_domains,
            blocked_domains=blocked_domains,
            url_patterns=url_patterns,
            stream=stream,
        )

        result = loop.run_until_complete(self._fetch_url_async(**fetch_args))

        # Auto-install Playwright browsers on first use
        if (
            not result.get("success")
            and result.get("error")
            and "Executable doesn't exist" in result["error"]
        ):
            try:
                _ensure_browsers_installed()
                result = loop.run_until_complete(self._fetch_url_async(**fetch_args))
            except Exception as retry_err:
                return {
                    "success": False,
                    "error": f"Browser auto-install failed: {retry_err}",
                    "content": None,
                }

        return result

    async def _fetch_url_async(
        self,
        url: str,
        extract_text: bool = True,
        max_length: Optional[int] = 50000,
        deep_crawl: bool = False,
        crawl_strategy: str = "best_first",
        max_depth: int = 1,
        include_external: bool = False,
        max_pages: Optional[int] = None,
        allowed_domains: Optional[Sequence[str]] = None,
        blocked_domains: Optional[Sequence[str]] = None,
        url_patterns: Optional[Sequence[str]] = None,
        stream: bool = False,
    ) -> dict[str, any]:
        """Async implementation of URL fetching.

        Args:
            url: URL to fetch
            extract_text: If True, return markdown format. If False, return raw HTML
            max_length: Maximum content length (None for no limit)
            deep_crawl: If True, follow links using a deep crawl strategy
            crawl_strategy: One of bfs, dfs, best_first (best_first default)
            max_depth: Maximum depth to traverse when deep crawling
            include_external: Allow following external domains
            max_pages: Optional cap on total crawled pages
            allowed_domains: Optional list of domains to keep during deep crawl
            blocked_domains: Optional list of domains to skip during deep crawl
            url_patterns: Optional glob patterns to include
            stream: If True (and deep_crawl), stream results as they are discovered

        Returns:
            Dictionary with success, content, and optional error
        """
        try:
            # Lazy import crawl4ai to avoid Pydantic deprecation warnings at startup
            from crawl4ai import AsyncWebCrawler, BrowserConfig, CacheMode, CrawlerRunConfig
            from crawl4ai.deep_crawling import BFSDeepCrawlStrategy, DFSDeepCrawlStrategy, BestFirstCrawlingStrategy
            from crawl4ai.deep_crawling.filters import FilterChain, DomainFilter, URLPatternFilter

            # Validate URL format
            if not url.startswith(("http://", "https://")):
                return {
                    "success": False,
                    "error": f"Invalid URL: must start with http:// or https://",
                    "content": None,
                }

            # Skip file downloads that cause navigation errors
            file_extensions = ('.zip', '.pdf', '.exe', '.dmg', '.tar.gz', '.rar', '.mp4', '.avi', '.mov', '.iso', '.pkg')
            if any(url.lower().endswith(ext) for ext in file_extensions):
                return {
                    "success": False,
                    "error": f"File downloads are not supported: {url}",
                    "content": None,
                }

            if deep_crawl and max_depth < 1:
                return {
                    "success": False,
                    "error": "max_depth must be greater than 0 when deep_crawl is enabled",
                    "content": None,
                }

            if deep_crawl and max_pages is not None and max_pages < 1:
                return {
                    "success": False,
                    "error": "max_pages must be greater than 0 when specified",
                    "content": None,
                }

            filter_chain = None
            deep_strategy = None
            stream_mode = stream if deep_crawl else False

            if deep_crawl:
                filter_chain = self._build_filter_chain(
                    allowed_domains, blocked_domains, url_patterns,
                    FilterChain=FilterChain, DomainFilter=DomainFilter, URLPatternFilter=URLPatternFilter,
                )
                deep_strategy = self._build_deep_strategy(
                    strategy=crawl_strategy,
                    max_depth=max_depth,
                    include_external=include_external,
                    max_pages=max_pages,
                    filter_chain=filter_chain,
                    BFSDeepCrawlStrategy=BFSDeepCrawlStrategy,
                    DFSDeepCrawlStrategy=DFSDeepCrawlStrategy,
                    BestFirstCrawlingStrategy=BestFirstCrawlingStrategy,
                )
            else:
                filter_chain = None

            # Configure browser
            browser_config = BrowserConfig(
                headless=True,
                verbose=False,
                user_agent="OpenDev/1.0 (AI Assistant Tool; Crawl4AI)",
            )

            # Configure crawler
            run_config = CrawlerRunConfig(
                cache_mode=CacheMode.BYPASS,
                word_count_threshold=10,
                exclude_external_links=False,
                remove_overlay_elements=True,
                process_iframes=False,
                page_timeout=self.timeout,
                deep_crawl_strategy=deep_strategy,
                stream=stream_mode,
            )

            # Fetch URL
            async with AsyncWebCrawler(config=browser_config) as crawler:
                if stream_mode:
                    page_results = []
                    async for streamed_result in crawler.arun(url=url, config=run_config):
                        page_results.append(streamed_result)
                else:
                    result = await crawler.arun(url=url, config=run_config)
                    page_results = result if isinstance(result, list) else [result]

                if deep_crawl:
                    successful_pages = [
                        page for page in page_results if getattr(page, "success", True)
                    ]
                    if not successful_pages:
                        return {
                            "success": False,
                            "error": "Deep crawl completed but returned no successful pages",
                            "content": None,
                        }

                    content, page_metadata = self._format_deep_crawl_results(successful_pages, extract_text)
                    if max_length and len(content) > max_length:
                        content = (
                            content[:max_length]
                            + f"\n\n... (truncated, total length: {len(content)} characters)"
                        )

                    return {
                        "success": True,
                        "content": content,
                        "error": None,
                        "url": url,
                        "status_code": 200,
                        "content_type": "text/markdown" if extract_text else "text/html",
                        "pages": page_metadata,
                        "page_count": len(successful_pages),
                    }

                result = page_results[0]

                if not result.success:
                    error_msg = (
                        result.error_message if hasattr(result, "error_message") else "Unknown error"
                    )
                    return {
                        "success": False,
                        "error": error_msg,
                        "content": None,
                    }

                content = result.markdown if extract_text else result.html

                if max_length and len(content) > max_length:
                    content = (
                        content[:max_length]
                        + f"\n\n... (truncated, total length: {len(content)} characters)"
                    )

                return {
                    "success": True,
                    "content": content,
                    "error": None,
                    "url": result.url,  # Final URL after redirects
                    "status_code": 200,  # Crawl4AI doesn't expose status codes directly
                    "content_type": "text/markdown" if extract_text else "text/html",
                    "links": result.links if hasattr(result, "links") else {},
                    "media": result.media if hasattr(result, "media") else {},
                }

        except asyncio.TimeoutError:
            return {
                "success": False,
                "error": f"Request timeout after {self.timeout / 1000} seconds",
                "content": None,
            }
        except asyncio.CancelledError:
            # User interrupted (Ctrl+C) - return gracefully
            return {
                "success": False,
                "error": "Request cancelled by user",
                "content": None,
            }
        except KeyboardInterrupt:
            # Direct keyboard interrupt
            return {
                "success": False,
                "error": "Request cancelled by user",
                "content": None,
            }
        except Exception as e:
            error_msg = str(e)
            # Suppress noisy Playwright errors during shutdown
            if "Target page, context or browser has been closed" in error_msg:
                return {
                    "success": False,
                    "error": "Request cancelled (browser closed)",
                    "content": None,
                }
            return {
                "success": False,
                "error": f"Failed to fetch URL: {error_msg}",
                "content": None,
            }

    def _build_filter_chain(
        self,
        allowed_domains: Optional[Sequence[str]],
        blocked_domains: Optional[Sequence[str]],
        url_patterns: Optional[Sequence[str]],
        *,
        FilterChain: type,
        DomainFilter: type,
        URLPatternFilter: type,
    ) -> Any:
        filters = []

        if allowed_domains or blocked_domains:
            filters.append(
                DomainFilter(
                    allowed_domains=list(allowed_domains) if allowed_domains else None,
                    blocked_domains=list(blocked_domains) if blocked_domains else None,
                )
            )

        if url_patterns:
            filters.append(URLPatternFilter(patterns=list(url_patterns)))

        return FilterChain(filters) if filters else None

    def _build_deep_strategy(
        self,
        *,
        strategy: str,
        max_depth: int,
        include_external: bool,
        max_pages: Optional[int],
        filter_chain: Any,
        BFSDeepCrawlStrategy: type,
        DFSDeepCrawlStrategy: type,
        BestFirstCrawlingStrategy: type,
    ):
        strategy_key = (strategy or "best_first").lower()
        strategy_map = {
            "bfs": BFSDeepCrawlStrategy,
            "dfs": DFSDeepCrawlStrategy,
            "best_first": BestFirstCrawlingStrategy,
            "best": BestFirstCrawlingStrategy,
        }

        if strategy_key not in strategy_map:
            raise ValueError(f"Unsupported crawl_strategy '{strategy}'. Use bfs, dfs, or best_first.")

        strategy_cls = strategy_map[strategy_key]
        kwargs: dict[str, Any] = {
            "max_depth": max_depth,
            "include_external": include_external,
        }

        if filter_chain:
            kwargs["filter_chain"] = filter_chain

        if max_pages is not None:
            kwargs["max_pages"] = max_pages

        return strategy_cls(**kwargs)

    def _format_deep_crawl_results(self, pages: Sequence[Any], extract_text: bool) -> tuple[str, list[dict[str, Any]]]:
        sections: list[str] = []
        metadata: list[dict[str, Any]] = []

        for index, page in enumerate(pages, start=1):
            page_url = getattr(page, "url", None)
            raw_metadata = getattr(page, "metadata", {}) or {}
            depth = raw_metadata.get("depth", 0) if isinstance(raw_metadata, dict) else raw_metadata

            page_content = page.markdown if extract_text else page.html
            page_content = page_content or ""

            sections.append(
                f"### Page {index}: {page_url or 'unknown'} (depth {depth})\n\n{page_content}".strip()
            )

            metadata.append(
                {
                    "url": page_url,
                    "depth": depth,
                    "content_length": len(page_content),
                }
            )

        combined_content = "\n\n".join(sections)
        return combined_content, metadata
