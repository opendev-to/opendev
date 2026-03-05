"""Long-term memory and learning for OpenDev (ACE Pattern).

This module provides a complete native re-implementation of the ACE framework
within swecli, without external dependencies.

ACE Components:
    - Playbook: Structured storage for learned strategies (Bullet objects)
    - Generator: Produces answers using playbook strategies
    - Reflector: Analyzes execution outcomes (LLM-powered)
    - Curator: Evolves playbook through delta operations
    - Delta operations: ADD, UPDATE, TAG, REMOVE mutations

Based on: Agentic Context Engine (ACE)
Paper: https://arxiv.org/abs/2510.04618
Repository: https://github.com/kayba-ai/agentic-context-engine
"""

# Import native ACE components
from swecli.core.context_engineering.memory.playbook import Playbook, Bullet
from swecli.core.context_engineering.memory.delta import DeltaOperation, DeltaBatch
from swecli.core.context_engineering.memory.roles import (
    AgentResponse,
    Reflector,
    Curator,
    ReflectorOutput,
    CuratorOutput,
)

# Legacy imports for backwards compatibility (deprecated)
from swecli.core.context_engineering.memory.playbook import SessionPlaybook, Strategy
from swecli.core.context_engineering.memory.reflection import ExecutionReflector, ReflectionResult

# Conversation summarization for thinking context
from swecli.core.context_engineering.memory.conversation_summarizer import (
    ConversationSummarizer,
    ConversationSummary,
)

__all__ = [
    # Native ACE Components (recommended)
    "Playbook",
    "Bullet",
    "AgentResponse",
    "Reflector",
    "Curator",
    "ReflectorOutput",
    "CuratorOutput",
    "DeltaOperation",
    "DeltaBatch",
    # Legacy (deprecated, for backwards compatibility)
    "SessionPlaybook",
    "Strategy",
    "ExecutionReflector",
    "ReflectionResult",
    # Conversation summarization for thinking context
    "ConversationSummarizer",
    "ConversationSummary",
]
