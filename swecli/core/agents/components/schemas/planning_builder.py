"""Planning tools set for subagent tool filtering.

PLANNING_TOOLS defines the set of read-only tools used by the Planner
sub-agent spec for codebase exploration.
"""

# Read-only tools allowed for planning/exploration subagents
PLANNING_TOOLS = {
    "read_file",
    "list_files",
    "search",  # Unified: type="text" (ripgrep) or "ast" (ast-grep)
    "fetch_url",
    "web_search",  # Web search is read-only
    "list_processes",
    "get_process_output",
    "read_pdf",  # PDF extraction is read-only
    # Symbol tools (read-only)
    "find_symbol",
    "find_referencing_symbols",
    # MCP tool discovery (read-only)
    "search_tools",
    # Subagent spawning (subagents handle their own restrictions)
    "spawn_subagent",
    # Ask user for clarifying questions
    "ask_user",
    # Task completion (always allowed - agents must signal completion)
    "task_complete",
}
