"""Query enhancement and message preparation for the REPL."""

from __future__ import annotations

import os
import re
from pathlib import Path
from typing import TYPE_CHECKING, Any

from swecli.core.agents.prompts import get_reminder

if TYPE_CHECKING:
    from swecli.repl.file_content_injector import InjectionResult


class QueryEnhancer:
    """Handles query enhancement and message preparation."""

    def __init__(self, file_ops, session_manager, config, console):
        """Initialize query enhancer.

        Args:
            file_ops: File operations interface
            session_manager: Session manager for conversation history
            config: Configuration object
            console: Rich console for output
        """
        self.file_ops = file_ops
        self.session_manager = session_manager
        self.config = config
        self.console = console
        # History window removed — compaction in react_executor handles overflow

    def enhance_query(self, query: str) -> tuple[str, list[dict]]:
        """Enhance query with file contents if referenced.

        Uses FileContentInjector for structured XML-tagged content injection
        with support for text files, directories, PDFs, and images.

        Args:
            query: Original query

        Returns:
            Tuple of (enhanced_query, image_blocks):
            - enhanced_query: Query with @ stripped and file contents appended
            - image_blocks: List of multimodal image blocks for vision API
        """
        from swecli.repl.file_content_injector import FileContentInjector

        # Get working directory from file_ops if available
        working_dir = Path.cwd()
        if hasattr(self.file_ops, "working_dir"):
            working_dir = Path(self.file_ops.working_dir)

        # Use FileContentInjector for structured content injection
        injector = FileContentInjector(self.file_ops, self.config, working_dir)
        result = injector.inject_content(query)

        # Strip @ from query (both quoted and unquoted patterns)
        # Pattern 1: Quoted paths @"path with spaces"
        enhanced = re.sub(r'@"([^"]+)"', r"\1", query)
        # Pattern 2: Unquoted paths (but not emails like user@example.com)
        enhanced = re.sub(r"(?:^|(?<=\s))@([a-zA-Z0-9_./\-]+)", r"\1", enhanced)

        # Append injected content if any
        if result.text_content:
            enhanced = f"{enhanced}\n\n{result.text_content}"

        return enhanced, result.image_blocks

    def prepare_messages(
        self,
        query: str,
        enhanced_query: str,
        agent: Any,
        image_blocks: list[dict] | None = None,
        thinking_visible: bool = False,
    ) -> list:
        """Prepare messages for LLM API call.

        Args:
            query: Original query
            enhanced_query: Query with file contents or @ references processed
            agent: Agent with system prompt
            image_blocks: Optional list of multimodal image blocks for vision API
            thinking_visible: Whether thinking mode is enabled (for dynamic prompt injection)

        Returns:
            List of API messages
        """
        session = self.session_manager.current_session
        messages: list[dict] = []

        if session:
            compaction = getattr(session, "metadata", {}).get("compaction_point")
            if compaction:
                # Start with the compaction summary + messages added after compaction
                summary_content = compaction["summary"]
                at_count = compaction["at_message_count"]
                messages = [{"role": "user", "content": summary_content}]
                # Convert post-compaction messages via a temporary Session-like slice
                post_msgs = session.messages[at_count:]
                if post_msgs:
                    from swecli.models.session import Session

                    temp = Session(session_id="tmp", working_directory="")
                    temp.messages = post_msgs
                    messages.extend(temp.to_api_messages())
            else:
                messages = session.to_api_messages()
            if enhanced_query != query:
                for entry in reversed(messages):
                    if entry.get("role") == "user":
                        entry["content"] = enhanced_query
                        break
        else:
            messages = []

        system_content = agent.system_prompt

        # Replace {thinking_instruction} placeholder based on thinking mode visibility
        if "{thinking_instruction}" in system_content:
            if thinking_visible:
                thinking_text = get_reminder("thinking_on_instruction")
            else:
                thinking_text = get_reminder("thinking_off_instruction")
            system_content = system_content.replace("{thinking_instruction}", thinking_text)

        if session:
            try:
                playbook = session.get_playbook()
                # Use ACE's as_context() method for intelligent bullet selection
                # Configuration from config.playbook section
                playbook_config = getattr(self.config, "playbook", None)
                if playbook_config:
                    max_strategies = playbook_config.max_strategies
                    use_selection = playbook_config.use_selection
                    weights = playbook_config.scoring_weights.to_dict()
                    embedding_model = playbook_config.embedding_model
                    cache_file = playbook_config.cache_file
                    # If cache_file not specified but cache enabled, use session-based default
                    if cache_file is None and playbook_config.cache_embeddings and session:
                        from swecli.core.paths import get_paths

                        paths = get_paths()
                        cache_file = str(
                            paths.global_sessions_dir / f"{session.session_id}_embeddings.json"
                        )
                else:
                    # Fallback to defaults if config not available
                    max_strategies = 30
                    use_selection = True
                    weights = None
                    embedding_model = "text-embedding-3-small"
                    cache_file = None

                playbook_context = playbook.as_context(
                    query=query,  # Enables semantic matching (Phase 2)
                    max_strategies=max_strategies,
                    use_selection=use_selection,
                    weights=weights,
                    embedding_model=embedding_model,
                    cache_file=cache_file,
                )
                if playbook_context:
                    system_content = (
                        f"{system_content.rstrip()}\n\n## Learned Strategies\n{playbook_context}"
                    )
            except Exception:  # pragma: no cover
                pass

        if not messages or messages[0].get("role") != "system":
            messages.insert(0, {"role": "system", "content": system_content})
        else:
            messages[0]["content"] = system_content

        # Handle multimodal content (images)
        # Convert user message to multimodal format if image blocks are present
        if image_blocks:
            for msg in reversed(messages):
                if msg.get("role") == "user":
                    # Get current content (text)
                    current_content = msg.get("content", "")
                    if isinstance(current_content, str):
                        # Convert to multimodal format: list of content blocks
                        multimodal_content: list[dict] = [{"type": "text", "text": current_content}]
                        multimodal_content.extend(image_blocks)
                        msg["content"] = multimodal_content
                    break

        # Debug: Log message count and estimated size
        total_chars = sum(len(str(m.get("content", ""))) for m in messages)
        estimated_tokens = total_chars // 4  # Rough estimate: 4 chars per token
        if self.console and hasattr(self.console, "print"):
            if estimated_tokens > 100000:  # Warn if > 100k tokens
                self.console.print(
                    f"[yellow]⚠ Large context: {len(messages)} messages, ~{estimated_tokens:,} tokens[/yellow]"
                )

        return messages

    @staticmethod
    def format_messages_summary(messages: list, max_preview_len: int = 60) -> str:
        """Format a summary of messages for debug display.

        Args:
            messages: List of message dictionaries
            max_preview_len: Maximum length for content preview

        Returns:
            Formatted summary string
        """
        if not messages:
            return "0 messages"

        summary_parts = []
        for msg in messages:
            role = msg.get("role", "unknown")
            content = msg.get("content", "")

            # Handle string content
            if isinstance(content, str):
                preview = content[:max_preview_len]
                if len(content) > max_preview_len:
                    preview += "..."
            # Handle list content (for tool results, images, etc.)
            elif isinstance(content, list):
                preview = f"[{len(content)} blocks]"
            else:
                preview = str(content)[:max_preview_len]

            summary_parts.append(f"{role}: {preview}")

        return f"{len(messages)} messages: " + " | ".join(summary_parts)
