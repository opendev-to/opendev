"""Structured error types for OpenDev.

Provides typed error classes with structured fields for better retry logic,
error-specific recovery, and comprehensive provider error classification.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from enum import Enum
from typing import Optional


class ErrorCategory(str, Enum):
    CONTEXT_OVERFLOW = "context_overflow"
    OUTPUT_LENGTH = "output_length"
    RATE_LIMIT = "rate_limit"
    AUTH = "auth"
    API = "api"
    GATEWAY = "gateway"
    PERMISSION = "permission"
    EDIT_MISMATCH = "edit_mismatch"
    FILE_NOT_FOUND = "file_not_found"
    TIMEOUT = "timeout"
    UNKNOWN = "unknown"


@dataclass
class StructuredError:
    """Base structured error with metadata."""

    category: ErrorCategory
    message: str
    is_retryable: bool = False
    status_code: Optional[int] = None
    provider: Optional[str] = None
    original_error: Optional[str] = None

    @property
    def should_compact(self) -> bool:
        return self.category == ErrorCategory.CONTEXT_OVERFLOW

    @property
    def should_retry(self) -> bool:
        return self.is_retryable


@dataclass
class ContextOverflowError(StructuredError):
    """Context window exceeded — retryable via compaction."""

    category: ErrorCategory = field(default=ErrorCategory.CONTEXT_OVERFLOW, init=False)
    is_retryable: bool = field(default=True, init=False)
    token_count: Optional[int] = None
    token_limit: Optional[int] = None


@dataclass
class OutputLengthError(StructuredError):
    """Model output was truncated due to max output tokens."""

    category: ErrorCategory = field(default=ErrorCategory.OUTPUT_LENGTH, init=False)
    is_retryable: bool = field(default=True, init=False)


@dataclass
class RateLimitError(StructuredError):
    """Rate limit or quota exceeded — retryable after backoff."""

    category: ErrorCategory = field(default=ErrorCategory.RATE_LIMIT, init=False)
    is_retryable: bool = field(default=True, init=False)
    retry_after: Optional[float] = None


@dataclass
class AuthError(StructuredError):
    """Authentication or authorization failure — not retryable."""

    category: ErrorCategory = field(default=ErrorCategory.AUTH, init=False)
    is_retryable: bool = field(default=False, init=False)


@dataclass
class GatewayError(StructuredError):
    """Gateway/proxy returned HTML or non-JSON response — retryable."""

    category: ErrorCategory = field(default=ErrorCategory.GATEWAY, init=False)
    is_retryable: bool = field(default=True, init=False)


# ---------------------------------------------------------------------------
# Provider error pattern library
# ---------------------------------------------------------------------------

# Context overflow patterns across providers
_OVERFLOW_PATTERNS = [
    # Anthropic
    r"prompt is too long",
    r"max_tokens_exceeded",
    r"context length.*exceeded",
    r"maximum context length",
    # OpenAI
    r"maximum context length.*is \d+ tokens",
    r"This model's maximum context length is",
    r"reduce the length of the messages",
    r"context_length_exceeded",
    # Google
    r"exceeds the maximum.*tokens",
    r"RESOURCE_EXHAUSTED.*token",
    r"GenerateContentRequest.*too large",
    # Azure
    r"Tokens in prompt.*exceed.*limit",
    # Generic
    r"token limit",
    r"too many tokens",
    r"context.*too long",
    r"input.*too long",
    r"prompt.*too large",
]

_RATE_LIMIT_PATTERNS = [
    r"rate.?limit",
    r"too many requests",
    r"429",
    r"quota exceeded",
    r"capacity",
    r"overloaded",
]

_AUTH_PATTERNS = [
    r"invalid.*api.?key",
    r"authentication",
    r"unauthorized",
    r"invalid.*token",
    r"api key.*invalid",
]

_GATEWAY_PATTERNS = [
    r"<!doctype html",
    r"<html",
    r"502 Bad Gateway",
    r"503 Service Unavailable",
    r"504 Gateway Timeout",
    r"CloudFlare",
    r"nginx",
]


def classify_api_error(
    error_message: str,
    status_code: int | None = None,
    provider: str | None = None,
) -> StructuredError:
    """Classify an API error into a structured error type.

    Checks the raw error message against known patterns for context overflow,
    rate limiting, authentication failures, and gateway/proxy issues across
    all supported providers (Anthropic, OpenAI, Google, Azure).

    Args:
        error_message: The raw error message or response body.
        status_code: HTTP status code if available.
        provider: Provider name (e.g. "openai", "anthropic") if known.

    Returns:
        A StructuredError subclass matching the detected error category.
    """
    msg_lower = error_message.lower()

    # Check gateway patterns first (HTML responses)
    for pattern in _GATEWAY_PATTERNS:
        if re.search(pattern, error_message, re.IGNORECASE):
            friendly_msg = (
                "API returned an HTML error page. "
                "Check your proxy/VPN settings or try again."
            )
            if status_code == 401:
                friendly_msg = (
                    "Authentication failed at gateway. "
                    "Check your API key and proxy settings."
                )
            elif status_code == 403:
                friendly_msg = (
                    "Access denied at gateway. "
                    "Check your permissions and proxy settings."
                )
            return GatewayError(
                message=friendly_msg,
                status_code=status_code,
                provider=provider,
                original_error=error_message[:500],
            )

    # Context overflow
    for pattern in _OVERFLOW_PATTERNS:
        if re.search(pattern, msg_lower):
            return ContextOverflowError(
                message=error_message,
                provider=provider,
                original_error=error_message,
            )

    # Rate limiting
    for pattern in _RATE_LIMIT_PATTERNS:
        if re.search(pattern, msg_lower):
            retry_after = None
            retry_match = re.search(r"retry.?after[:\s]+(\d+\.?\d*)", msg_lower)
            if retry_match:
                retry_after = float(retry_match.group(1))
            return RateLimitError(
                message=error_message,
                provider=provider,
                retry_after=retry_after,
                original_error=error_message,
            )

    # Auth errors — check status code first, then patterns
    if status_code in (401, 403):
        return AuthError(
            message=error_message,
            status_code=status_code,
            provider=provider,
            original_error=error_message,
        )
    for pattern in _AUTH_PATTERNS:
        if re.search(pattern, msg_lower):
            return AuthError(
                message=error_message,
                status_code=status_code,
                provider=provider,
                original_error=error_message,
            )

    # Generic API error
    return StructuredError(
        category=ErrorCategory.API if status_code else ErrorCategory.UNKNOWN,
        message=error_message,
        is_retryable=status_code in (500, 502, 503, 504) if status_code else False,
        status_code=status_code,
        provider=provider,
        original_error=error_message,
    )
