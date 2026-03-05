"""Vision Language Model tool for analyzing images."""

import os
import base64
import requests
from pathlib import Path
from typing import Optional, Dict, Any

from swecli.models.config import AppConfig
from swecli.config import get_model_registry
from swecli.core.agents.components import build_max_tokens_param


class VLMTool:
    """Tool for analyzing images using Vision Language Models."""

    def __init__(self, config: AppConfig, working_dir: Path):
        """Initialize VLM tool.

        Args:
            config: Application configuration
            working_dir: Working directory for resolving relative paths
        """
        self.config = config
        self.working_dir = working_dir
        # Extended timeout for VLM requests (connect=10s, read=300s for image analysis)
        self.timeout = (10, 300)

    def is_available(self) -> bool:
        """Check if VLM functionality is available.

        Returns:
            True if VLM model is configured, False otherwise
        """
        return bool(self.config.model_vlm and self.config.model_vlm_provider)

    def encode_image(self, image_path: str) -> Optional[str]:
        """Encode an image file to base64.

        Args:
            image_path: Path to the image file (absolute or relative to working_dir)

        Returns:
            Base64-encoded image string, or None if file not found/readable
        """
        try:
            # Resolve path relative to working directory
            path = Path(image_path)
            if not path.is_absolute():
                path = self.working_dir / path

            # Read and encode the image
            with open(path, "rb") as image_file:
                return base64.b64encode(image_file.read()).decode("utf-8")
        except FileNotFoundError:
            return None
        except Exception as e:
            return None

    def analyze_image(
        self,
        prompt: str,
        image_path: Optional[str] = None,
        image_url: Optional[str] = None,
        max_tokens: Optional[int] = None,
    ) -> Dict[str, Any]:
        """Analyze an image using the configured VLM model.

        Args:
            prompt: Text prompt describing what to analyze in the image
            image_path: Path to local image file (takes precedence over image_url)
            image_url: URL of online image (used if image_path not provided)
            max_tokens: Maximum tokens in response (defaults to config value)

        Returns:
            Dictionary with success, content, and optional error
        """
        # Check if VLM is configured
        if not self.is_available():
            return {
                "success": False,
                "error": (
                    "Vision model not configured. "
                    "Please configure a VLM model using '/models' command and select a Vision model."
                ),
                "content": None,
            }

        # Validate that at least one image source is provided
        if not image_path and not image_url:
            return {
                "success": False,
                "error": "Either image_path or image_url must be provided",
                "content": None,
            }

        # Get VLM model info
        vlm_info = self.config.get_vlm_model_info()
        if not vlm_info:
            return {
                "success": False,
                "error": "Vision model configuration is invalid",
                "content": None,
            }

        provider_id, model_id, model_info = vlm_info

        # Get provider info from registry
        registry = get_model_registry()
        provider_info = registry.get_provider(provider_id)
        if not provider_info:
            return {
                "success": False,
                "error": f"Provider '{provider_id}' not found",
                "content": None,
            }

        # Get API key from environment
        api_key_env = provider_info.api_key_env
        api_key = os.getenv(api_key_env)
        if not api_key:
            return {
                "success": False,
                "error": f"API key not found. Please set {api_key_env} environment variable.",
                "content": None,
            }

        # Prepare image URL (base64 for local files, direct URL for online images)
        final_image_url = None
        if image_path:
            # Local file - encode to base64
            base64_image = self.encode_image(image_path)
            if base64_image is None:
                return {
                    "success": False,
                    "error": f"Failed to read image file: {image_path}",
                    "content": None,
                }
            # Detect image type from extension
            path = Path(image_path)
            ext = path.suffix.lower()
            mime_type = {
                ".jpg": "image/jpeg",
                ".jpeg": "image/jpeg",
                ".png": "image/png",
                ".gif": "image/gif",
                ".webp": "image/webp",
            }.get(
                ext, "image/jpeg"
            )  # Default to jpeg
            final_image_url = f"data:{mime_type};base64,{base64_image}"
        elif image_url:
            # Online URL - use directly
            if not image_url.startswith(("http://", "https://", "data:")):
                return {
                    "success": False,
                    "error": f"Invalid image URL: must start with http://, https://, or data:",
                    "content": None,
                }
            final_image_url = image_url

        # Use provided max_tokens or fallback to config
        if max_tokens is None:
            max_tokens = self.config.max_tokens

        # Call the appropriate provider's API
        try:
            if provider_id == "fireworks":
                return self._call_fireworks_api(
                    api_key, model_info.id, prompt, final_image_url, max_tokens
                )
            elif provider_id == "openai":
                return self._call_openai_api(
                    api_key, model_info.id, prompt, final_image_url, max_tokens
                )
            elif provider_id == "anthropic":
                return self._call_anthropic_api(
                    api_key, model_info.id, prompt, final_image_url, max_tokens
                )
            else:
                return {
                    "success": False,
                    "error": f"Provider '{provider_id}' not supported for vision tasks",
                    "content": None,
                }

        except requests.exceptions.Timeout:
            # Show read timeout (second value in tuple)
            timeout_seconds = self.timeout[1] if isinstance(self.timeout, tuple) else self.timeout
            return {
                "success": False,
                "error": f"Request timeout after {timeout_seconds} seconds",
                "content": None,
            }
        except requests.exceptions.RequestException as e:
            return {
                "success": False,
                "error": f"Request failed: {str(e)}",
                "content": None,
            }
        except Exception as e:
            return {
                "success": False,
                "error": f"Unexpected error: {str(e)}",
                "content": None,
            }

    def _call_fireworks_api(
        self,
        api_key: str,
        model_id: str,
        prompt: str,
        image_url: str,
        max_tokens: int,
    ) -> Dict[str, Any]:
        """Call Fireworks AI vision API.

        Args:
            api_key: Fireworks API key
            model_id: Full model ID
            prompt: Text prompt
            image_url: Image URL
            max_tokens: Max tokens in response

        Returns:
            Result dictionary
        """
        url = "https://api.fireworks.ai/inference/v1/chat/completions"

        payload = {
            "model": model_id,
            "max_tokens": max_tokens,
            "top_p": 1,
            "top_k": 40,
            "presence_penalty": 0,
            "frequency_penalty": 0,
            "temperature": self.config.temperature,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": prompt},
                        {"type": "image_url", "image_url": {"url": image_url}},
                    ],
                }
            ],
        }

        headers = {
            "Accept": "application/json",
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}",
        }

        response = requests.post(url, headers=headers, json=payload, timeout=self.timeout)

        if response.status_code != 200:
            return {
                "success": False,
                "error": f"HTTP {response.status_code}: {response.text}",
                "content": None,
            }

        result = response.json()
        content = result.get("choices", [{}])[0].get("message", {}).get("content", "")

        return {
            "success": True,
            "content": content,
            "error": None,
            "model": model_id,
            "provider": "fireworks",
        }

    def _call_openai_api(
        self,
        api_key: str,
        model_id: str,
        prompt: str,
        image_url: str,
        max_tokens: int,
    ) -> Dict[str, Any]:
        """Call OpenAI vision API.

        Args:
            api_key: OpenAI API key
            model_id: Model ID (e.g., gpt-4-vision-preview)
            prompt: Text prompt
            image_url: Image URL
            max_tokens: Max tokens in response

        Returns:
            Result dictionary
        """
        url = "https://api.openai.com/v1/chat/completions"

        payload = {
            "model": model_id,
            **build_max_tokens_param(model_id, max_tokens),
            "temperature": self.config.temperature,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": prompt},
                        {"type": "image_url", "image_url": {"url": image_url}},
                    ],
                }
            ],
        }

        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}",
        }

        response = requests.post(url, headers=headers, json=payload, timeout=self.timeout)

        if response.status_code != 200:
            return {
                "success": False,
                "error": f"HTTP {response.status_code}: {response.text}",
                "content": None,
            }

        result = response.json()
        content = result.get("choices", [{}])[0].get("message", {}).get("content", "")

        return {
            "success": True,
            "content": content,
            "error": None,
            "model": model_id,
            "provider": "openai",
        }

    def _call_anthropic_api(
        self,
        api_key: str,
        model_id: str,
        prompt: str,
        image_url: str,
        max_tokens: int,
    ) -> Dict[str, Any]:
        """Call Anthropic vision API.

        Args:
            api_key: Anthropic API key
            model_id: Model ID (e.g., claude-3-opus-20240229)
            prompt: Text prompt
            image_url: Image URL
            max_tokens: Max tokens in response

        Returns:
            Result dictionary
        """
        url = "https://api.anthropic.com/v1/messages"

        # Anthropic requires downloading the image and base64 encoding
        # For now, we'll just return an error suggesting a URL-based approach
        # This would need enhancement to support Anthropic's format properly

        return {
            "success": False,
            "error": (
                "Anthropic vision API requires base64-encoded images. "
                "Please use Fireworks or OpenAI for URL-based image analysis."
            ),
            "content": None,
        }
