"""Handlers for web-related tool invocations."""

from __future__ import annotations

from typing import Any


class WebToolHandler:
    """Executes web fetch operations."""

    def __init__(self, web_fetch_tool: Any) -> None:
        self._web_fetch_tool = web_fetch_tool

    def fetch_url(self, args: dict[str, Any]) -> dict[str, Any]:
        if not self._web_fetch_tool:
            return {"success": False, "error": "WebFetchTool not available"}

        url = args["url"]
        extract_text = args.get("extract_text", True)
        max_length = args.get("max_length", 50000)
        deep_crawl = args.get("deep_crawl", False)
        crawl_strategy = args.get("crawl_strategy", "best_first")
        max_depth = args.get("max_depth", 1)
        include_external = args.get("include_external", False)
        max_pages = args.get("max_pages")
        allowed_domains = args.get("allowed_domains")
        blocked_domains = args.get("blocked_domains")
        url_patterns = args.get("url_patterns")
        stream = args.get("stream", False)

        try:
            result = self._web_fetch_tool.fetch_url(
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

            if not result["success"]:
                return {"success": False, "error": result["error"], "output": None}

            base_header = (
                f"Fetched: {result.get('url', url)}\n"
                f"Status: {result.get('status_code', 'unknown')}\n"
                f"Content-Type: {result.get('content_type', 'unknown')}"
            )

            if deep_crawl:
                base_header += f"\nPages Crawled: {result.get('page_count', 'unknown')}"

            output = f"{base_header}\n\n{result['content']}"

            return {"success": True, "output": output, "error": None}
        except Exception as exc:  # noqa: BLE001
            return {"success": False, "error": str(exc), "output": None}
