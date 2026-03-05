"""PR Reviewer subagent for GitHub pull request analysis."""

from swecli.core.agents.prompts.loader import load_prompt
from swecli.core.agents.subagents.specs import SubAgentSpec

PR_REVIEWER_SUBAGENT = SubAgentSpec(
    name="PR-Reviewer",
    description=(
        "Reviews GitHub pull requests. Analyzes code changes for correctness, "
        "style, performance, tests, and security. "
        "USE FOR: PR code review, diff analysis, pre-merge review. "
        "NOT FOR: Security-only audits (use Security-Reviewer instead)."
    ),
    system_prompt=load_prompt("subagents/subagent-pr-reviewer"),
    tools=[
        "read_file",
        "search",
        "list_files",
        "find_symbol",
        "find_referencing_symbols",
        "run_command",
    ],
)
