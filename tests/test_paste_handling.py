"""Tests for paste handling inside ChatTextArea."""

from unittest.mock import patch, PropertyMock
from textual.events import Paste

from swecli.ui_textual.chat_app import ChatTextArea


def _make_textarea(threshold: int = 10) -> ChatTextArea:
    """Create a ChatTextArea with debounce and app access bypassed."""
    textarea = ChatTextArea(paste_threshold=threshold)
    textarea.load_text("")
    # Bypass debounce timer (requires running Textual event loop)
    textarea.update_suggestion = lambda: None
    return textarea


def test_small_paste_inserts_directly() -> None:
    textarea = _make_textarea()

    # Patch the Textual app property to avoid NoActiveAppError
    with patch.object(type(textarea), "app", new_callable=PropertyMock, return_value=None):
        textarea.on_paste(Paste("hello"))

    assert textarea.text == "hello"


def test_large_paste_uses_placeholder_and_cache() -> None:
    textarea = _make_textarea()

    large_content = "x" * 20
    textarea.on_paste(Paste(large_content))

    tokenized = textarea.text
    assert tokenized != large_content
    assert "PASTE" in tokenized
    assert textarea.resolve_large_pastes(tokenized) == large_content

    textarea.clear_large_pastes()
    assert textarea.resolve_large_pastes(tokenized) == tokenized
