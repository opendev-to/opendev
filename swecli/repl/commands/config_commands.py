"""Configuration commands for REPL."""

import asyncio
from typing import TYPE_CHECKING, Optional

from rich.console import Console

from swecli.config import get_model_registry
from swecli.repl.commands.base import CommandHandler, CommandResult

if TYPE_CHECKING:
    from swecli.core.runtime import ConfigManager


class ConfigCommands(CommandHandler):
    """Handler for configuration-related commands: /models."""

    def __init__(
        self,
        console: Console,
        config_manager: "ConfigManager",
        chat_app=None,
    ):
        """Initialize config commands handler.

        Args:
            console: Rich console for output
            config_manager: Config manager instance
            chat_app: Chat application instance (for interactive modal)
        """
        super().__init__(console)
        self.config_manager = config_manager
        self.chat_app = chat_app

    def handle(self, args: str) -> CommandResult:
        """Handle config command (not used, individual methods called directly)."""
        raise NotImplementedError("Use specific methods: show_model_selector()")

    async def show_model_selector_async(self) -> CommandResult:
        """Show interactive model selector modal with three categories (async version).

        Workflow:
        1. Show category selector (Normal, Thinking, VLM, Finish)
        2. If user selects "Finish", show summary and exit
        3. If user selects a category, show filtered model selector
        4. If user selects "back" in model selector, return to category selector
        5. If user selects a model, configure it and return to category selector
        6. Loop continues until user selects "Finish"

        Returns:
            CommandResult indicating success or failure
        """
        if not self.chat_app:
            self.print_error("Interactive model selector not available in this mode")
            return CommandResult(success=False, message="Chat app not available")

        # Import here to avoid circular imports

        # Loop to allow configuring multiple models
        while True:
            # Show category selector (Normal / Thinking / VLM / Finish)
            selected_category = await self._show_category_selector()

            if not selected_category:
                # User cancelled - silently return without printing message
                return CommandResult(success=False, message="Selection cancelled")

            # Check if user selected "Finish"
            if selected_category == "finish":
                # Show final summary and exit
                if self.chat_app:
                    self._show_model_config_summary()
                return CommandResult(success=True, message="Model configuration complete")

            # Show the modal with filtered models for selected category
            selected, item = await self.chat_app.model_selector_modal_manager.show_model_selector(
                selection_mode=selected_category
            )

            if not selected or not item:
                # User cancelled - silently return without printing message
                return CommandResult(success=False, message="Selection cancelled")

            # Check if user selected "back" button
            if item.get("type") == "back":
                # Go back to category selection (loop continues)
                continue

            # Extract selection info
            provider_id = item["provider_id"]
            model_id = item["model_id"]
            mode = item.get("mode", "normal")

            # Switch to the selected model for the appropriate slot
            result = self._switch_to_model(provider_id, model_id, mode)

            # If successful, continue loop to let user configure more models
            # Don't exit here - return to category selector
            if not result.success:
                # If switch failed, exit with error
                return result

            # Success - loop continues, user can configure more models

    async def _show_category_selector(self) -> Optional[str]:
        """Show category selector to choose which model slot to configure.

        Returns:
            Selected category ("normal", "thinking", "vlm") or None if cancelled
        """
        from swecli.ui_textual.components.category_selector_message import (
            create_category_selector_message,
            get_category_items
        )

        # Check if normal model is configured
        config = self.config_manager.get_config()
        normal_configured = bool(config.model and config.model_provider)

        # Reset state
        self.chat_app.model_selector_modal_manager.reset_state()
        self.chat_app.model_selector_modal_manager._selector_mode = True
        self.chat_app.model_selector_modal_manager._is_category_selector = True  # Mark as category selector
        self.chat_app.model_selector_modal_manager._normal_configured = normal_configured

        # Get category items (with disabled status based on normal_configured)
        category_items = get_category_items(normal_configured)
        self.chat_app.model_selector_modal_manager._selector_items = category_items
        self.chat_app.model_selector_modal_manager._selector_selected_index = 0

        # Unlock input
        self.chat_app._input_locked = False
        self.chat_app.input_buffer.text = ""
        self.chat_app.input_buffer.cursor_position = 0

        # Show category selector
        selector_msg = create_category_selector_message(0, normal_configured)
        self.chat_app.conversation.add_assistant_message(selector_msg)
        self.chat_app._update_conversation_buffer()
        self.chat_app.model_selector_modal_manager._position_conversation_for_selector()
        self.chat_app.app.invalidate()

        # Wait for selection
        try:
            result = await self.chat_app.model_selector_modal_manager._wait_for_user_selection()
        finally:
            self.chat_app.model_selector_modal_manager._selector_mode = False
            self.chat_app._input_locked = False

        # Remove selector message
        if self.chat_app.conversation.messages:
            self.chat_app.conversation.messages.pop()
            self.chat_app._update_conversation_buffer()

        if result["selected"] and result["item"]:
            return result["item"]["category"]

        return None

    def show_model_selector(self) -> CommandResult:
        """Show interactive model selector modal (sync wrapper).

        Returns:
            CommandResult indicating success or failure
        """
        # Run the async version
        loop = asyncio.get_event_loop()
        return loop.run_until_complete(self.show_model_selector_async())

    def _switch_to_model(self, provider_id: str, model_id: str, mode: str = "normal") -> CommandResult:
        """Switch to a specific model for a specific slot.

        Args:
            provider_id: Provider ID
            model_id: Model ID
            mode: Model slot ("normal", "thinking", "vlm")

        Returns:
            CommandResult indicating success or failure
        """
        registry = get_model_registry()
        config = self.config_manager.get_config()

        # Find the model
        result = registry.find_model_by_id(model_id)
        if not result:
            self.print_error(f"Model '{model_id}' not found")
            return CommandResult(success=False, message="Model not found")

        found_provider_id, _, model_info = result

        # Verify provider matches
        if found_provider_id != provider_id:
            self.print_error(f"Model provider mismatch")
            return CommandResult(success=False, message="Provider mismatch")

        # Check API key for new provider (silently - user will get error when they try to use it)
        if provider_id != config.model_provider:
            provider_info = registry.get_provider(provider_id)
            env_var = provider_info.api_key_env
            # Skip warning - let them discover missing API key when they try to use it

        # Update configuration based on mode
        mode_names = {
            "normal": "Normal",
            "thinking": "Thinking",
            "vlm": "Vision/Multi-modal",
            "critique": "Critique",
            "compact": "Compact",
        }

        if mode == "normal":
            config.model_provider = provider_id
            config.model = model_info.id
            # Recalculate max_context_tokens based on normal model
            config.max_context_tokens = int(model_info.context_length * 0.8)

            # Auto-populate thinking/vision slots based on capabilities (only if not set)
            if "reasoning" in model_info.capabilities and not config.model_thinking:
                config.model_thinking_provider = provider_id
                config.model_thinking = model_info.id

            if "vision" in model_info.capabilities and not config.model_vlm:
                config.model_vlm_provider = provider_id
                config.model_vlm = model_info.id

            # Auto-populate compact slot (any model can do summarization)
            if not config.model_compact:
                config.model_compact_provider = provider_id
                config.model_compact = model_info.id

        elif mode == "thinking":
            config.model_thinking_provider = provider_id
            config.model_thinking = model_info.id

        elif mode == "vlm":
            config.model_vlm_provider = provider_id
            config.model_vlm = model_info.id

        elif mode == "critique":
            config.model_critique_provider = provider_id
            config.model_critique = model_info.id

        elif mode == "compact":
            config.model_compact_provider = provider_id
            config.model_compact = model_info.id

        # Save configuration
        try:
            self.config_manager.save_config(config, global_config=True)

            # Refresh the UI (footer will show new model)
            if self.chat_app:
                refresher = getattr(self.chat_app, "refresh", None)
                if callable(refresher):
                    refresher()

            mode_name = mode_names.get(mode, mode)
            return CommandResult(
                success=True,
                message=f"Switched {mode_name} model to {model_info.name}",
                data={"model": model_info, "provider": provider_id, "mode": mode}
            )

        except Exception as e:
            self.print_error(f"Failed to save configuration: {e}")
            return CommandResult(success=False, message=str(e))

    def _show_model_config_summary(self) -> None:
        """Show a concise summary of currently configured models in the conversation."""
        config = self.config_manager.get_config()

        # Build summary lines in tool result style
        lines = ["⏺ Models configured"]

        # Normal model (always show)
        if config.model:
            normal_name = config.model.split('/')[-1]
            provider_name = config.model_provider.capitalize()
            lines.append(f"  ⎿  Normal: {provider_name}/{normal_name}")

        # Thinking model (if configured)
        if config.model_thinking:
            thinking_name = config.model_thinking.split('/')[-1]
            thinking_provider = config.model_thinking_provider.capitalize() if config.model_thinking_provider else "Unknown"
            lines.append(f"  ⎿  Thinking: {thinking_provider}/{thinking_name}")
        else:
            lines.append(f"  ⎿  Thinking: Not set (falls back to Normal)")

        # VLM model (if configured)
        if config.model_vlm:
            vlm_name = config.model_vlm.split('/')[-1]
            vlm_provider = config.model_vlm_provider.capitalize() if config.model_vlm_provider else "Unknown"
            lines.append(f"  ⎿  Vision: {vlm_provider}/{vlm_name}")
        else:
            lines.append(f"  ⎿  Vision: Not set (vision tasks unavailable)")

        # Critique model (if configured)
        if config.model_critique:
            critique_name = config.model_critique.split('/')[-1]
            critique_provider = config.model_critique_provider.capitalize() if config.model_critique_provider else "Unknown"
            lines.append(f"  ⎿  Critique: {critique_provider}/{critique_name}")
        else:
            lines.append(f"  ⎿  Critique: Not set (falls back to Thinking)")

        # Compact model (if configured)
        if config.model_compact:
            compact_name = config.model_compact.split('/')[-1]
            compact_provider = config.model_compact_provider.capitalize() if config.model_compact_provider else "Unknown"
            lines.append(f"  ⎿  Compact: {compact_provider}/{compact_name}")
        else:
            lines.append(f"  ⎿  Compact: Not set (falls back to Normal)")

        # Create the message in tool result format
        full_message = "\n".join(lines)

        # Add to conversation as assistant message
        self.chat_app.conversation.add_assistant_message(full_message)
        self.chat_app._update_conversation_buffer()
        self.chat_app.app.invalidate()
