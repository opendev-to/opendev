# Textual UI for SWE-CLI - Proof of Concept

This directory contains the new Textual-based user interface for SWE-CLI, designed to replace the prompt_toolkit implementation.

## Why Textual?

The existing prompt_toolkit UI has issues with full-screen terminal display. Textual is a modern Python TUI framework (similar to Bubble Tea in Go, which Crush uses) that provides:

- **Native full-screen support** - Terminal takeover works out of the box
- **Rich integration** - Seamless text formatting and syntax highlighting
- **Widget-based architecture** - Cleaner, more maintainable code
- **Reactive updates** - Automatic UI updates on data changes
- **CSS-like styling** - Easy theming and customization
- **Production-ready** - Used by Posting, Toolong, Dolphie, and other apps

## POC Status (Phase 1)

✅ **Completed Features:**
- Full-screen terminal display
- Scrollable conversation log (RichLog)
- Input area with submit on Enter
- Color-coded messages (user, assistant, system, tools)
- Tool call and result formatting
- Keyboard shortcuts (Ctrl+C, Ctrl+L, ESC)
- Status bar with mode/context/model info
- Command system (/help, /clear, /demo, /quit)
- Welcome screen
- Proper styling with TCSS

## Directory Structure

```
swecli/ui_textual/
├── __init__.py
├── README.md                 # This file
├── chat_app.py               # Main Textual application
├── controllers/              # Interactive controllers (model picker, spinner, etc.)
│   ├── __init__.py
│   ├── approval_prompt_controller.py
│   ├── autocomplete_popup_controller.py
│   ├── command_router.py
│   ├── message_controller.py
│   ├── model_picker_controller.py
│   └── spinner_controller.py
├── managers/                 # State/data managers used by the app
│   ├── __init__.py
│   ├── console_buffer_manager.py
│   ├── message_history.py
│   └── tool_summary_manager.py
├── renderers/                # Rich/Textual rendering helpers
│   ├── __init__.py
│   ├── markdown.py
│   └── welcome_panel.py
├── screens/                  # Modal screens and overlays
│   ├── __init__.py
│   └── command_approval_modal.py
├── styles/                   # Textual CSS
│   └── chat.tcss
├── widgets/                  # Custom Textual widgets
│   ├── __init__.py
│   ├── chat_text_area.py
│   ├── conversation_log.py
│   └── status_bar.py
├── runner.py                 # Entry point used by `swecli`
├── ui_callback.py            # Real-time tool display callback
└── welcome_panel.py          # Rich welcome panel renderer
```

## Testing the POC

### Quick Test

```bash
# From the swe-cli root directory
python test_textual_ui.py
```

**Expected behavior:**
- The terminal should immediately switch to alternate screen buffer
- All previous terminal content should disappear (like when you open Vim)
- You should see ONLY the SWE-CLI chat interface filling the entire screen
- When you exit (Ctrl+C), the previous terminal content is restored

**If you still see previous terminal content:**
1. Your terminal emulator might not support alternate screen properly
2. Try a different terminal (iTerm2, Alacritty, or modern versions of Terminal.app on macOS)
3. Check your terminal settings for alternate screen buffer support

### Direct Run

```bash
# Run the chat app directly
python -m swecli.ui_textual.chat_app
```

### Features to Test

1. **Full-screen display** - Verify terminal is completely taken over
2. **Message types:**
   - Type a message and press Enter (user message)
   - See echo response (assistant message)
   - Run `/demo` to see all message types
3. **Keyboard shortcuts:**
   - `Ctrl+L` - Clear conversation
   - `Ctrl+C` - Exit application
   - `ESC` - Interrupt (simulated)
4. **Commands:**
   - `/help` - Show help
   - `/clear` - Clear conversation
   - `/demo` - Show demo messages
   - `/quit` - Exit
5. **Scrolling:**
   - Type many messages to test scrollback
   - Use mouse wheel or arrow keys to scroll

## Next Steps (If POC Succeeds)

### Phase 2: Core Features (1 week)
- Migrate message formatting from ConversationBuffer
- Implement multi-line input (TextArea instead of Input)
- Add command history (Up/Down arrows)
- Paste detection for large content
- Enhanced status bar with real-time updates

### Phase 3: Modals & Dialogs (1 week)
- Approval modal for bash commands
- Model selector modal
- MCP viewer modal
- Rules editor modal

### Phase 4: Advanced Features (1 week)
- Autocomplete for @ mentions and / commands
- Spinner animation during LLM processing
- Real-time token usage tracking
- Syntax highlighting for code blocks

### Phase 5: Integration (3-4 days)
- Connect to existing REPL logic
- Wire up agent responses
- Handle tool execution
- Session management

### Phase 6: Polish & Cleanup (2-3 days)
- Remove old prompt_toolkit UI
- Update documentation
- Performance optimization
- Testing and bug fixes

## Component Mapping

| prompt_toolkit | Textual | Notes |
|---|---|---|
| `ConversationBuffer` | `RichLog` | Better scrollback |
| `Buffer` (input) | `Input` / `TextArea` | Multi-line support |
| `HSplit` / `VSplit` | `Vertical` / `Horizontal` | Similar API |
| `Window` | `Static` / `Widget` | Widget system |
| `FloatContainer` | Modal `Screen` | Better modals |
| `CompletionsMenu` | Built-in suggestions | Native support |
| `Style.from_dict()` | TCSS files | CSS-like |

## Color Scheme

The POC uses the same color scheme as the current UI:

- **Cyan (#00ffff)** - User prompts, function names, borders
- **White (#ffffff)** - Main text, assistant messages
- **Green (#90EE90)** - Tool results, success messages
- **Red (#FF6B6B)** - Errors
- **Gray (#888888)** - System messages, status bar
- **Yellow/Orange** - Normal mode indicator
- **Magenta** - Model name

## Development Notes

### Adding New Widgets

```python
# swecli/ui_textual/widgets/custom_widget.py
from textual.widget import Widget

class CustomWidget(Widget):
    def render(self) -> str:
        return "Widget content"
```

### Adding Modal Screens

```python
# swecli/ui_textual/screens/modal_screen.py
from textual.screen import ModalScreen

class CustomModal(ModalScreen[bool]):
    def compose(self) -> ComposeResult:
        # Modal content
        pass
```

### Styling with TCSS

Edit `styles/chat.tcss` for visual customization. Textual uses CSS-like syntax for styling.

## Resources

- [Textual Documentation](https://textual.textualize.io/)
- [Textual GitHub](https://github.com/Textualize/textual)
- [Rich Documentation](https://rich.readthedocs.io/) (used by Textual)
- [Example Apps](https://www.textualize.io/projects/)

## Questions or Issues?

If the POC reveals issues or limitations, document them here for discussion.

---

**POC Created:** 2025-01-27
**Status:** Testing Phase
**Next Decision:** Proceed with full migration if POC successful
