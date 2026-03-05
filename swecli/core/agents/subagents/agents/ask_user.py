"""Ask-user built-in subagent for gathering user input."""

from swecli.core.agents.prompts.loader import load_prompt
from swecli.core.agents.subagents.specs import SubAgentSpec

ASK_USER_SUBAGENT: SubAgentSpec = {
    "name": "ask-user",
    "description": (
        "Ask the user clarifying questions with structured multiple-choice options. "
        "Use when you need to gather preferences, clarify ambiguous requirements, "
        "or confirm critical decisions. "
        "IMPORTANT: The prompt parameter MUST be a JSON string with this exact structure: "
        '{"questions": [{"question": "Your question?", "header": "ShortTag", '
        '"options": [{"label": "Option1", "description": "What it means"}, '
        '{"label": "Option2", "description": "What it means"}], "multiSelect": false}]}. '
        "Each question needs 2-4 options. Returns user's selections or indicates if cancelled."
    ),
    "system_prompt": load_prompt("subagents/subagent-ask-user"),
    "tools": [],  # No tools needed - UI-only interaction
    # Mark as built-in special type for manager.py detection
    "_builtin_type": "ask-user",
}
