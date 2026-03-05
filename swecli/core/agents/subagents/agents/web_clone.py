"""Web clone subagent for replicating website UI/design."""

from swecli.core.agents.prompts.loader import load_prompt
from swecli.core.agents.subagents.specs import SubAgentSpec

WEB_CLONE_SUBAGENT = SubAgentSpec(
    name="Web-clone",
    description="Analyzes websites visually and generates code to replicate their UI/design. Use for cloning landing pages, dashboards, or any web UI.",
    system_prompt=load_prompt("subagents/subagent-web-clone"),
    tools=["capture_web_screenshot", "analyze_image", "write_file", "read_file", "run_command", "list_files"],
)
