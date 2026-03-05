"""Toolbar component for REPL bottom status bar."""

from typing import TYPE_CHECKING

from prompt_toolkit.formatted_text import FormattedText

from swecli.ui_textual.style_tokens import PT_GREEN, PT_GREY, PT_ORANGE, PT_PURPLE

if TYPE_CHECKING:
    from swecli.core.runtime import ModeManager
    from swecli.core.context_engineering.history import SessionManager
    from swecli.models.config import Config


class Toolbar:
    """Generates bottom toolbar showing mode, shortcuts, and context."""

    def __init__(
        self,
        mode_manager: "ModeManager",
        session_manager: "SessionManager",
        config: "Config",
    ):
        """Initialize toolbar.

        Args:
            mode_manager: Mode manager for current mode
            session_manager: Session manager for token tracking
            config: Configuration for token limits
        """
        self.mode_manager = mode_manager
        self.session_manager = session_manager
        self.config = config

    def build_tokens(self) -> FormattedText:
        """Generate bottom toolbar text showing mode and shortcuts.

        Returns:
            FormattedText for bottom toolbar
        """
        from swecli.core.runtime import OperationMode

        mode = self.mode_manager.current_mode.value.upper()
        limit = self.config.max_context_tokens or 1
        used = (
            self.session_manager.current_session.total_tokens()
            if self.session_manager.current_session
            else 0
        )
        remaining_pct = max(0.0, 100.0 - (used / limit * 100.0))

        mode_style = (
            f'fg:{PT_ORANGE} bold'
            if self.mode_manager.current_mode == OperationMode.NORMAL
            else f'fg:{PT_GREEN} bold'
        )

        # Extract readable model name (last part after /)
        model_name = self.config.model.split('/')[-1] if self.config.model else 'unknown'
        provider_name = self.config.model_provider.capitalize()

        return FormattedText(
            [
                (mode_style, f" {mode} "),
                (
                    f'fg:{PT_GREY}',
                    " • Shift+Tab: Toggle Mode • Ctrl+C: Exit • Context Left: ",
                ),
                (f'fg:{PT_GREY}', f"{remaining_pct:.0f}% "),
                (f'fg:{PT_GREY}', f"• {provider_name}: "),
                (f'fg:{PT_PURPLE}', f"{model_name} "),
            ]
        )
