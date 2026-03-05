"""Web Generator subagent for creating beautiful web applications."""

from swecli.core.agents.prompts.loader import load_prompt
from swecli.core.agents.subagents.specs import SubAgentSpec

WEB_GENERATOR_SUBAGENT = SubAgentSpec(
    name="Web-Generator",
    description="Creates beautiful, responsive web applications from scratch using React, TypeScript, and Tailwind CSS. Use for generating new web apps, landing pages, dashboards, or UI-focused projects.",
    system_prompt=load_prompt("subagents/subagent-web-generator"),
    tools=["write_file", "edit_file", "run_command", "list_files", "read_file"],
)
