"""Tests for command history navigation in Textual chat app."""

from swecli.ui_textual.chat_app import ChatTextArea, ConversationLog, create_chat_app


def test_history_navigation_roundtrip() -> None:
    """Ensure history navigation cycles through entries and restores current input."""

    app = create_chat_app()

    # Attach minimal widget instances needed by the history actions.
    app.conversation = ConversationLog()
    app.input_field = ChatTextArea()

    # Seed history via the MessageHistory API
    app._history.record("first")
    app._history.record("second")

    # Start with empty field so we can verify restoration later.
    app.input_field.load_text("")

    app.action_history_up()
    assert app.input_field.text == "second"

    app.action_history_up()
    assert app.input_field.text == "first"

    app.action_history_down()
    assert app.input_field.text == "second"

    app.action_history_down()
    assert app.input_field.text == ""
