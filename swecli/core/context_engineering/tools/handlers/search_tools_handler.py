"""Handler for MCP tool discovery with token-efficient search.

This handler implements the search_tools function that allows agents to
discover MCP tools on-demand rather than loading all schemas upfront.
"""

from __future__ import annotations

import logging
from typing import Any, TYPE_CHECKING

if TYPE_CHECKING:
    from swecli.core.context_engineering.mcp.manager import MCPManager

logger = logging.getLogger(__name__)


class SearchToolsHandler:
    """Handler for searching and discovering MCP tools.

    This enables token-efficient tool discovery - MCP tool schemas are not
    loaded into context by default. The agent uses search_tools() to find
    relevant tools, which then get added to the discovered set and included
    in subsequent LLM calls.
    """

    def __init__(
        self,
        mcp_manager: "MCPManager | None" = None,
        on_discover: "callable | None" = None,
    ):
        """Initialize search tools handler.

        Args:
            mcp_manager: MCP manager instance for accessing tool metadata
            on_discover: Callback when tools are discovered (adds to registry)
        """
        self._mcp_manager = mcp_manager
        self._on_discover = on_discover

    def set_mcp_manager(self, mcp_manager: "MCPManager | None") -> None:
        """Set the MCP manager.

        Args:
            mcp_manager: MCP manager instance
        """
        self._mcp_manager = mcp_manager

    def set_on_discover(self, callback: "callable | None") -> None:
        """Set the discovery callback.

        Args:
            callback: Function to call when tools are discovered
        """
        self._on_discover = callback

    def _score_and_match_tools(
        self, query: str, tools: list[dict[str, Any]]
    ) -> list[dict[str, Any]]:
        """Score tools using DYNAMIC vocabulary from actual tool data.

        This approach extracts keywords from actual tool names/descriptions,
        then filters query words to only those that exist in the vocabulary.
        Words like "java", "python", "500", "stars" naturally get filtered out
        because they don't appear in any tool name or description.

        Args:
            query: Search query string
            tools: List of tool definitions to search

        Returns:
            List of matched tools, sorted by relevance score
        """
        # Step 1: Build vocabulary from actual tool names/descriptions
        # This is the source of truth - no hardcoding!
        tool_vocabulary: set[str] = set()

        for tool in tools:
            name = tool.get("name", "").lower()
            desc = (tool.get("description") or "").lower()

            # Extract words from tool names (split on _ and -)
            for word in name.replace("_", " ").replace("-", " ").split():
                if len(word) >= 2:  # Skip single chars
                    tool_vocabulary.add(word)

            # Extract words from descriptions
            for word in desc.split():
                word = word.strip(".,;:()[]\"'")
                if len(word) >= 3:  # Skip short words from descriptions
                    tool_vocabulary.add(word)

        # Common abbreviations that map to vocabulary words
        abbreviations = {
            "repo": "repository",
            "repos": "repositories",
            "pr": "pull",
            "prs": "pull",
        }

        # Step 2: Filter query to only words that exist in tool vocabulary
        query_lower = query.lower()
        # Remove common data query syntax before tokenizing
        query_clean = query_lower.replace(":", " ").replace(">=", " ").replace("<=", " ")
        query_words = query_clean.replace("-", " ").replace("_", " ").split()

        relevant_words: list[str] = []
        for word in query_words:
            word = word.strip(".,;:()[]\"'")
            if not word or len(word) < 2:
                continue

            # Check if word is in vocabulary
            if word in tool_vocabulary:
                relevant_words.append(word)
            # Check abbreviation expansion
            elif word in abbreviations and abbreviations[word] in tool_vocabulary:
                relevant_words.append(abbreviations[word])
            # Check partial match (prefix matching)
            else:
                for vocab_word in tool_vocabulary:
                    if vocab_word.startswith(word) and len(word) >= 3:
                        relevant_words.append(vocab_word)
                        break
                    if word.startswith(vocab_word) and len(vocab_word) >= 3:
                        relevant_words.append(vocab_word)
                        break

        # If no relevant words found, return empty (no matches)
        if not relevant_words:
            logger.debug(f"No vocabulary matches for query: {query}")
            return []

        logger.debug(f"Query '{query}' -> relevant words: {relevant_words}")

        # Step 3: Score tools based on relevant words only
        scored_tools: list[tuple[int, dict[str, Any]]] = []

        for tool in tools:
            name = tool.get("name", "").lower()
            desc = (tool.get("description") or "").lower()
            name_normalized = name.replace("_", " ").replace("-", " ")

            score = 0
            for word in relevant_words:
                if word in name_normalized:
                    score += 2  # Higher score for name match
                elif word in desc:
                    score += 1  # Lower score for description match

            if score > 0:
                scored_tools.append((score, tool))

        # Sort by score (highest first), then by name for stability
        scored_tools.sort(key=lambda x: (-x[0], x[1].get("name", "")))

        return [tool for _, tool in scored_tools]

    def search_tools(
        self, arguments: dict[str, Any], context: Any = None
    ) -> dict[str, Any]:
        """Search for MCP tools matching a query.

        Args:
            arguments: Tool arguments containing:
                - query: Search query (matches names and descriptions)
                - detail_level: 'names', 'brief', or 'full'
                - server: Optional server name filter
            context: Tool execution context

        Returns:
            Result dict with matching tools
        """
        if not self._mcp_manager:
            return {
                "success": False,
                "error": "MCP Manager not available. No MCP servers connected.",
                "output": None,
            }

        query = arguments.get("query", "").lower().strip()
        detail_level = arguments.get("detail_level", "brief")
        server_filter = arguments.get("server")

        # Auto-detect server from query (e.g., "github" in query → filter to github)
        if not server_filter:
            for server_name in self._mcp_manager.list_servers():
                if server_name.lower() in query:
                    server_filter = server_name
                    logger.debug(f"Auto-detected server filter: {server_filter}")
                    break

        # Get all MCP tools
        if server_filter:
            all_tools = self._mcp_manager.get_server_tools(server_filter)
            if not all_tools:
                return {
                    "success": False,
                    "error": f"Server '{server_filter}' not found or has no tools.",
                    "output": None,
                }
        else:
            all_tools = self._mcp_manager.get_all_tools()

        if not all_tools:
            return {
                "success": True,
                "output": "No MCP tools available. Connect to MCP servers first using /mcp connect <name>.",
                "tools": [],
                "count": 0,
            }

        # Filter tools by query using score-based matching
        if query and query != "*":
            matched_tools = self._score_and_match_tools(query, all_tools)
        else:
            # Return all tools if query is empty or '*'
            matched_tools = all_tools

        if not matched_tools:
            return {
                "success": True,
                "output": f"No tools found matching '{query}'.",
                "tools": [],
                "count": 0,
            }

        # Mark matched tools as discovered (for full detail level)
        # This adds them to the registry so they appear in subsequent LLM calls
        discovered_names = []
        if detail_level == "full" and self._on_discover:
            for tool in matched_tools:
                tool_name = tool.get("name", "")
                if tool_name:
                    self._on_discover(tool_name)
                    discovered_names.append(tool_name)

        # Format output based on detail level
        output_lines = []
        tools_data = []

        if detail_level == "names":
            output_lines.append(f"Found {len(matched_tools)} tool(s):\n")
            for tool in matched_tools:
                name = tool.get("name", "")
                output_lines.append(f"  - {name}")
                tools_data.append({"name": name})

        elif detail_level == "brief":
            output_lines.append(f"Found {len(matched_tools)} tool(s):\n")
            for tool in matched_tools:
                name = tool.get("name", "")
                desc = tool.get("description", "")
                # Truncate description to first line/sentence
                if desc:
                    desc = desc.split("\n")[0][:100]
                    if len(desc) == 100:
                        desc += "..."
                output_lines.append(f"  - **{name}**: {desc}")
                tools_data.append({"name": name, "description": desc})

        else:  # full
            output_lines.append(f"Found {len(matched_tools)} tool(s) (schemas now loaded):\n")
            for tool in matched_tools:
                name = tool.get("name", "")
                desc = tool.get("description", "")
                params = tool.get("input_schema", {})

                output_lines.append(f"\n### {name}")
                if desc:
                    output_lines.append(f"{desc}\n")

                # Show parameters
                if params and params.get("properties"):
                    output_lines.append("**Parameters:**")
                    for param_name, param_info in params.get("properties", {}).items():
                        param_type = param_info.get("type", "any")
                        param_desc = param_info.get("description", "")[:80]
                        required = param_name in params.get("required", [])
                        req_marker = " (required)" if required else ""
                        output_lines.append(f"  - `{param_name}` ({param_type}){req_marker}: {param_desc}")

                tools_data.append({
                    "name": name,
                    "description": desc,
                    "parameters": params,
                })

        # Add hint about using full detail level
        if detail_level != "full" and matched_tools:
            output_lines.append(
                "\n\nTip: Use detail_level='full' to load tool schemas into context for use."
            )

        result = {
            "success": True,
            "output": "\n".join(output_lines),
            "tools": tools_data,
            "count": len(matched_tools),
            "detail_level": detail_level,
        }

        if discovered_names:
            result["discovered"] = discovered_names
            result["output"] += f"\n\n✓ {len(discovered_names)} tool schema(s) now available for use."

        return result
