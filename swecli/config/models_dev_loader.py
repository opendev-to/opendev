"""Helpers for loading the Models.dev catalog used by OpenCode."""

from __future__ import annotations

import json
import logging
import os
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any, Dict, List, Optional

from swecli.core.paths import get_paths

_LOG = logging.getLogger(__name__)

MODELS_DEV_URL = "https://models.dev/api.json"
DEFAULT_CACHE_TTL = 60 * 60 * 24  # 24 hours


def _load_from_path(path: Path) -> Dict[str, Any]:
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def _fetch_models_dev(url: str = MODELS_DEV_URL, timeout: int = 10) -> Optional[Dict[str, Any]]:
    request = urllib.request.Request(
        url,
        headers={
            "User-Agent": os.getenv("OPENDEV_HTTP_USER_AGENT", "swecli/unknown"),
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            payload = response.read().decode("utf-8")
            return json.loads(payload)
    except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError) as exc:
        _LOG.debug("Failed to fetch models.dev catalog: %s", exc)
    except Exception as exc:  # pragma: no cover - safeguard
        _LOG.warning("Unexpected error fetching models.dev catalog: %s", exc)
    return None


def load_models_dev_catalog(
    *,
    cache_dir: Optional[Path] = None,
    cache_ttl: int = DEFAULT_CACHE_TTL,
    disable_network: Optional[bool] = None,
) -> Optional[Dict[str, Any]]:
    """Load the Models.dev provider catalog, respecting cache and overrides.

    Precedence:
        1. OPENDEV_MODELS_DEV_PATH environment variable (JSON file path)
        2. Cached response under ~/.opendev/cache/models.dev.json (if fresh)
        3. Live fetch from https://models.dev/api.json (unless disabled)

    Args:
        cache_dir: Optional directory to use for cache writes.
        cache_ttl: Maximum age (seconds) before cached data is considered stale.
        disable_network: Force skipping live fetch attempts.

    Returns:
        Parsed JSON structure from Models.dev or None if unavailable.
    """

    override_path = os.getenv("OPENDEV_MODELS_DEV_PATH")
    if override_path:
        override = Path(override_path).expanduser()
        if override.exists():
            try:
                return _load_from_path(override)
            except Exception as exc:
                _LOG.warning("Failed to load Models.dev catalog override from %s: %s", override, exc)
        else:
            _LOG.warning("OPENDEV_MODELS_DEV_PATH %s does not exist", override)

    if cache_dir is None:
        cache_dir = get_paths().global_cache_dir
    cache_path: Optional[Path] = None
    try:
        cache_dir.mkdir(parents=True, exist_ok=True)
        cache_path = cache_dir / "models.dev.json"
    except PermissionError:
        _LOG.debug("Cannot create cache directory %s; caching disabled", cache_dir)

    disable_fetch = disable_network
    if disable_fetch is None:
        disable_fetch = os.getenv("OPENDEV_DISABLE_REMOTE_MODELS", "").lower() in {"1", "true", "yes"}

    now = time.time()
    if cache_path and cache_path.exists():
        try:
            cache_mtime = cache_path.stat().st_mtime
        except OSError:
            cache_mtime = 0
        if now - cache_mtime <= cache_ttl:
            try:
                return _load_from_path(cache_path)
            except Exception as exc:
                _LOG.debug("Failed to read cached Models.dev catalog: %s", exc)
        elif disable_fetch:
            try:
                return _load_from_path(cache_path)
            except Exception:
                pass

    if disable_fetch:
        return None

    data = _fetch_models_dev()
    if data is not None:
        if cache_path:
            try:
                cache_path.write_text(json.dumps(data), encoding="utf-8")
            except Exception as exc:  # pragma: no cover - cache is best-effort
                _LOG.debug("Unable to write Models.dev cache: %s", exc)
        return data

    # As a fallback, try returning whatever stale cache we might have
    if cache_path and cache_path.exists():
        try:
            return _load_from_path(cache_path)
        except Exception:
            pass

    return None


# ---------------------------------------------------------------------------
# Per-provider cache (sync_provider_cache)
# ---------------------------------------------------------------------------


def is_cache_stale(providers_dir: Path, ttl: int = DEFAULT_CACHE_TTL) -> bool:
    """Check if the per-provider cache needs refreshing."""
    marker = providers_dir / ".last_sync"
    if not marker.exists():
        return True
    try:
        return time.time() - marker.stat().st_mtime > ttl
    except OSError:
        return True


def sync_provider_cache(
    cache_dir: Optional[Path] = None,
    cache_ttl: int = DEFAULT_CACHE_TTL,
) -> bool:
    """Fetch models.dev and write per-provider JSON files to cache.

    Cache location: ~/.opendev/cache/providers/{provider_id}.json
    Uses a .last_sync marker file for TTL checks.

    Returns True if cache was updated.
    """
    if cache_dir is None:
        cache_dir = get_paths().global_cache_dir
    providers_dir = cache_dir / "providers"

    # Check TTL via marker file
    marker = providers_dir / ".last_sync"
    if marker.exists():
        try:
            if time.time() - marker.stat().st_mtime <= cache_ttl:
                return False  # Still fresh
        except OSError:
            pass

    # Respect env overrides
    if os.getenv("OPENDEV_DISABLE_REMOTE_MODELS", "").lower() in {"1", "true", "yes"}:
        return False

    # Support OPENDEV_MODELS_DEV_PATH override
    override_path = os.getenv("OPENDEV_MODELS_DEV_PATH")
    if override_path:
        override = Path(override_path).expanduser()
        if override.exists():
            try:
                catalog = _load_from_path(override)
            except Exception as exc:
                _LOG.warning("Failed to load override %s: %s", override, exc)
                return False
        else:
            _LOG.warning("OPENDEV_MODELS_DEV_PATH %s does not exist", override)
            return False
    else:
        catalog = _fetch_models_dev()

    if not catalog:
        return False

    providers_dir.mkdir(parents=True, exist_ok=True)

    for provider_id, provider_data in catalog.items():
        try:
            provider_json = _convert_provider_to_internal(provider_id, provider_data)
            if provider_json and provider_json.get("models"):
                path = providers_dir / f"{provider_id}.json"
                path.write_text(json.dumps(provider_json, indent=2), encoding="utf-8")
        except Exception as exc:
            _LOG.debug("Failed to write cache for provider %s: %s", provider_id, exc)

    # Touch marker
    try:
        marker.touch()
    except OSError:
        pass

    return True


def _convert_provider_to_internal(
    provider_id: str, provider_data: Dict[str, Any]
) -> Dict[str, Any]:
    """Convert a models.dev provider entry to our internal JSON format.

    Input (models.dev):
        { "name": "Anthropic", "env": ["ANTHROPIC_API_KEY"], "api": "...",
          "models": { "model-id": { "id", "name", "limit", "cost", "modalities", ... } } }

    Output (our format):
        { "id", "name", "description", "api_key_env", "api_base_url",
          "models": { "model-id": { "id", "name", "provider", "context_length",
                      "capabilities", "pricing", "max_tokens", "supports_temperature" } } }
    """
    provider_name = provider_data.get("name") or provider_id.title()
    env_vars: List[str] = provider_data.get("env") or []
    models_block: Dict[str, Any] = provider_data.get("models") or {}

    converted_models: Dict[str, Any] = {}
    first_model = True
    for model_key, md in models_block.items():
        limit = md.get("limit") or {}
        context = int(limit.get("context") or 0)
        if context <= 0:
            continue

        modalities = md.get("modalities") or {}
        input_mods: List[str] = modalities.get("input") or []
        if input_mods and "text" not in input_mods:
            continue  # Skip embedding-only / non-text models

        cost = md.get("cost") or {}
        capabilities: List[str] = []
        if not input_mods or "text" in input_mods:
            capabilities.append("text")
        if "image" in input_mods:
            capabilities.append("vision")
        if md.get("reasoning"):
            capabilities.append("reasoning")
        if "audio" in input_mods:
            capabilities.append("audio")

        max_tokens_raw = int(limit.get("output") or 0)
        converted_models[model_key] = {
            "id": md.get("id") or model_key,
            "name": md.get("name") or model_key,
            "provider": provider_name,
            "context_length": context,
            "capabilities": capabilities,
            "pricing": {
                "input": float(cost.get("input") or 0),
                "output": float(cost.get("output") or 0),
                "unit": "per 1M tokens",
            },
            "recommended": first_model,
            "max_tokens": max_tokens_raw if max_tokens_raw > 0 else None,
            "supports_temperature": md.get("temperature", True),
        }
        first_model = False

    return {
        "id": provider_id,
        "name": provider_name,
        "description": f"{provider_name} models",
        "api_key_env": env_vars[0] if env_vars else "",
        "api_base_url": provider_data.get("api") or "",
        "models": converted_models,
    }
