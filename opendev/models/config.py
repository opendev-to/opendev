"""Configuration models."""

import re
from typing import Any, Optional

from pydantic import BaseModel, Field, field_validator

from opendev.core.paths import APP_DIR_NAME


class ToolPermission(BaseModel):
    """Permission settings for a specific tool."""

    enabled: bool = True
    always_allow: bool = False
    deny_patterns: list[str] = Field(default_factory=list)
    compiled_patterns: list[re.Pattern[str]] = Field(default_factory=list, exclude=True)

    def model_post_init(self, __context: Any) -> None:
        """Compile regex patterns after initialization."""
        self.compiled_patterns = [re.compile(pattern) for pattern in self.deny_patterns]

    def is_allowed(self, target: str) -> bool:
        """Check if a target (file path, command, etc.) is allowed."""
        if not self.enabled:
            return False
        if self.always_allow:
            return True
        return not any(pattern.match(target) for pattern in self.compiled_patterns)


class PermissionConfig(BaseModel):
    """Global permission configuration."""

    file_write: ToolPermission = Field(default_factory=ToolPermission)
    file_read: ToolPermission = Field(default_factory=ToolPermission)
    bash: ToolPermission = Field(
        default_factory=lambda: ToolPermission(
            enabled=True,  # Enabled for development
            always_allow=False,
            deny_patterns=["rm -rf /", "sudo rm -rf /*", "chmod -R 777 /*"],
        )
    )
    git: ToolPermission = Field(default_factory=ToolPermission)
    web_fetch: ToolPermission = Field(default_factory=ToolPermission)


class AutoModeConfig(BaseModel):
    """Auto mode configuration."""

    enabled: bool = False
    max_operations: int = 10  # Max operations before requiring approval
    require_confirmation_after: int = 5  # Ask for confirmation after N operations
    dangerous_operations_require_approval: bool = True


class OperationConfig(BaseModel):
    """Operation-specific settings."""

    show_diffs: bool = True
    backup_before_edit: bool = True
    max_file_size: int = 1_000_000  # 1MB max file size
    allowed_extensions: list[str] = Field(default_factory=list)  # Empty = all allowed


class PlaybookScoringWeights(BaseModel):
    """Scoring weights for ACE playbook bullet selection."""

    effectiveness: float = Field(default=0.5, ge=0.0, le=1.0)
    recency: float = Field(default=0.3, ge=0.0, le=1.0)
    semantic: float = Field(default=0.2, ge=0.0, le=1.0)

    @field_validator("effectiveness", "recency", "semantic")
    @classmethod
    def validate_weight(cls, v: float) -> float:
        """Ensure weights are between 0 and 1."""
        if not 0.0 <= v <= 1.0:
            raise ValueError("Weight must be between 0.0 and 1.0")
        return v

    def to_dict(self) -> dict[str, float]:
        """Convert to dictionary format for BulletSelector."""
        return {
            "effectiveness": self.effectiveness,
            "recency": self.recency,
            "semantic": self.semantic,
        }


class PlaybookConfig(BaseModel):
    """ACE playbook configuration."""

    max_strategies: int = Field(default=30, ge=1)
    use_selection: bool = True
    embedding_model: str = "text-embedding-3-small"
    embedding_provider: str = "openai"
    scoring_weights: PlaybookScoringWeights = Field(default_factory=PlaybookScoringWeights)
    cache_embeddings: bool = True  # Phase 4: Enable embedding persistence
    cache_file: Optional[str] = None  # Path to embedding cache file (None = session-based default)


class ModelVariant(BaseModel):
    """A named model configuration variant."""

    name: str
    model: str
    provider: str
    temperature: float = 0.6
    max_tokens: int = 16384
    description: str = ""


