"""Unified context selection and assembly for LLM calls.

This module provides the ContextPicker class - the single entry point for all
context engineering before LLM calls. It coordinates file reference injection,
conversation history, playbook strategies, and system prompt assembly.

All decisions are logged as ContextReason objects for full traceability.
"""

from __future__ import annotations

import logging
import os
from pathlib import Path
from typing import TYPE_CHECKING, Any, Optional

from swecli.core.paths import get_paths
from .models import (
    AssembledContext,
    ContextCategory,
    ContextPiece,
    ContextReason,
)
from .tracer import get_tracer

if TYPE_CHECKING:
    from swecli.core.context_engineering.history import SessionManager
    from swecli.core.context_engineering.memory import Playbook
    from swecli.core.context_engineering.tools.implementations import FileOperations
    from swecli.models.config import Config

logger = logging.getLogger(__name__)


class ContextPicker:
    """Unified context selection and assembly.
    
    This is THE single entry point for all context engineering before LLM calls.
    It coordinates:
    - File reference injection (@mentions)
    - Conversation history windowing
    - Playbook strategy selection
    - System prompt assembly
    
    All decisions are logged as ContextReason objects for traceability.
    
    Example:
        picker = ContextPicker(session_manager, config, file_ops)
        context = picker.pick_context("Explain @main.py", agent)
        
        # Debug what was included
        print(context.summary(verbose=True))
        
        # Use for LLM call
        messages = context.messages
    """
    
    # Default configuration
    DEFAULT_MAX_STRATEGIES = 30
    
    def __init__(
        self,
        session_manager: "SessionManager",
        config: "Config",
        file_ops: "FileOperations",
        *,
        console: Any = None,
    ):
        """Initialize the context picker.
        
        Args:
            session_manager: Session manager for conversation history
            config: Configuration object
            file_ops: File operations interface
            console: Optional Rich console for output
            tracer: Optional custom tracer (uses default if not provided)
        """
        self.session_manager = session_manager
        self.config = config
        self.file_ops = file_ops
        self.console = console
        self._tracer = get_tracer()
        
        # Lazy-loaded injector
        self._file_injector: Any = None
    
    def pick_context(
        self,
        query: str,
        agent: Any,
        *,
        max_tokens: Optional[int] = None,
        trace: bool = False,
    ) -> AssembledContext:
        """Assemble all context for an LLM call.
        
        This is THE method called before every LLM invocation.
        
        Args:
            query: User's query (may contain @file references)
            agent: Agent with system prompt
            max_tokens: Token budget (uses config default if None)
            trace: If True, log the context trace
            
        Returns:
            AssembledContext with all pieces and their reasons
        """
        pieces: list[ContextPiece] = []
        image_blocks: list[dict] = []
        
        # 1. Pick file references from @ mentions
        file_pieces, file_images = self._pick_file_references(query)
        pieces.extend(file_pieces)
        image_blocks.extend(file_images)
        
        # 2. Pick playbook strategies
        strategy_pieces = self._pick_playbook_strategies(query)
        pieces.extend(strategy_pieces)
        
        # 3. Build system prompt with strategies
        system_piece = self._assemble_system_prompt(agent, strategy_pieces)
        pieces.append(system_piece)
        
        # 4. Pick conversation history
        history_pieces = self._pick_conversation_history()
        pieces.extend(history_pieces)
        
        # 5. Add current query piece
        query_piece = self._create_query_piece(query, file_pieces)
        pieces.append(query_piece)
        
        # 6. Assemble final messages
        messages = self._build_messages(
            system_prompt=system_piece.content,
            query=query,
            file_pieces=file_pieces,
            image_blocks=image_blocks,
        )
        
        # Calculate total tokens
        total_tokens = sum(p.tokens_estimate for p in pieces)
        
        # Create assembled context
        context = AssembledContext(
            system_prompt=system_piece.content,
            messages=messages,
            pieces=pieces,
            image_blocks=image_blocks,
            total_tokens_estimate=total_tokens,
        )
        
        # Trace if requested (for debugging)
        if trace:
            self._tracer.trace(context)
        
        # Warn on large context
        max_context = max_tokens or getattr(self.config, "max_context_tokens", 128000)
        if total_tokens > max_context * 0.8:
            logger.warning(
                f"Context is large: {total_tokens:,} tokens "
                f"({total_tokens / max_context * 100:.1f}% of {max_context:,} limit)"
            )
        
        return context
    
    def _pick_file_references(
        self,
        query: str,
    ) -> tuple[list[ContextPiece], list[dict]]:
        """Extract and inject @file references.
        
        Args:
            query: User query with potential @ references
            
        Returns:
            Tuple of (context pieces, image blocks)
        """
        from swecli.repl.file_content_injector import FileContentInjector
        
        pieces: list[ContextPiece] = []
        
        # Get working directory
        working_dir = Path.cwd()
        if hasattr(self.file_ops, "working_dir"):
            working_dir = Path(self.file_ops.working_dir)
        
        # Initialize injector if needed
        if self._file_injector is None:
            self._file_injector = FileContentInjector(
                self.file_ops,
                self.config,
                working_dir,
            )
        
        # Inject content
        result = self._file_injector.inject_content(query)
        
        # Create pieces for text content
        if result.text_content:
            # Parse the injected content to identify individual files
            # For now, treat as single piece - could be enhanced to split
            reason = ContextReason(
                source="file_reference",
                reason=f"User referenced files with @ in query",
                tokens_estimate=len(result.text_content) // 4,
                metadata={"query_contains": "@"},
            )
            pieces.append(ContextPiece(
                content=result.text_content,
                reason=reason,
                category=ContextCategory.FILE_REFERENCE,
                order=10,
            ))
        
        # Create pieces for images
        for img_block in result.image_blocks:
            reason = ContextReason(
                source="image_reference",
                reason="User referenced image file with @",
                metadata={"image_url": img_block.get("image_url", {}).get("url", "")[:50]},
            )
            pieces.append(ContextPiece(
                content="[Image content]",
                reason=reason,
                category=ContextCategory.IMAGE_CONTENT,
                order=15,
            ))
        
        # Log errors
        for error in result.errors:
            logger.warning(f"File injection error: {error}")
        
        return pieces, result.image_blocks
    
    def _pick_conversation_history(self) -> list[ContextPiece]:
        """Select relevant conversation history.
        
        Returns:
            List of context pieces from history
        """
        pieces: list[ContextPiece] = []
        
        session = self.session_manager.current_session
        if not session:
            return pieces
        
        # Get all messages from session (compaction handles overflow)
        messages = session.to_api_messages()
        
        # Exclude system message (handled separately)
        history_messages = [m for m in messages if m.get("role") != "system"]
        
        if history_messages:
            # Calculate tokens
            total_chars = sum(len(str(m.get("content", ""))) for m in history_messages)
            
            reason = ContextReason(
                source="conversation_history",
                reason=f"All {len(history_messages)} messages from session",
                tokens_estimate=total_chars // 4,
                metadata={
                    "message_count": len(history_messages),
                },
            )
            
            # Track as single piece for now
            pieces.append(ContextPiece(
                content=f"[{len(history_messages)} conversation messages]",
                reason=reason,
                category=ContextCategory.CONVERSATION_HISTORY,
                order=50,
            ))
        
        return pieces
    
    def _pick_playbook_strategies(self, query: str) -> list[ContextPiece]:
        """Select relevant learned strategies from playbook.
        
        Args:
            query: User query for semantic matching
            
        Returns:
            List of context pieces from playbook
        """
        pieces: list[ContextPiece] = []
        
        session = self.session_manager.current_session
        if not session:
            return pieces
        
        try:
            playbook = session.get_playbook()
            if not playbook or not playbook.bullets():
                return pieces
            
            # Get playbook config
            playbook_config = getattr(self.config, "playbook", None)
            
            if playbook_config:
                max_strategies = playbook_config.max_strategies
                use_selection = playbook_config.use_selection
                weights = playbook_config.scoring_weights.to_dict()
                embedding_model = playbook_config.embedding_model
                cache_file = playbook_config.cache_file
                
                # Default cache file if not specified
                if cache_file is None and playbook_config.cache_embeddings:
                    paths = get_paths()
                    cache_file = str(
                        paths.global_sessions_dir / f"{session.session_id}_embeddings.json"
                    )
            else:
                max_strategies = self.DEFAULT_MAX_STRATEGIES
                use_selection = True
                weights = None
                embedding_model = "text-embedding-3-small"
                cache_file = None
            
            # Get selected strategies
            playbook_context = playbook.as_context(
                query=query,
                max_strategies=max_strategies,
                use_selection=use_selection,
                weights=weights,
                embedding_model=embedding_model,
                cache_file=cache_file,
            )
            
            if playbook_context:
                # Count strategies in output
                strategy_count = playbook_context.count("•") or len(playbook.bullets())
                
                reason = ContextReason(
                    source="playbook_strategies",
                    reason=f"Selected {strategy_count} relevant strategies from learned playbook",
                    tokens_estimate=len(playbook_context) // 4,
                    relevance_score=0.9,  # High relevance since selected
                    metadata={
                        "total_strategies": len(playbook.bullets()),
                        "selected_strategies": strategy_count,
                        "use_selection": use_selection,
                        "embedding_model": embedding_model,
                    },
                )
                
                pieces.append(ContextPiece(
                    content=playbook_context,
                    reason=reason,
                    category=ContextCategory.PLAYBOOK_STRATEGY,
                    order=5,  # Strategies go early in context
                ))
        
        except Exception as e:
            logger.debug(f"Error getting playbook strategies: {e}")
        
        return pieces
    
    def _assemble_system_prompt(
        self,
        agent: Any,
        strategy_pieces: list[ContextPiece],
    ) -> ContextPiece:
        """Build final system prompt with strategies.
        
        Args:
            agent: Agent with base system prompt
            strategy_pieces: Selected strategy pieces to include
            
        Returns:
            Context piece for system prompt
        """
        base_prompt = agent.system_prompt
        
        # Append strategies if any
        if strategy_pieces:
            strategies_content = "\n\n".join(p.content for p in strategy_pieces)
            full_prompt = f"{base_prompt.rstrip()}\n\n## Learned Strategies\n{strategies_content}"
        else:
            full_prompt = base_prompt
        
        reason = ContextReason(
            source="system_prompt",
            reason=f"Agent system prompt" + (
                f" with {len(strategy_pieces)} strategy sections"
                if strategy_pieces else ""
            ),
            tokens_estimate=len(full_prompt) // 4,
            metadata={"agent_type": type(agent).__name__},
        )
        
        return ContextPiece(
            content=full_prompt,
            reason=reason,
            category=ContextCategory.SYSTEM_PROMPT,
            order=0,  # System prompt first
        )
    
    def _create_query_piece(
        self,
        query: str,
        file_pieces: list[ContextPiece],
    ) -> ContextPiece:
        """Create context piece for the user's query.
        
        Args:
            query: User's query
            file_pieces: File references found in query
            
        Returns:
            Context piece for query
        """
        reason = ContextReason(
            source="user_query",
            reason="Current user query",
            tokens_estimate=len(query) // 4,
            metadata={
                "has_file_refs": len(file_pieces) > 0,
                "query_length": len(query),
            },
        )
        
        return ContextPiece(
            content=query,
            reason=reason,
            category=ContextCategory.USER_QUERY,
            order=100,  # Query at end
        )
    
    def _build_messages(
        self,
        system_prompt: str,
        query: str,
        file_pieces: list[ContextPiece],
        image_blocks: list[dict],
    ) -> list[dict[str, Any]]:
        """Build final messages list for API call.
        
        Args:
            system_prompt: Complete system prompt
            query: User's query
            file_pieces: File content pieces
            image_blocks: Image blocks for multimodal
            
        Returns:
            List of message dicts for API
        """
        import re
        
        # Get conversation history (compaction handles overflow)
        session = self.session_manager.current_session
        if session:
            messages = session.to_api_messages()
        else:
            messages = []
        
        # Enhance query with file content
        enhanced_query = query
        if file_pieces:
            file_content = "\n\n".join(p.content for p in file_pieces)
            # Strip @ from query
            enhanced = re.sub(r'@"([^"]+)"', r'\1', query)
            enhanced = re.sub(r'(?:^|(?<=\s))@([a-zA-Z0-9_./\-]+)', r'\1', enhanced)
            enhanced_query = f"{enhanced}\n\n{file_content}"
        
        # Update user message in history or create new
        if messages:
            # Update last user message
            for entry in reversed(messages):
                if entry.get("role") == "user":
                    entry["content"] = enhanced_query
                    break
        
        # Ensure system message is first
        if not messages or messages[0].get("role") != "system":
            messages.insert(0, {"role": "system", "content": system_prompt})
        else:
            messages[0]["content"] = system_prompt
        
        # Handle multimodal content
        if image_blocks:
            for msg in reversed(messages):
                if msg.get("role") == "user":
                    current_content = msg.get("content", "")
                    if isinstance(current_content, str):
                        multimodal_content: list[dict] = [
                            {"type": "text", "text": current_content}
                        ]
                        multimodal_content.extend(image_blocks)
                        msg["content"] = multimodal_content
                    break
        
        return messages
    
    def get_summary(self, context: AssembledContext) -> str:
        """Get a concise summary of context.
        
        Args:
            context: Assembled context
            
        Returns:
            Summary string
        """
        return context.summary()
