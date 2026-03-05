"""Reflection system for extracting learnings from tool executions.

This module implements pattern extraction and learning from successful
and failed tool executions, inspired by ACE's Reflector role.
"""

from swecli.core.context_engineering.memory.reflection.reflector import (
    ExecutionReflector,
    ReflectionResult,
)

__all__ = ["ExecutionReflector", "ReflectionResult"]