class AppConfig(BaseModel):
    """Application configuration."""

    model_config = {"protected_namespaces": ()}

    # AI Provider settings - Three model system
    # Normal model: For standard coding tasks
    model_provider: str = "fireworks"
    model: str = "accounts/fireworks/models/kimi-k2-instruct-0905"

    # Thinking model: For complex reasoning tasks (optional, falls back to normal if not set)
    model_thinking: Optional[str] = None
    model_thinking_provider: Optional[str] = None

    # Vision/Multi-modal model: For image processing tasks (optional, falls back to normal if not set)
    model_vlm: Optional[str] = None
    model_vlm_provider: Optional[str] = None

    # Critique model: For self-critique of reasoning (optional, falls back to thinking model if not set)
    model_critique: Optional[str] = None
    model_critique_provider: Optional[str] = None

    # Compact model: For context compaction summaries (optional, falls back to normal if not set)
    model_compact: Optional[str] = None
    model_compact_provider: Optional[str] = None

    api_key: Optional[str] = None
    api_base_url: Optional[str] = None
    max_tokens: int = 16384
    temperature: float = 0.6

    # Session settings
    auto_save_interval: int = 5  # Save every N turns
    max_context_tokens: int = 100000  # Dynamically set from model context_length (80%)

    # UI settings
    verbose: bool = False
    debug_logging: bool = False  # Show [QUERY], [REACT], [LLM] debug messages
    color_scheme: str = "monokai"
    show_token_count: bool = True
    enable_sound: bool = True

    # Permissions
    permissions: PermissionConfig = Field(default_factory=PermissionConfig)

    # Phase 2: Operation settings
    enable_bash: bool = True  # Enable bash execution for development
    bash_timeout: int = 30  # Timeout in seconds for bash commands
    auto_mode: AutoModeConfig = Field(default_factory=AutoModeConfig)
    operation: OperationConfig = Field(default_factory=OperationConfig)
    max_undo_history: int = 50  # Maximum operations to track for undo

    # Session intelligence
    topic_detection: bool = True

    # ACE Playbook settings
    playbook: PlaybookConfig = Field(default_factory=PlaybookConfig)

    # Plan mode configuration
    plan_mode_workflow: str = "5-phase"  # "5-phase" or "iterative"
    plan_mode_explore_agent_count: int = 3
    plan_mode_plan_agent_count: int = 1
    plan_mode_explore_variant: str = "enabled"  # "enabled" or "disabled"

    # Custom instructions (accumulated across config levels)
    instructions: Optional[str] = None

    # Model variants
    model_variants: dict[str, ModelVariant] = Field(default_factory=dict)

    # Paths - using APP_DIR_NAME constant for consistency
    opendev_dir: str = f"~/{APP_DIR_NAME}"
    session_dir: str = f"~/{APP_DIR_NAME}/sessions"
    log_dir: str = f"~/{APP_DIR_NAME}/logs"
    command_dir: str = f"{APP_DIR_NAME}/commands"

    @field_validator("model_provider")
    @classmethod
    def validate_provider(cls, v: str) -> str:
        """Validate model provider."""
        supported = [
            "fireworks",
            "anthropic",
            "openai",
            "azure",
            "groq",
            "mistral",
            "deepinfra",
            "openrouter",
        ]
        if v not in supported:
            raise ValueError(
                f"Unsupported provider '{v}'. Supported providers: {', '.join(supported)}"
            )
        return v

    def get_api_key(self) -> str:
        """Get API key from config or environment."""
        import os

        if self.api_key:
            return self.api_key

        provider_env_vars = {
            "fireworks": "FIREWORKS_API_KEY",
            "anthropic": "ANTHROPIC_API_KEY",
            "openai": "OPENAI_API_KEY",
            "azure": "AZURE_OPENAI_API_KEY",
            "groq": "GROQ_API_KEY",
            "mistral": "MISTRAL_API_KEY",
            "deepinfra": "DEEPINFRA_API_KEY",
            "openrouter": "OPENROUTER_API_KEY",
        }
        env_var = provider_env_vars.get(self.model_provider, "OPENAI_API_KEY")
        key = os.getenv(env_var)

        if not key:
            raise ValueError(
                f"No API key found. Set {self.model_provider.upper()}_API_KEY environment variable"
            )
        return key

    def get_model_info(self):
        """Get model information from the registry.

        Returns:
            ModelInfo object or None if model not found
        """
        from opendev.config import get_model_registry

        registry = get_model_registry()
        result = registry.find_model_by_id(self.model)
        if result:
            return result[2]  # Return ModelInfo
        return None

    def get_provider_info(self):
        """Get provider information from the registry.

        Returns:
            ProviderInfo object or None if provider not found
        """
        from opendev.config import get_model_registry

        registry = get_model_registry()
        return registry.get_provider(self.model_provider)

    def get_thinking_model_info(self):
        """Get thinking model information, fallback to normal if not set.

        Returns:
            Tuple of (provider_id, model_id, ModelInfo) or None
        """
        from opendev.config import get_model_registry

        registry = get_model_registry()

        # Use thinking model if configured
        if self.model_thinking and self.model_thinking_provider:
            result = registry.find_model_by_id(self.model_thinking)
            if result:
                return result

        # Fallback to normal model
        result = registry.find_model_by_id(self.model)
        return result

    def get_vlm_model_info(self):
        """Get VLM model information, fallback to normal if not set.

        Returns:
            Tuple of (provider_id, model_id, ModelInfo) or None
        """
        from opendev.config import get_model_registry

        registry = get_model_registry()

        # Use VLM model if configured
        if self.model_vlm and self.model_vlm_provider:
            result = registry.find_model_by_id(self.model_vlm)
            if result:
                return result

        # Fallback to normal model if it has vision capability
        result = registry.find_model_by_id(self.model)
        if result:
            _, _, model_info = result
            if "vision" in model_info.capabilities:
                return result

        # No vision model available
        return None

    def get_critique_model_info(self):
        """Get critique model information, fallback to thinking model if not set.

        Returns:
            Tuple of (provider_id, model_id, ModelInfo) or None
        """
        from opendev.config import get_model_registry

        registry = get_model_registry()

        # Use critique model if configured
        if self.model_critique and self.model_critique_provider:
            result = registry.find_model_by_id(self.model_critique)
            if result:
                return result

        # Fallback to thinking model if configured
        if self.model_thinking and self.model_thinking_provider:
            result = registry.find_model_by_id(self.model_thinking)
            if result:
                return result

        # Fallback to normal model
        result = registry.find_model_by_id(self.model)
        return result

    def get_compact_model_info(self):
        """Get compact model information, fallback to normal model if not set.

        Returns:
            Tuple of (provider_id, model_id, ModelInfo) or None
        """
        from opendev.config import get_model_registry

        registry = get_model_registry()

        # Use compact model if configured
        if self.model_compact and self.model_compact_provider:
            result = registry.find_model_by_id(self.model_compact)
            if result:
                return result

        # Fallback to normal model
        result = registry.find_model_by_id(self.model)
        return result

    def should_use_provider_for_all(self, provider_id: str) -> bool:
        """Check if provider supports all capabilities in all models (OpenAI/Anthropic).

        For OpenAI and Anthropic, all models support text, vision, and code.
        For other providers (Fireworks), models have specific capabilities.

        Args:
            provider_id: Provider ID to check

        Returns:
            True if all models from this provider support all capabilities
        """
        return provider_id in ["openai", "anthropic"]

    def get_variant(self, name: str) -> ModelVariant | None:
        """Get a named model variant configuration."""
        return self.model_variants.get(name)

    def apply_variant(self, name: str) -> bool:
        """Apply a named model variant to the current config.

        Returns True if variant was found and applied.
        """
        variant = self.model_variants.get(name)
        if not variant:
            return False
        self.model = variant.model
        self.model_provider = variant.provider
        self.temperature = variant.temperature
        self.max_tokens = variant.max_tokens
        return True
