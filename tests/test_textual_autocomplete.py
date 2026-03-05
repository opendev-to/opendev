"""Autocomplete behavior for the Textual chat input."""

from pathlib import Path
from unittest.mock import patch

from swecli.ui_textual.autocomplete import SwecliCompleter
from swecli.ui_textual.widgets.chat_text_area import ChatTextArea


def _build_area(tmp_path: Path) -> ChatTextArea:
    """Create a ChatTextArea wired to a swecli completer rooted at tmp_path."""

    completer = SwecliCompleter(tmp_path)
    area = ChatTextArea(completer=completer)
    # Bypass debounce timer: replace update_suggestion with direct _do_autocomplete
    area.update_suggestion = area._do_autocomplete
    return area


def test_slash_command_suggestion(tmp_path) -> None:
    """Slash commands surface inline suggestions for the leading match."""

    area = _build_area(tmp_path)
    area.text = "/"
    area.cursor_location = (0, len(area.document[0]))
    area.update_suggestion()

    assert area.suggestion == "help"
    assert any(entry[0].startswith("/help") for entry in area._completion_entries)


def test_file_mention_suggestion(tmp_path) -> None:
    """@ mentions suggest files (and directories) relative to the working directory."""

    (tmp_path / "foo.txt").write_text("hello")
    (tmp_path / "src").mkdir()
    (tmp_path / "src" / "main.py").write_text("print('hi')")

    area = _build_area(tmp_path)
    area.text = "@f"
    area.cursor_location = (0, len(area.document[0]))
    area.update_suggestion()

    # The inline suggestion should propose the remainder of the file path.
    assert area.suggestion.endswith("oo.txt")
    assert any("foo.txt" in entry[0] for entry in area._completion_entries)

    # Folder mentions should be included with a trailing slash.
    area.text = "@s"
    area.cursor_location = (0, len(area.document[0]))
    area.update_suggestion()

    assert any("src/" in entry[0] for entry in area._completion_entries)


def test_autocomplete_selection_cycle(tmp_path) -> None:
    """Arrow keys rotate through available completions."""

    area = _build_area(tmp_path)
    area.text = "/"
    area.cursor_location = (0, len(area.document[0]))
    area.update_suggestion()

    initial = area.suggestion
    area._move_completion_selection(1)

    assert area._highlight_index == 1
    # Suggestion should reflect the newly highlighted completion.
    assert area.suggestion != initial


def test_accept_completion_updates_text(tmp_path) -> None:
    """Accepting a completion replaces the current token."""

    area = _build_area(tmp_path)
    area.text = "/he"
    area.cursor_location = (0, len(area.document[0]))
    area.update_suggestion()

    accepted = area._accept_completion_selection()
    assert accepted
    assert area.text == "/help"
