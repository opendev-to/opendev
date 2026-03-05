"""Provider definitions and model configurations using model registry."""

from typing import Dict, List, Any, Optional

from swecli.config import get_model_registry


def get_provider_config(provider_id: str) -> Optional[Dict[str, Any]]:
    """Get configuration for a specific provider from registry."""
    registry = get_model_registry()
    provider_info = registry.get_provider(provider_id)

    if not provider_info:
        return None

    return {
        "name": provider_info.name,
        "description": provider_info.description,
        "env_var": provider_info.api_key_env,
        "api_url": provider_info.api_base_url,
        "api_format": "openai" if provider_id != "anthropic" else "anthropic",
    }


def get_provider_models(provider_id: str) -> List[Dict[str, str]]:
    """Get available models for a provider from registry."""
    registry = get_model_registry()
    provider_info = registry.get_provider(provider_id)

    if not provider_info:
        return []

    # Convert ModelInfo objects to dict format for wizard
    models = []
    for model_info in provider_info.list_models():
        # Create description with pricing and context info
        description = (
            f"{model_info.format_pricing()} • "
            f"{model_info.context_length//1000}k context"
        )
        if model_info.recommended:
            description = "Recommended - " + description

        models.append({
            "id": model_info.id,
            "name": model_info.name,
            "description": description,
        })

    return models


def get_provider_choices() -> List[tuple[str, str, str]]:
    """Get provider choices for the wizard menu from registry.

    Returns:
        List of (id, name, description) tuples, sorted with popular providers first
    """
    registry = get_model_registry()
    # list_providers() already returns providers in priority order
    return [
        (provider_info.id, provider_info.name, provider_info.description)
        for provider_info in registry.list_providers()
    ]


# Legacy PROVIDERS dict for backward compatibility
PROVIDERS: Dict[str, Dict[str, Any]] = {
    provider_id: get_provider_config(provider_id)
    for provider_id in ["fireworks", "openai", "anthropic"]
}
