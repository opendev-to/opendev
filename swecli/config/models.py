"""Model and provider configuration management."""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, List, Optional

from swecli.core.paths import get_paths

_LOG = logging.getLogger(__name__)

# Display order for provider lists (setup wizard, /models picker, etc.)
# Providers listed here appear first in this exact order; all others follow alphabetically.
PRIORITY_PROVIDERS = [
    "openai",
    "anthropic",
    "fireworks", "fireworks-ai",
    "google",
    "deepseek",
    "groq",
    "mistral",
    "cohere",
    "perplexity",
    "togetherai", "together",
]


def provider_sort_key(provider_id: str, provider_name: str) -> tuple[int, int | str]:
    """Sort key: priority providers first (in order), then alphabetical by name."""
    try:
        return (0, PRIORITY_PROVIDERS.index(provider_id))
    except ValueError:
        return (1, provider_name.lower())


@dataclass
class ModelInfo:
    """Information about a specific model."""

    id: str
    name: str
    provider: str
    context_length: int
    capabilities: List[str]
    pricing_input: float = 0.0
    pricing_output: float = 0.0
    pricing_unit: str = "per million tokens"
    serverless: bool = False
    tunable: bool = False
    recommended: bool = False
    max_tokens: Optional[int] = None
    supports_temperature: bool = True  # False for reasoning models (o1, o3, o4)
    api_type: str = "chat"  # "chat" for /v1/chat/completions, "responses" for /v1/responses

    def __str__(self) -> str:
        """String representation of model."""
        caps = ", ".join(self.capabilities)
        return (
            f"{self.name}\n"
            f"  Provider: {self.provider}\n"
            f"  Context: {self.context_length:,} tokens\n"
            f"  Capabilities: {caps}"
        )

    def format_pricing(self) -> str:
        """Format pricing for display."""
        if self.pricing_input == 0 and self.pricing_output == 0:
            return "N/A"
        return f"${self.pricing_input:.2f} in / ${self.pricing_output:.2f} out {self.pricing_unit}"


@dataclass
class ProviderInfo:
    """Information about a provider."""

    id: str
    name: str
    description: str
    api_key_env: str
    api_base_url: str
    models: Dict[str, ModelInfo]

    def list_models(self, capability: Optional[str] = None) -> List[ModelInfo]:
        """List all models, optionally filtered by capability."""
        models = list(self.models.values())
        if capability:
            models = [m for m in models if capability in m.capabilities]
        return sorted(models, key=lambda m: m.context_length, reverse=True)

    def get_recommended_model(self) -> Optional[ModelInfo]:
        """Get the recommended model for this provider."""
        for model in self.models.values():
            if model.recommended:
                return model
        # If no recommended, return first model
        return list(self.models.values())[0] if self.models else None


