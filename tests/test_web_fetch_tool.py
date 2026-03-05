"""Unit tests for the Crawl4AI WebFetchTool helpers."""

from pathlib import Path

import pytest
from crawl4ai.deep_crawling import BFSDeepCrawlStrategy, BestFirstCrawlingStrategy, DFSDeepCrawlStrategy
from crawl4ai.deep_crawling.filters import DomainFilter, FilterChain, URLPatternFilter

from swecli.models.config import AppConfig
from swecli.core.context_engineering.tools.implementations import WebFetchTool


@pytest.fixture()
def web_fetch_tool(tmp_path: Path) -> WebFetchTool:
    """Provide a WebFetchTool instance with default config."""
    return WebFetchTool(AppConfig(), tmp_path)


def test_build_deep_strategy_defaults_to_best_first(web_fetch_tool: WebFetchTool) -> None:
    """Default strategy should be relevance-driven best_first."""
    strategy = web_fetch_tool._build_deep_strategy(
        strategy=None,
        max_depth=2,
        include_external=False,
        max_pages=None,
        filter_chain=None,
        BFSDeepCrawlStrategy=BFSDeepCrawlStrategy,
        DFSDeepCrawlStrategy=DFSDeepCrawlStrategy,
        BestFirstCrawlingStrategy=BestFirstCrawlingStrategy,
    )

    assert isinstance(strategy, BestFirstCrawlingStrategy)
    assert getattr(strategy, "max_depth") == 2
    assert getattr(strategy, "include_external") is False


def test_build_deep_strategy_respects_explicit_bfs(web_fetch_tool: WebFetchTool) -> None:
    """Explicit bfs strategy should honor all provided options."""
    filter_chain = web_fetch_tool._build_filter_chain(
        allowed_domains=["docs.example.com"],
        blocked_domains=["old.docs.example.com"],
        url_patterns=["*guide*"],
        FilterChain=FilterChain,
        DomainFilter=DomainFilter,
        URLPatternFilter=URLPatternFilter,
    )

    strategy = web_fetch_tool._build_deep_strategy(
        strategy="bfs",
        max_depth=3,
        include_external=True,
        max_pages=5,
        filter_chain=filter_chain,
        BFSDeepCrawlStrategy=BFSDeepCrawlStrategy,
        DFSDeepCrawlStrategy=DFSDeepCrawlStrategy,
        BestFirstCrawlingStrategy=BestFirstCrawlingStrategy,
    )

    assert isinstance(strategy, BFSDeepCrawlStrategy)
    assert getattr(strategy, "max_depth") == 3
    assert getattr(strategy, "include_external") is True
    assert getattr(strategy, "max_pages") == 5
    assert getattr(strategy, "filter_chain") is filter_chain


def test_build_filter_chain_combines_filters(web_fetch_tool: WebFetchTool) -> None:
    """Filter chain should contain domain and pattern filters when provided."""
    filter_chain = web_fetch_tool._build_filter_chain(
        allowed_domains=["docs.example.com"],
        blocked_domains=["old.docs.example.com"],
        url_patterns=["*guide*"],
        FilterChain=FilterChain,
        DomainFilter=DomainFilter,
        URLPatternFilter=URLPatternFilter,
    )

    assert isinstance(filter_chain, FilterChain)
    assert [type(filter_) for filter_ in filter_chain.filters] == [DomainFilter, URLPatternFilter]


def test_build_filter_chain_returns_none_without_constraints(web_fetch_tool: WebFetchTool) -> None:
    """No constraints => no filter chain."""
    assert web_fetch_tool._build_filter_chain(
        None, None, None,
        FilterChain=FilterChain,
        DomainFilter=DomainFilter,
        URLPatternFilter=URLPatternFilter,
    ) is None
