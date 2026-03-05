"""API key validation for different providers."""

import requests
from typing import Tuple, Optional


def validate_fireworks_key(api_key: str) -> Tuple[bool, Optional[str]]:
    """Validate Fireworks API key by making a test request.

    Returns:
        (success, error_message)
    """
    try:
        response = requests.post(
            "https://api.fireworks.ai/inference/v1/chat/completions",
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {api_key}",
            },
            json={
                "model": "accounts/fireworks/models/llama-v3p3-70b-instruct",
                "messages": [{"role": "user", "content": "test"}],
                "max_tokens": 10,
            },
            timeout=10,
        )
        if response.status_code == 200:
            return True, None
        elif response.status_code == 401:
            return False, "Invalid API key"
        else:
            return False, f"API error: {response.status_code}"
    except requests.exceptions.Timeout:
        return False, "Request timeout - please check your connection"
    except Exception as e:
        return False, f"Validation failed: {str(e)}"


def validate_openai_key(api_key: str) -> Tuple[bool, Optional[str]]:
    """Validate OpenAI API key by making a test request.

    Returns:
        (success, error_message)
    """
    try:
        response = requests.post(
            "https://api.openai.com/v1/chat/completions",
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {api_key}",
            },
            json={
                "model": "gpt-3.5-turbo",
                "messages": [{"role": "user", "content": "test"}],
                "max_tokens": 10,
            },
            timeout=10,
        )
        if response.status_code == 200:
            return True, None
        elif response.status_code == 401:
            return False, "Invalid API key"
        else:
            return False, f"API error: {response.status_code}"
    except requests.exceptions.Timeout:
        return False, "Request timeout - please check your connection"
    except Exception as e:
        return False, f"Validation failed: {str(e)}"


def validate_anthropic_key(api_key: str) -> Tuple[bool, Optional[str]]:
    """Validate Anthropic API key by making a test request.

    Returns:
        (success, error_message)
    """
    try:
        response = requests.post(
            "https://api.anthropic.com/v1/messages",
            headers={
                "Content-Type": "application/json",
                "x-api-key": api_key,
                "anthropic-version": "2023-06-01",
            },
            json={
                "model": "claude-3-haiku-20240307",
                "messages": [{"role": "user", "content": "test"}],
                "max_tokens": 10,
            },
            timeout=10,
        )
        if response.status_code == 200:
            return True, None
        elif response.status_code == 401:
            return False, "Invalid API key"
        else:
            return False, f"API error: {response.status_code}"
    except requests.exceptions.Timeout:
        return False, "Request timeout - please check your connection"
    except Exception as e:
        return False, f"Validation failed: {str(e)}"


def validate_api_key(provider: str, api_key: str) -> Tuple[bool, Optional[str]]:
    """Validate API key for the given provider.

    Args:
        provider: Provider ID (fireworks, openai, anthropic)
        api_key: API key to validate

    Returns:
        (success, error_message)
    """
    validators = {
        "fireworks": validate_fireworks_key,
        "openai": validate_openai_key,
        "anthropic": validate_anthropic_key,
    }

    validator = validators.get(provider)
    if not validator:
        return False, f"Unknown provider: {provider}"

    return validator(api_key)
