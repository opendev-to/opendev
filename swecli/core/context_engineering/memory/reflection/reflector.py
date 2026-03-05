"""Execution reflector for extracting learnings from tool executions.

Inspired by ACE's Reflector role, this module analyzes tool execution patterns
to extract reusable strategies that can be stored in the session playbook.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, List, Optional

if TYPE_CHECKING:
    from swecli.models.message import ToolCall


@dataclass
class ReflectionResult:
    """Result of reflecting on a tool execution sequence.

    Attributes:
        category: Strategy category (e.g., "file_operations")
        content: The distilled strategy content
        confidence: Confidence level (0.0 to 1.0)
        reasoning: Why this pattern is worth learning
    """

    category: str
    content: str
    confidence: float
    reasoning: str


class ExecutionReflector:
    """Analyzes tool execution sequences to extract learnable patterns.

    The reflector identifies patterns worth learning from tool executions,
    such as multi-step workflows, error recovery patterns, and best practices.

    Example:
        >>> reflector = ExecutionReflector()
        >>> tool_calls = [
        ...     ToolCall(name="list_files", parameters={"path": "."}),
        ...     ToolCall(name="read_file", parameters={"file_path": "test.py"})
        ... ]
        >>> result = reflector.reflect(query="check the test file", tool_calls=tool_calls)
        >>> if result:
        ...     print(result.content)
        List directory before reading files to understand structure
    """

    def __init__(
        self,
        min_tool_calls: int = 2,
        min_confidence: float = 0.6,
    ):
        """Initialize execution reflector.

        Args:
            min_tool_calls: Minimum tool calls to trigger reflection (default: 2)
            min_confidence: Minimum confidence to return a result (default: 0.6)
        """
        self.min_tool_calls = min_tool_calls
        self.min_confidence = min_confidence

    def reflect(
        self,
        query: str,
        tool_calls: List[ToolCall],
        outcome: str = "success",
    ) -> Optional[ReflectionResult]:
        """Extract a reusable strategy from tool execution.

        Args:
            query: Original user query
            tool_calls: List of tool calls that were executed
            outcome: Execution outcome ("success", "error", or "partial")

        Returns:
            ReflectionResult if a learnable pattern is found, None otherwise
        """
        if not self._is_worth_learning(tool_calls, outcome):
            return None

        # Try different pattern extractors
        result = (
            self._extract_file_operation_pattern(tool_calls, query)
            or self._extract_code_navigation_pattern(tool_calls, query)
            or self._extract_testing_pattern(tool_calls, query)
            or self._extract_shell_command_pattern(tool_calls, query)
            or self._extract_error_recovery_pattern(tool_calls, query, outcome)
        )

        # Filter by confidence threshold
        if result and result.confidence >= self.min_confidence:
            return result

        return None

    def _is_worth_learning(self, tool_calls: List[ToolCall], outcome: str) -> bool:
        """Determine if execution contains learnable patterns.

        Args:
            tool_calls: List of tool calls
            outcome: Execution outcome

        Returns:
            True if worth learning from, False otherwise
        """
        # Skip single trivial operations
        if len(tool_calls) == 1:
            # Single read is usually not interesting
            if tool_calls[0].name in {"read_file", "list_files"}:
                return False

        # Learn from multi-step sequences
        if len(tool_calls) >= self.min_tool_calls:
            return True

        # Learn from error recovery
        if outcome == "error" and len(tool_calls) > 0:
            return True

        return False

    def _extract_file_operation_pattern(
        self, tool_calls: List[ToolCall], query: str
    ) -> Optional[ReflectionResult]:
        """Extract file operation patterns.

        Patterns:
        - List directory before reading files
        - Search before read for targeted file access
        - Write after read for file modifications
        """
        tool_names = [tc.name for tc in tool_calls]

        # Pattern: list_files -> read_file
        if "list_files" in tool_names and "read_file" in tool_names:
            list_idx = tool_names.index("list_files")
            read_idx = tool_names.index("read_file")
            if list_idx < read_idx:
                return ReflectionResult(
                    category="file_operations",
                    content="List directory contents before reading files to understand structure and locate files",
                    confidence=0.75,
                    reasoning="Sequential list_files -> read_file pattern shows exploratory file access",
                )

        # Pattern: read_file -> write_file (modification workflow)
        if "read_file" in tool_names and "write_file" in tool_names:
            read_idx = tool_names.index("read_file")
            write_idx = tool_names.index("write_file")
            if read_idx < write_idx:
                return ReflectionResult(
                    category="file_operations",
                    content="Read file contents before writing to understand current state and preserve important data",
                    confidence=0.8,
                    reasoning="Sequential read_file -> write_file shows safe modification workflow",
                )

        # Pattern: multiple read_file calls
        read_count = sum(1 for name in tool_names if name == "read_file")
        if read_count >= 3:
            return ReflectionResult(
                category="code_navigation",
                content="When understanding complex code, read multiple related files to build complete picture",
                confidence=0.7,
                reasoning=f"Multiple file reads ({read_count}) indicates thorough code exploration",
            )

        return None

    def _extract_code_navigation_pattern(
        self, tool_calls: List[ToolCall], query: str
    ) -> Optional[ReflectionResult]:
        """Extract code navigation patterns.

        Patterns:
        - Search before read for targeted access
        - Grep for patterns then read matching files
        """
        tool_names = [tc.name for tc in tool_calls]

        # Pattern: search -> read_file
        if "search" in tool_names and "read_file" in tool_names:
            search_idx = tool_names.index("search")
            read_idx = tool_names.index("read_file")
            if search_idx < read_idx:
                return ReflectionResult(
                    category="code_navigation",
                    content="Search for keywords or patterns before reading files to locate relevant code efficiently",
                    confidence=0.8,
                    reasoning="Search followed by read shows targeted file access",
                )

        # Pattern: multiple searches (refinement)
        search_count = sum(1 for name in tool_names if name == "search")
        if search_count >= 2:
            return ReflectionResult(
                category="code_navigation",
                content="Use multiple searches with different keywords to thoroughly explore codebase and find all relevant locations",
                confidence=0.7,
                reasoning=f"Multiple searches ({search_count}) shows iterative code exploration",
            )

        return None

    def _extract_testing_pattern(
        self, tool_calls: List[ToolCall], query: str
    ) -> Optional[ReflectionResult]:
        """Extract testing patterns.

        Patterns:
        - Run tests after code changes
        - Write test before implementation (TDD)
        """
        tool_names = [tc.name for tc in tool_calls]

        # Check for test-related commands
        test_commands = []
        for tc in tool_calls:
            if tc.name == "run_command":
                cmd = tc.parameters.get("command", "")
                if any(keyword in cmd.lower() for keyword in ["test", "pytest", "jest", "npm test"]):
                    test_commands.append(cmd)

        if not test_commands:
            return None

        # Pattern: write/edit -> run tests
        if ("write_file" in tool_names or "edit_file" in tool_names) and test_commands:
            return ReflectionResult(
                category="testing",
                content="Run tests after making code changes to verify correctness and catch regressions early",
                confidence=0.85,
                reasoning="Code modification followed by test execution shows good development practice",
            )

        # Pattern: read test file -> run tests
        if "read_file" in tool_names and test_commands:
            # Check if any read files were test files
            test_file_read = any(
                "test" in tc.parameters.get("file_path", "").lower()
                for tc in tool_calls
                if tc.name == "read_file"
            )
            if test_file_read:
                return ReflectionResult(
                    category="testing",
                    content="Review test files before running tests to understand expected behavior and test coverage",
                    confidence=0.7,
                    reasoning="Reading test files before execution shows thorough testing approach",
                )

        return None

    def _extract_shell_command_pattern(
        self, tool_calls: List[ToolCall], query: str
    ) -> Optional[ReflectionResult]:
        """Extract shell command patterns.

        Patterns:
        - Install dependencies before running
        - Build before test
        - Check status before git operations
        """
        tool_names = [tc.name for tc in tool_calls]

        # Get all run_command calls
        commands = [
            tc.parameters.get("command", "")
            for tc in tool_calls
            if tc.name == "run_command"
        ]

        if len(commands) < 2:
            return None

        # Pattern: install -> run/test
        install_keywords = ["npm install", "pip install", "yarn install", "poetry install"]
        run_keywords = ["npm start", "python", "node", "pytest"]

        has_install = any(
            any(keyword in cmd.lower() for keyword in install_keywords) for cmd in commands
        )
        has_run = any(
            any(keyword in cmd.lower() for keyword in run_keywords) for cmd in commands
        )

        if has_install and has_run:
            return ReflectionResult(
                category="shell_commands",
                content="Install dependencies before running or testing applications to ensure all requirements are met",
                confidence=0.8,
                reasoning="Install followed by run/test shows proper setup workflow",
            )

        # Pattern: git status -> git operations
        if any("git status" in cmd for cmd in commands) and len(commands) > 1:
            return ReflectionResult(
                category="git_operations",
                content="Check git status before performing git operations to understand current state and avoid mistakes",
                confidence=0.75,
                reasoning="Git status check before operations shows careful version control practice",
            )

        return None

    def _extract_error_recovery_pattern(
        self, tool_calls: List[ToolCall], query: str, outcome: str
    ) -> Optional[ReflectionResult]:
        """Extract error recovery patterns.

        Patterns:
        - Retry with different approach after failure
        - Check preconditions before operations
        """
        if outcome != "error":
            return None

        tool_names = [tc.name for tc in tool_calls]

        # Pattern: failed to access file -> list directory first
        if "read_file" in tool_names:
            return ReflectionResult(
                category="error_handling",
                content="When file access fails, list directory first to verify file exists and check path correctness",
                confidence=0.7,
                reasoning="File access error suggests need for directory verification",
            )

        # Pattern: command failed -> check environment
        if "run_command" in tool_names:
            return ReflectionResult(
                category="error_handling",
                content="When commands fail, verify environment setup, dependencies, and working directory before retrying",
                confidence=0.65,
                reasoning="Command execution error suggests environment or dependency issue",
            )

        return None
