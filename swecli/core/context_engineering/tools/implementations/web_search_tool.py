"""Web search tool using DuckDuckGo for privacy-respecting searches."""

from __future__ import annotations

import logging
from pathlib import Path
from typing import Any, Sequence

from swecli.models.config import AppConfig

logger = logging.getLogger(__name__)


class WebSearchTool:
    """Tool for searching the web using DuckDuckGo.

    Uses the duckduckgo-search library for privacy-respecting web searches.
    This tool is read-only and safe for use in plan mode.
    """

    def __init__(self, config: AppConfig, working_dir: Path):
        """Initialize web search tool.

        Args:
            config: Application configuration
            working_dir: Working directory (not used but kept for consistency)
        """
        self.config = config
        self.working_dir = working_dir
        self.default_max_results = 10

    def search(
        self,
        query: str,
        max_results: int = 10,
        allowed_domains: Sequence[str] | None = None,
        blocked_domains: Sequence[str] | None = None,
    ) -> dict[str, Any]:
        """Search the web and return results with titles, URLs, and snippets.

        Args:
            query: Search query string
            max_results: Maximum number of results to return (default: 10)
            allowed_domains: Only include results from these domains
            blocked_domains: Exclude results from these domains

        Returns:
            Dictionary with:
            - success: bool
            - results: list of {title, url, snippet}
            - query: str
            - error: str | None
        """
        if not query or not query.strip():
            return {
                "success": False,
                "error": "Search query is required",
                "results": [],
                "query": query,
            }

        try:
            # Lazy import to avoid loading at startup
            from duckduckgo_search import DDGS

            # Perform the search
            with DDGS() as ddgs:
                # Get text search results
                raw_results = list(ddgs.text(
                    query,
                    max_results=max_results * 2,  # Fetch extra for filtering
                ))

            # Filter results by domain if specified
            filtered_results = self._filter_by_domain(
                raw_results,
                allowed_domains,
                blocked_domains,
            )

            # Limit to max_results after filtering
            filtered_results = filtered_results[:max_results]

            # Format results
            formatted_results = []
            for result in filtered_results:
                formatted_results.append({
                    "title": result.get("title", ""),
                    "url": result.get("href", result.get("link", "")),
                    "snippet": result.get("body", result.get("snippet", "")),
                })

            return {
                "success": True,
                "results": formatted_results,
                "query": query,
                "result_count": len(formatted_results),
                "error": None,
            }

        except ImportError:
            return {
                "success": False,
                "error": "duckduckgo-search package not installed. Run: pip install duckduckgo-search",
                "results": [],
                "query": query,
            }
        except Exception as e:
            logger.exception(f"Web search failed for query: {query}")
            return {
                "success": False,
                "error": f"Search failed: {str(e)}",
                "results": [],
                "query": query,
            }

    def _filter_by_domain(
        self,
        results: list[dict[str, Any]],
        allowed_domains: Sequence[str] | None,
        blocked_domains: Sequence[str] | None,
    ) -> list[dict[str, Any]]:
        """Filter search results by domain.

        Args:
            results: Raw search results
            allowed_domains: Only include these domains (if specified)
            blocked_domains: Exclude these domains

        Returns:
            Filtered list of results
        """
        if not allowed_domains and not blocked_domains:
            return results

        filtered = []
        for result in results:
            url = result.get("href", result.get("link", ""))
            if not url:
                continue

            # Extract domain from URL
            try:
                from urllib.parse import urlparse
                domain = urlparse(url).netloc.lower()
                # Remove www. prefix for matching
                if domain.startswith("www."):
                    domain = domain[4:]
            except Exception:
                continue

            # Check allowed domains
            if allowed_domains:
                allowed = False
                for allowed_domain in allowed_domains:
                    allowed_clean = allowed_domain.lower().lstrip("www.")
                    if domain == allowed_clean or domain.endswith("." + allowed_clean):
                        allowed = True
                        break
                if not allowed:
                    continue

            # Check blocked domains
            if blocked_domains:
                blocked = False
                for blocked_domain in blocked_domains:
                    blocked_clean = blocked_domain.lower().lstrip("www.")
                    if domain == blocked_clean or domain.endswith("." + blocked_clean):
                        blocked = True
                        break
                if blocked:
                    continue

            filtered.append(result)

        return filtered
