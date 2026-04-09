Launch a new agent to handle complex, multi-step tasks autonomously.

{agent_listing}

When using the Agent tool, specify an agent_type parameter to select which agent type to use.

When NOT to use the Agent tool:
- If you want to read a specific file path, use Read instead of spawning an agent
- If you are searching for a specific class definition like "class Foo", use Grep instead
- If you are searching for code within a specific file or set of 2-3 files, use Read instead
- Any task achievable in 1-2 tool calls — subagent overhead is never justified for these

Usage notes:
- Always include a short description (3-8 words) summarizing what the agent will do
- Launch multiple agents concurrently whenever possible, to maximize performance; to do that, make all Agent calls in the SAME response for parallel execution
- Use foreground (default) when you need the agent's results before you can proceed. Use background (run_in_background: true) when you have genuinely independent work to do in parallel
- The agent's outputs are not visible to the user — you must present findings in your response
- Clearly tell the agent whether you expect it to write code or just do research
- If the user specifies that they want you to run agents "in parallel", you MUST make all Agent tool calls in a single response

**Note**: For tasks requiring inter-agent coordination, shared task lists, or dependent steps, use Agent Teams (`SpawnTeammate`) instead. Activate via `ToolSearch(query="select:SpawnTeammate,SendMessage")`.
