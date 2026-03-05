<!--
name: 'Tool Description: invoke_skill'
description: Load a skill's knowledge into the conversation
version: 2.0.0
-->

Load a skill's knowledge and instructions into the current conversation context. Skills provide specialized capabilities and domain knowledge for specific types of tasks.

## Usage notes

- Skills only need to be loaded ONCE per conversation — after loading, the skill content remains available in context. Do not re-invoke a skill that is already loaded
- When a skill tag (from a previous invocation) is already present in the conversation, follow its instructions directly instead of invoking again
- BLOCKING REQUIREMENT: When a user references a skill or slash command (e.g., /commit, /review-pr), invoke the relevant skill BEFORE generating any other response about the task
- Call without skill_name to list all available skills
- Do not use this tool for built-in CLI commands (/help, /clear, etc.) — those are handled directly by the CLI
- After a skill is loaded, it may contain checklists or workflows. Follow the skill's instructions exactly as specified
