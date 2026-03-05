"""Centralized blank line management for ALL conversation rendering.

CORE RULE: Exactly 1 blank line between ANY two visible content elements.

Spacing Philosophy:
-------------------
To achieve exactly 1 blank line between elements:
1. CONTENT ELEMENTS: No trailing blank (next element adds leading blank)
   - Assistant messages
   - Tool results
   - System messages

2. STRUCTURAL ELEMENTS: Add trailing blank (next element still checks first)
   - User messages
   - Error messages
   - Command results
   - Thinking blocks
   - Agent completions (parallel/single)
   - Bash output boxes

3. LEADING BLANKS: All elements add leading blank IF previous line has content
   - This prevents double spacing
   - Check only looks at the LAST line (not past blanks) to respect structural trailing blanks

4. ERROR CASES: Follow structural element rules
   - Command not found: trailing blank
   - Tool errors: no trailing blank (content flows to next)
   - Agent failures: trailing blank (completion)

Example Flow:
-------------
User message (structural -> adds trailing blank)
Assistant message (content -> checks prev, sees blank, no leading; no trailing)
Tool result (content -> checks prev, sees content, adds leading; no trailing)
Thinking block (structural -> checks prev, sees content, adds leading; adds trailing)
"""

from rich.text import Text
from swecli.ui_textual.widgets.conversation.protocols import RichLogInterface


class SpacingManager:
    """Centralized blank line management for ALL conversation rendering.

    Ensures exactly 1 blank line between ANY two visible content elements.
    """

    # Content types for tracking
    CONTENT = "content"  # No trailing blank
    STRUCTURAL = "structural"  # Has trailing blank

    def __init__(self, log: RichLogInterface):
        """Initialize the SpacingManager.

        Args:
            log: The RichLogInterface to write spacing to
        """
        self.log = log

    def _get_last_content(self) -> str:
        """Get the content of the last line (not looking past blanks).

        Returns empty string if last line is blank, allowing structural
        elements' trailing blanks to prevent double spacing.

        Handles both Text objects (.plain) and Strip objects (.text).

        Returns:
            The stripped content of the last line, or empty string if blank
        """
        if not self.log.lines:
            return ""

        last_line = self.log.lines[-1]
        if hasattr(last_line, "plain"):
            return last_line.plain.strip() if last_line.plain else ""
        elif hasattr(last_line, "text"):
            return last_line.text.strip() if last_line.text else ""
        return ""

    def needs_spacing_before(self) -> bool:
        """Check if a blank line is needed before new content.

        Returns:
            True if the last line has content (blank line needed)
        """
        return bool(self._get_last_content())

    def add_spacing_if_needed(self) -> None:
        """Add a leading blank line if the last line has content.

        Use this BEFORE writing any new content element.
        """
        if self.needs_spacing_before():
            self.log.write(Text(" "))  # Visible blank line (space character)

    def mark_end(self, element_type: str) -> None:
        """Mark the end of an element.

        Args:
            element_type: 'content' or 'structural'
                - 'content': No trailing blank (let next element add leading)
                - 'structural': Add trailing blank for visual separation
        """
        if element_type == self.STRUCTURAL:
            self.log.write(Text(" "))
        # For 'content', do nothing - no trailing blank

    # Convenience methods for specific element types

    def before_user_message(self) -> None:
        """Add spacing before user message."""
        self.add_spacing_if_needed()

    def after_user_message(self, is_command: bool) -> None:
        """Add spacing after user message.

        Args:
            is_command: If True, no trailing blank (commands handle spacing)
                       If False, add trailing blank (structural element)
        """
        if not is_command:
            self.mark_end(self.STRUCTURAL)

    def before_assistant_message(self) -> None:
        """Add spacing before assistant message."""
        self.add_spacing_if_needed()

    def after_assistant_message(self) -> None:
        """No trailing blank - content element."""
        pass

    def before_thinking(self) -> None:
        """Add spacing before thinking block."""
        self.add_spacing_if_needed()

    def after_thinking(self) -> None:
        """Add trailing blank - structural element for visual separation."""
        self.mark_end(self.STRUCTURAL)

    def before_tool_call(self) -> None:
        """Add spacing before tool call."""
        self.add_spacing_if_needed()

    def after_tool_result(self) -> None:
        """No trailing blank - content element."""
        pass

    def after_tool_result_continuation(self) -> None:
        """Add trailing blank after tool result continuation."""
        self.mark_end(self.STRUCTURAL)

    def before_system_message(self) -> None:
        """Add spacing before system message."""
        self.add_spacing_if_needed()

    def after_error(self) -> None:
        """Add trailing blank - structural element."""
        self.mark_end(self.STRUCTURAL)

    def after_command_result(self) -> None:
        """Add trailing blank - structural element."""
        self.mark_end(self.STRUCTURAL)

    def before_command_header(self) -> None:
        """Always add blank before command header (unconditional)."""
        self.log.write(Text(" "))

    def before_nested_tool_call(self) -> None:
        """Add spacing before nested tool call (expanded mode)."""
        self.add_spacing_if_needed()

    def before_parallel_agents(self) -> None:
        """Add spacing before parallel agents."""
        self.add_spacing_if_needed()

    def after_parallel_agents(self) -> None:
        """Add trailing blank after parallel agents complete."""
        self.mark_end(self.STRUCTURAL)

    def before_single_agent(self) -> None:
        """Add spacing before single agent."""
        self.add_spacing_if_needed()

    def after_single_agent(self) -> None:
        """Add trailing blank after single agent completes."""
        self.mark_end(self.STRUCTURAL)

    def after_bash_output_box(self) -> None:
        """Add trailing blank after bash output box."""
        self.mark_end(self.STRUCTURAL)
