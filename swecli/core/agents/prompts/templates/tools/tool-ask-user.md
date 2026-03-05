<!--
name: 'Tool Description: ask_user'
description: Ask the user structured questions with multiple-choice options
version: 2.0.0
-->

Ask the user structured questions with multiple-choice options. Use this when you need to gather user preferences, clarify ambiguous instructions, get decisions on implementation choices, or offer choices about direction.

## Usage notes

- Supports 1-4 questions with 2-4 options each
- Users will always be able to select "Other" to provide custom text input — you do not need to include an "Other" option
- Use multiSelect=true to allow multiple answers when choices are not mutually exclusive. Phrase the question accordingly (e.g., "Which features do you want to enable?")
- If you recommend a specific option, make that the first option in the list and add "(Recommended)" at the end of the label
- When planning: use ask_user to clarify requirements or choose between approaches BEFORE finalizing a plan. Do NOT use ask_user to ask "Is this plan okay?" — use present_plan for plan approval instead
