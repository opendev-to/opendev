#!/usr/bin/env python3
"""Simple test to verify Textual UI behavior without terminal dependency."""

import asyncio
import sys
from pathlib import Path

# Add the project to path
sys.path.insert(0, str(Path(__file__).parent))

from textual.app import App
from textual.containers import Container, Vertical
from textual.widgets import Header, Footer, Static, Button
from swecli.ui_textual.chat_app import ConversationLog, ChatTextArea

class TestApp(App):
    """Simple test app to verify UI rendering."""

    CSS = """
    Screen {
        background: $background;
    }

    #main-container {
        height: 100%;
        layout: vertical;
    }

    #conversation {
        height: 1fr;
        border: solid $accent;
        background: $surface;
        padding: 0 1;
    }

    #button-container {
        height: 3;
        content-align: center middle;
    }
    """

    def compose(self):
        """Create child widgets."""
        yield Header(show_clock=True)

        with Container(id="main-container"):
            yield ConversationLog(id="conversation")

            with Container(id="button-container"):
                yield Button("Test Message", id="test-btn")
                yield Button("Clear", id="clear-btn")

        yield Footer()

    def on_mount(self):
        """Initialize the app on mount."""
        self.title = "OpenDev UI Test"
        self.conversation = self.query_one("#conversation", ConversationLog)

        # Add welcome message
        self.conversation.add_system_message("🧪 UI Test Started")
        self.conversation.add_system_message("Click 'Test Message' to simulate LLM response")

    async def on_button_pressed(self, event):
        """Handle button presses."""
        if event.button.id == "test-btn":
            # Simulate what happens when we get a response
            self.conversation.add_user_message("hello")
            self.conversation.add_assistant_message("Hello! This is a test response. If you can see this, the UI rendering works!")
            self.conversation.add_tool_call("TestTool", "param='test'")
            self.conversation.add_tool_result("Test result: Success!")

        elif event.button.id == "clear-btn":
            self.conversation.clear()
            self.conversation.add_system_message("Conversation cleared")

if __name__ == "__main__":
    print("🧪 Starting Textual UI test...")
    print("If the UI launches, you should see:")
    print("1. A welcome message")
    print("2. A 'Test Message' button")
    print("3. Click the button to simulate LLM responses")
    print("4. If you see colored text and tool calls, the UI works")

    app = TestApp()
    app.run()