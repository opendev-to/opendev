"""Handler for web search tool invocations."""

from __future__ import annotations

from typing import Any


class WebSearchHandler:
    """Executes web search operations and formats results."""

    def __init__(self, web_search_tool: Any) -> None:
        """Initialize the handler.

        Args:
            web_search_tool: WebSearchTool instance for performing searches
        """
        self._web_search_tool = web_search_tool

    def search(self, args: dict[str, Any]) -> dict[str, Any]:
        """Handle web search requests.

        Args:
            args: Dictionary with:
                - query: Search query string (required)
                - max_results: Maximum results to return (default: 10)
                - allowed_domains: Only include these domains
                - blocked_domains: Exclude these domains

        Returns:
            Result dictionary with formatted output
        """
        if not self._web_search_tool:
            return {
                "success": False,
                "error": "WebSearchTool not available",
                "output": None,
            }

        query = args.get("query", "")
        max_results = args.get("max_results", 10)
        allowed_domains = args.get("allowed_domains")
        blocked_domains = args.get("blocked_domains")

        # Perform the search
        result = self._web_search_tool.search(
            query=query,
            max_results=max_results,
            allowed_domains=allowed_domains,
            blocked_domains=blocked_domains,
        )

        if not result.get("success"):
            return {
                "success": False,
                "error": result.get("error", "Unknown search error"),
                "output": None,
            }

        # Format results for display with markdown links
        results = result.get("results", [])
        if not results:
            return {
                "success": True,
                "output": f"No results found for: {query}",
                "result_count": 0,
            }

        # Build formatted output with markdown links
        output_lines = [
            f"Found {len(results)} result(s) for: {query}",
            "",
        ]

        for i, item in enumerate(results, 1):
            title = item.get("title", "Untitled")
            url = item.get("url", "")
            snippet = item.get("snippet", "")

            # Format as markdown link with snippet
            output_lines.append(f"{i}. [{title}]({url})")
            if snippet:
                # Truncate long snippets
                if len(snippet) > 200:
                    snippet = snippet[:200] + "..."
                output_lines.append(f"   {snippet}")
            output_lines.append("")

        return {
            "success": True,
            "output": "\n".join(output_lines),
            "result_count": len(results),
            "results": results,  # Include raw results for programmatic use
        }
