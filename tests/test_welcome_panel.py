"""Tests for AnimatedWelcomePanel widget."""

from swecli.ui_textual.widgets.welcome_panel import (
    AnimatedWelcomePanel,
    hsl_to_ansi256,
)
from swecli.core.runtime import OperationMode


class TestHslToAnsi256:
    """Test HSL to ANSI-256 color conversion."""

    def test_red_hue(self):
        """Red (hue=0) should produce a red-ish color."""
        color = hsl_to_ansi256(0, 0.8, 0.6)
        assert 16 <= color <= 231  # Color cube range

    def test_green_hue(self):
        """Green (hue=120) should produce a green-ish color."""
        color = hsl_to_ansi256(120, 0.8, 0.6)
        assert 16 <= color <= 231

    def test_blue_hue(self):
        """Blue (hue=240) should produce a blue-ish color."""
        color = hsl_to_ansi256(240, 0.8, 0.6)
        assert 16 <= color <= 231

    def test_hue_wrapping(self):
        """Hue should wrap around at 360."""
        color_0 = hsl_to_ansi256(0, 0.7, 0.6)
        color_360 = hsl_to_ansi256(360, 0.7, 0.6)
        assert color_0 == color_360

    def test_full_rainbow(self):
        """All rainbow hues should produce valid colors."""
        for hue in range(0, 360, 30):
            color = hsl_to_ansi256(hue)
            assert 16 <= color <= 231, f"Hue {hue} produced invalid color {color}"


class TestAnimatedWelcomePanel:
    """Test AnimatedWelcomePanel widget."""

    def test_creation_default(self):
        """Panel can be created with defaults."""
        panel = AnimatedWelcomePanel()
        assert panel is not None
        assert panel._current_mode == OperationMode.NORMAL
        assert panel._username is not None

    def test_creation_with_mode(self):
        """Panel respects operation mode."""
        panel = AnimatedWelcomePanel(current_mode=OperationMode.PLAN)
        assert panel._current_mode == OperationMode.PLAN
        # Check mode appears in content (multi-line list)
        content_text = "\n".join(panel._content_lines)
        assert "PLAN" in content_text

    def test_creation_with_username(self):
        """Panel stores username (no longer shown in horizontal bar)."""
        panel = AnimatedWelcomePanel(username="TestUser")
        assert panel._username == "TestUser"
        # Username is stored but not shown in the compact horizontal bar

    def test_content_generation(self):
        """Content is generated as multi-line list."""
        panel = AnimatedWelcomePanel()
        content = panel._content_lines

        # Check for expected content in multi-line format
        assert isinstance(content, list)
        assert len(content) > 0
        content_text = "\n".join(content)
        assert "O P E N D E V" in content_text  # ASCII art title with spaces
        assert "/help" in content_text
        assert "Shift+Tab" in content_text

    def test_gradient_offset_reactive(self):
        """Gradient offset is reactive property."""
        panel = AnimatedWelcomePanel()
        assert panel.gradient_offset == 0

        panel.gradient_offset = 180
        assert panel.gradient_offset == 180

    def test_fade_progress_reactive(self):
        """Fade progress is reactive property."""
        panel = AnimatedWelcomePanel()
        assert panel.fade_progress == 1.0

        panel.fade_progress = 0.5
        assert panel.fade_progress == 0.5

    def test_apply_gradient_preserves_whitespace(self):
        """Gradient coloring preserves whitespace."""
        panel = AnimatedWelcomePanel()
        text = "  hello  world  "
        result = panel._apply_gradient(text)

        # Result should have same length (Rich Text)
        assert len(result.plain) == len(text)

    def test_apply_gradient_colors_characters(self):
        """Gradient applies colors to non-whitespace."""
        panel = AnimatedWelcomePanel()
        text = "abc"
        result = panel._apply_gradient(text)

        # Should produce Rich Text with style spans
        assert result.plain == "abc"
        # Each character should have a style
        spans = result._spans
        assert len(spans) >= 1

    def test_fade_out_initial_state(self):
        """Panel starts with fading disabled."""
        panel = AnimatedWelcomePanel()
        assert not panel._is_fading
        assert panel.fade_progress == 1.0

    def test_do_fade_decrements_progress(self):
        """_do_fade decrements fade_progress."""
        panel = AnimatedWelcomePanel()
        panel._is_fading = True
        initial = panel.fade_progress

        panel._do_fade()
        assert panel.fade_progress < initial

    def test_get_version(self):
        """Version retrieval works."""
        version = AnimatedWelcomePanel.get_version()
        assert version.startswith("v")


class TestWelcomePanelSessionResumption:
    """Test welcome panel behavior on session resumption."""

    def test_chat_app_new_session_defaults(self):
        """New session should have welcome_visible=True by default."""
        from swecli.ui_textual.chat_app import SWECLIChatApp

        app = SWECLIChatApp(is_resumed_session=False)
        assert app._is_resumed_session is False
        assert app._welcome_visible is True

    def test_chat_app_resumed_session_flags(self):
        """Resumed session should have welcome_visible=False."""
        from swecli.ui_textual.chat_app import SWECLIChatApp

        app = SWECLIChatApp(is_resumed_session=True)
        assert app._is_resumed_session is True
        assert app._welcome_visible is False

    def test_create_chat_app_accepts_resumed_flag(self):
        """create_chat_app should accept is_resumed_session parameter."""
        from swecli.ui_textual.chat_app import create_chat_app

        # New session
        app_new = create_chat_app(is_resumed_session=False)
        assert app_new._is_resumed_session is False
        assert app_new._welcome_visible is True

        # Resumed session
        app_resumed = create_chat_app(is_resumed_session=True)
        assert app_resumed._is_resumed_session is True
        assert app_resumed._welcome_visible is False


class TestAnimatedWelcomePanelIntegration:
    """Integration tests for AnimatedWelcomePanel."""

    def test_import_from_widgets(self):
        """Can import from widgets package."""
        from swecli.ui_textual.widgets import AnimatedWelcomePanel as Imported

        assert Imported is AnimatedWelcomePanel

    def test_widget_has_default_css(self):
        """Widget has CSS defined."""
        panel = AnimatedWelcomePanel()
        assert hasattr(panel, "DEFAULT_CSS")
        assert "AnimatedWelcomePanel" in panel.DEFAULT_CSS

