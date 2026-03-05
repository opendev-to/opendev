"""Tests for HTTP client exponential backoff retry logic."""

from unittest.mock import MagicMock, patch

import pytest
import httpx

from swecli.core.agents.components.api.http_client import (
    MAX_RETRIES,
    AgentHttpClient,
    HttpResult,
)


def _make_response(status_code: int = 200, headers: dict | None = None) -> MagicMock:
    """Create a mock httpx.Response with the given status code."""
    resp = MagicMock(spec=httpx.Response)
    resp.status_code = status_code
    resp.headers = httpx.Headers(headers or {})
    resp.text = ""
    resp.json.return_value = {}
    return resp


class TestHttpClientRetry:
    """Test exponential backoff retry logic."""

    @pytest.fixture()
    def client(self) -> AgentHttpClient:
        return AgentHttpClient("https://api.example.com/v1/chat", {"Authorization": "Bearer test"})

    def test_retry_on_429(self, client: AgentHttpClient) -> None:
        """Should retry up to MAX_RETRIES times with backoff on rate limit."""
        responses = [
            _make_response(429),
            _make_response(429),
            _make_response(200),
        ]
        call_count = 0

        def fast_monotonic() -> float:
            nonlocal call_count
            call_count += 1
            # Always return a large value so the sleep loop exits immediately
            return float(call_count * 100)

        with (
            patch.object(client._client, "post", side_effect=responses),
            patch(
                "swecli.core.agents.components.api.http_client.time.monotonic",
                side_effect=fast_monotonic,
            ),
            patch("swecli.core.agents.components.api.http_client.time.sleep"),
        ):
            result = client.post_json({"model": "test"})
        assert result.success
        assert result.response is not None
        assert result.response.status_code == 200

    def test_retry_on_503(self, client: AgentHttpClient) -> None:
        """Should retry on service unavailable."""
        responses = [
            _make_response(503),
            _make_response(200),
        ]
        counter = {"n": 0}

        def fast_monotonic() -> float:
            counter["n"] += 1
            return float(counter["n"] * 100)

        with (
            patch.object(client._client, "post", side_effect=responses),
            patch(
                "swecli.core.agents.components.api.http_client.time.monotonic",
                side_effect=fast_monotonic,
            ),
            patch("swecli.core.agents.components.api.http_client.time.sleep"),
        ):
            result = client.post_json({"model": "test"})
        assert result.success
        assert result.response is not None
        assert result.response.status_code == 200

    def test_no_retry_on_400(self, client: AgentHttpClient) -> None:
        """Should NOT retry on client errors (400, 401, 404)."""
        for code in (400, 401, 404):
            with patch.object(client._client, "post", return_value=_make_response(code)):
                result = client.post_json({"model": "test"})
            assert result.success  # HTTP result success (transport OK)
            assert result.response is not None
            assert result.response.status_code == code

    def test_respects_retry_after_header(self, client: AgentHttpClient) -> None:
        """Should use Retry-After header when present."""
        resp_429 = _make_response(429, headers={"Retry-After": "5"})
        resp_200 = _make_response(200)

        counter = {"n": 0}

        def fast_monotonic() -> float:
            counter["n"] += 1
            return float(counter["n"] * 100)

        with (
            patch.object(client._client, "post", side_effect=[resp_429, resp_200]),
            patch(
                "swecli.core.agents.components.api.http_client.time.monotonic",
                side_effect=fast_monotonic,
            ),
            patch("swecli.core.agents.components.api.http_client.time.sleep"),
        ):
            result = client.post_json({"model": "test"})

        assert result.success
        assert result.response is not None
        assert result.response.status_code == 200

    def test_max_retries_exceeded(self, client: AgentHttpClient) -> None:
        """Should return error after exhausting retries."""
        responses = [_make_response(429) for _ in range(MAX_RETRIES + 1)]
        counter = {"n": 0}

        def fast_monotonic() -> float:
            counter["n"] += 1
            return float(counter["n"] * 100)

        with (
            patch.object(client._client, "post", side_effect=responses),
            patch(
                "swecli.core.agents.components.api.http_client.time.monotonic",
                side_effect=fast_monotonic,
            ),
            patch("swecli.core.agents.components.api.http_client.time.sleep"),
        ):
            result = client.post_json({"model": "test"})
        assert result.success  # Transport succeeded
        assert result.response is not None
        assert result.response.status_code == 429

    def test_interrupt_during_retry(self, client: AgentHttpClient) -> None:
        """Should abort retry if task_monitor.should_interrupt() returns True."""
        monitor = MagicMock()
        # First call: not interrupted (for the initial request), second: interrupted
        monitor.should_interrupt.side_effect = [False, True]

        resp_429 = _make_response(429)
        with patch.object(
            client, "_execute_request", return_value=HttpResult(success=True, response=resp_429)
        ):
            result = client.post_json({"model": "test"}, task_monitor=monitor)
        assert not result.success
        assert result.interrupted

    def test_no_retry_on_network_error(self, client: AgentHttpClient) -> None:
        """Network errors should not be retried."""
        with patch.object(
            client._client, "post", side_effect=httpx.ConnectError("Connection refused")
        ):
            result = client.post_json({"model": "test"})
        assert not result.success
        assert "Connection refused" in (result.error or "")

    def test_success_on_first_try(self, client: AgentHttpClient) -> None:
        """Successful request should return immediately without retries."""
        with patch.object(
            client._client, "post", return_value=_make_response(200)
        ) as mock_post:
            result = client.post_json({"model": "test"})
        assert result.success
        assert mock_post.call_count == 1
