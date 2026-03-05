"""Security Reviewer subagent for vulnerability analysis."""

from swecli.core.agents.prompts.loader import load_prompt
from swecli.core.agents.subagents.specs import SubAgentSpec

SECURITY_REVIEWER_SUBAGENT = SubAgentSpec(
    name="Security-Reviewer",
    description=(
        "Security-focused code review. Analyzes code changes for vulnerabilities "
        "with structured severity/confidence scoring. "
        "USE FOR: Security audits, PR security review, vulnerability assessment. "
        "NOT FOR: General code review or style feedback."
    ),
    system_prompt=load_prompt("subagents/subagent-security-reviewer"),
    tools=[
        "read_file",
        "search",
        "list_files",
        "find_symbol",
        "find_referencing_symbols",
        "run_command",
    ],
)
