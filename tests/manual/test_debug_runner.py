#!/usr/bin/env python3
"""Debug script to trace exactly what happens in the TextualRunner."""

import asyncio
import sys
from pathlib import Path

# Add the project to path
sys.path.insert(0, str(Path(__file__).parent))

from textual.app import App, ComposeResult
from textual.containers import Container, Vertical
from textual.widgets import Header, Footer, Static, Button
from swecli.ui_textual.chat_app import ConversationLog, ChatTextArea
from swecli.ui_textual.runner import TextualRunner
from swecli.models.message import Role

class DebugApp(App):
    """Debug app that shows exactly what happens when we process a query."""

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
        height: 5;
        content-align: center middle;
    }

    #status {
        height: 3;
        background: $surface;
        content-align: center middle;
        color: $text;
        border: solid $accent;
    }
    """

    def compose(self):
        """Create child widgets."""
        yield Header(show_clock=True)

        with Container(id="main-container"):
            yield ConversationLog(id="conversation")

            with Container(id="button-container"):
                yield Button("Test Backend", id="test-backend")
                yield Button("Test Direct Message", id="test-direct")
                yield Button("Clear", id="clear-btn")

            yield Static("Ready to test...", id="status")

        yield Footer()

    def on_mount(self):
        """Initialize the app on mount."""
        self.title = "OpenDev Debug Test"
        self.conversation = self.query_one("#conversation", ConversationLog)
        self.status = self.query_one("#status", Static)

        # Create the runner
        try:
            self.runner = TextualRunner(working_dir=Path.cwd())
            self.status.update("✅ TextualRunner initialized successfully")
            self.conversation.add_system_message("🐛 Debug mode enabled")
            self.conversation.add_system_message(f"Model: {self.runner.config.model_provider}/{self.runner.config.model}")
            self.conversation.add_system_message("Click buttons to test different scenarios")
        except Exception as e:
            self.status.update(f"❌ Failed to initialize runner: {e}")
            self.conversation.add_error(f"Runner initialization failed: {e}")

    async def on_button_pressed(self, event):
        """Handle button presses."""
        if event.button.id == "test-backend":
            await self.test_backend_integration()

        elif event.button.id == "test-direct":
            self.test_direct_message()

        elif event.button.id == "clear-btn":
            self.conversation.clear()
            self.status.update("Conversation cleared")

    async def test_backend_integration(self):
        """Test the actual backend integration."""
        self.status.update("Testing backend integration...")
        self.conversation.add_system_message("🔄 Testing backend integration")

        try:
            # Simulate what the real UI does
            test_message = "hello"
            self.conversation.add_user_message(test_message)

            # This is what the real TextualRunner does
            new_messages = await asyncio.to_thread(self.runner._run_query, test_message)

            self.conversation.add_system_message(f"[DEBUG] Backend returned {len(new_messages)} new messages")

            if new_messages:
                for i, msg in enumerate(new_messages, 1):
                    self.conversation.add_system_message(f"[DEBUG] Message {i}: [{msg.role.value}] {msg.content[:50]}...")

                    # Now render it like the real app does
                    if msg.role == Role.ASSISTANT:
                        self.conversation.add_assistant_message(msg.content)
                    elif msg.role == Role.SYSTEM:
                        self.conversation.add_system_message(msg.content)
                    else:
                        self.conversation.add_system_message(msg.content)

                self.status.update(f"✅ Backend working! Got {len(new_messages)} messages")
            else:
                self.conversation.add_error("[DEBUG] No messages returned from backend")
                self.status.update("⚠️ Backend returned no messages")

        except Exception as e:
            self.conversation.add_error(f"Backend test failed: {e}")
            import traceback
            error_detail = traceback.format_exc()
            self.conversation.add_system_message(f"[DEBUG] Full error:\n{error_detail}")
            self.status.update("❌ Backend test failed")

    def test_direct_message(self):
        """Test direct message rendering without backend."""
        self.status.update("Testing direct message rendering...")
        self.conversation.add_user_message("test message")
        self.conversation.add_assistant_message("This is a test response from the UI itself (no backend).")
        self.conversation.add_tool_call("TestTool", "param='direct'")
        self.conversation.add_tool_result("Direct test result: SUCCESS!")
        self.status.update("✅ Direct rendering test complete")

if __name__ == "__main__":
    print("🐛 Starting OpenDev debug test...")
    print("This will help identify if the issue is:")
    print("1. Backend integration (click 'Test Backend')")
    print("2. UI message rendering (click 'Test Direct Message')")
    print("\nIf 'Test Direct Message' works but 'Test Backend' doesn't,")
    print("the issue is in the backend integration.")

    app = DebugApp()
    app.run()