class ModelRegistry:
    """Registry for managing model and provider configurations."""

    def __init__(self):
        """Initialize model registry."""
        self.providers: Dict[str, ProviderInfo] = {}
        self._load_config()

    def _load_config(self) -> None:
        """Load model configuration.

        Priority:
          1. Cached per-provider files (~/.opendev/cache/providers/*.json)
             — populated from models.dev on first run and refreshed every 24h
          2. If cache empty, try sync_provider_cache() (blocking, first run only)
          3. Schedule background refresh if cache exists but is stale
        """
        from .models_dev_loader import is_cache_stale, sync_provider_cache

        cache_providers_dir = get_paths().global_cache_dir / "providers"

        # Try loading from cache (even if stale — fast startup)
        loaded = self._load_providers_from_dir(cache_providers_dir)

        if not loaded:
            # Cache empty (first run) — blocking fetch is acceptable
            sync_provider_cache()
            loaded = self._load_providers_from_dir(cache_providers_dir)

        if not loaded:
            _LOG.warning(
                "No provider configurations loaded. "
                "Check network connectivity and retry, or run: swecli setup"
            )

        # Schedule background refresh if cache is stale (non-blocking)
        if loaded and is_cache_stale(cache_providers_dir):
            import threading

            threading.Thread(
                target=sync_provider_cache,
                daemon=True,
                name="models-dev-sync",
            ).start()

    def _load_providers_from_dir(self, directory: Path) -> bool:
        """Load all provider JSON files from a directory into self.providers.

        Returns True if at least one provider was loaded.
        """
        if not directory.exists():
            return False

        count = 0
        for provider_file in sorted(directory.glob("*.json")):
            if provider_file.name.startswith("."):
                continue  # Skip .last_sync marker
            try:
                with open(provider_file) as f:
                    provider_data = json.load(f)

                provider_id = provider_data["id"]
                models: Dict[str, ModelInfo] = {}

                for model_key, model_data in provider_data.get("models", {}).items():
                    pricing = model_data.get("pricing", {})
                    models[model_key] = ModelInfo(
                        id=model_data["id"],
                        name=model_data["name"],
                        provider=model_data["provider"],
                        context_length=model_data["context_length"],
                        capabilities=model_data["capabilities"],
                        pricing_input=pricing.get("input", 0.0),
                        pricing_output=pricing.get("output", 0.0),
                        pricing_unit=pricing.get("unit", "per million tokens"),
                        serverless=model_data.get("serverless", False),
                        tunable=model_data.get("tunable", False),
                        recommended=model_data.get("recommended", False),
                        max_tokens=model_data.get("max_tokens"),
                        supports_temperature=model_data.get("supports_temperature", True),
                        api_type=model_data.get("api_type", "chat"),
                    )

                self.providers[provider_id] = ProviderInfo(
                    id=provider_id,
                    name=provider_data["name"],
                    description=provider_data.get("description", ""),
                    api_key_env=provider_data.get("api_key_env", ""),
                    api_base_url=provider_data.get("api_base_url", ""),
                    models=models,
                )
                count += 1
            except Exception as exc:
                _LOG.debug("Failed to load provider %s: %s", provider_file.name, exc)

        return count > 0

    def get_provider(self, provider_id: str) -> Optional[ProviderInfo]:
        """Get provider information by ID."""
        return self.providers.get(provider_id)

    def list_providers(self) -> List[ProviderInfo]:
        """List all available providers, sorted by priority then alphabetically."""
        return sorted(
            self.providers.values(),
            key=lambda p: provider_sort_key(p.id, p.name),
        )

    def get_model(self, provider_id: str, model_key: str) -> Optional[ModelInfo]:
        """Get model information by provider and model key."""
        provider = self.get_provider(provider_id)
        if provider:
            return provider.models.get(model_key)
        return None

    def find_model_by_id(self, model_id: str) -> Optional[tuple[str, str, ModelInfo]]:
        """Find a model by its full ID.

        Returns:
            Tuple of (provider_id, model_key, ModelInfo) or None
        """
        for provider_id, provider in self.providers.items():
            for model_key, model in provider.models.items():
                if model.id == model_id:
                    return (provider_id, model_key, model)
        return None

    def list_all_models(
        self,
        capability: Optional[str] = None,
        max_price: Optional[float] = None,
    ) -> List[tuple[str, ModelInfo]]:
        """List all models across all providers.

        Args:
            capability: Filter by capability (e.g., "vision", "code")
            max_price: Maximum output price per million tokens

        Returns:
            List of (provider_id, ModelInfo) tuples
        """
        models = []
        for provider_id, provider in self.providers.items():
            for model in provider.models.values():
                # Apply filters
                if capability and capability not in model.capabilities:
                    continue
                if max_price is not None and model.pricing_output > max_price:
                    continue
                models.append((provider_id, model))

        # Sort by price (output tokens)
        return sorted(models, key=lambda x: x[1].pricing_output)


# Global registry instance
_registry: Optional[ModelRegistry] = None


def get_model_registry() -> ModelRegistry:
    """Get the global model registry instance."""
    global _registry
    if _registry is None:
        _registry = ModelRegistry()
    return _registry
