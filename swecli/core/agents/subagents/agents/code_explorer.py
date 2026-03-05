"""Code Explorer subagent for codebase exploration and research."""

from swecli.core.agents.prompts.loader import load_prompt
from swecli.core.agents.subagents.specs import SubAgentSpec

CODE_EXPLORER_SUBAGENT = SubAgentSpec(
    name="Code-Explorer",
    description=(
        "Deep LOCAL codebase exploration and research. Systematically searches and analyzes code to answer questions. "
        "USE FOR: Understanding code architecture, finding patterns, researching implementation details in LOCAL files. "
        "NOT FOR: External searches (GitHub repos, web) - use MCP tools or fetch_url instead."
    ),
    system_prompt=load_prompt("subagents/subagent-code-explorer"),
    tools=["read_file", "search", "list_files", "find_symbol", "find_referencing_symbols"],
)
