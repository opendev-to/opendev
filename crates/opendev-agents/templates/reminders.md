--- thinking_analysis_prompt ---
The user's original request: {original_task}

Analyze the full context and provide your reasoning for the next step. Keep the user's complete original request in mind — if it has multiple parts, ensure you are working toward ALL parts, not just the first.

IMPORTANT: If your next step involves reading or searching multiple files to understand code structure, architecture, or patterns, you MUST delegate to Explore rather than doing it yourself. Only use direct read_file/search for known, specific targets (1-2 files).

--- thinking_analysis_prompt_with_todos ---
The user's original request: {original_task}

Current todos ({done_count}/{total_count} done):
{todo_status}

Analyze the context and provide your reasoning for the next step. You MUST continue working on the next incomplete todo. Do not summarize or finish until all todos are done.

IMPORTANT: If your next step involves reading or searching multiple files to understand code structure, architecture, or patterns, you MUST delegate to Explore rather than doing it yourself. Only use direct read_file/search for known, specific targets (1-2 files).

--- thinking_trace_reminder ---
<thinking_trace>
{thinking_trace}
</thinking_trace>

The thinking trace above is your INTERNAL reasoning — treat it as your private thought process, not as a task report. Use it to guide your next action:
- If the trace identifies a tool call or code change, perform that step.
- If the trace concludes a conversational response is appropriate, respond naturally to the user with text — no need to call task_complete.
Stay aligned with the trace's conclusions but express yourself naturally.

--- subagent_complete_signal ---
<subagent_complete>
All subagents have completed. Evaluate ALL results together and continue:
1. If the user asked a question, synthesize findings from all agents into one unified answer — do not summarize each agent separately.
2. If the user requested implementation, proceed — write code, edit files, run commands.
3. If the subagent failed or found nothing useful, handle the task directly. Do NOT re-spawn the same subagent.
</subagent_complete>

--- planner_complete_signal ---
<planner_complete>
The Planner has finished writing the plan. You MUST now call present_plan(plan_file_path="{plan_file_path}") to show it to the user for approval. Do NOT start implementing or reading files — the user must approve the plan first.
</planner_complete>

--- failed_tool_nudge ---
The previous tool call failed. Read the error carefully, identify the root cause, and fix it before retrying. Do NOT repeat the exact same call — change your approach. If you cannot fix the issue, try an alternative approach or explain what is blocking you.

--- nudge_permission_error ---
Permission denied. Do NOT retry the same command. Check if the file is read-only or try a different path you have write access to.

--- nudge_file_not_found ---
File not found. Do NOT guess the path. Use list_files or search to find the correct path first, then retry with the exact path found.

--- nudge_syntax_error ---
The edit introduced a syntax error. Read the file to see the current state, identify the syntax issue, and retry with corrected content. Do NOT repeat the same edit.

--- nudge_rate_limit ---
The API rate limit was hit. Wait a moment before retrying. Consider reducing concurrent operations.

--- nudge_timeout ---
The command timed out. Try a more targeted approach — narrow the scope, search specific directories instead of the entire repo, or break the operation into smaller steps.

--- nudge_edit_mismatch ---
The edit old_content did not match. The file has changed since you last read it. Read the file again to get its current content, then retry with the exact text.

--- consecutive_reads_nudge ---
You have been reading without taking action. If you have enough information, proceed with implementation. If you need clarification, ask the user.

--- safety_limit_summary ---
MAXIMUM STEPS REACHED — tools are now disabled.

Provide a structured summary:
1. **Goal**: What was the original task?
2. **Accomplished**: What was completed successfully?
3. **Remaining**: What still needs to be done?
4. **Next steps**: What should the user do to continue?

Be specific about file paths and changes made.

--- thinking_on_instruction ---
**CRITICAL REQUIREMENT - THINKING MODE IS ON:** You MUST call the `think` tool FIRST before calling ANY other tool. This is mandatory - do NOT skip this step. Do NOT call write_file, read_file, bash, or any other tool before calling `think`. In your thinking, explain step-by-step: what you understand about the task, your approach, and your planned actions. Aim for 100-300 words. Only after calling `think` may you proceed with other tools.

--- thinking_off_instruction ---
For complex tasks, briefly explain your reasoning in 1-2 sentences. For simple tasks, act directly.

--- incomplete_todos_nudge ---
You have {count} incomplete todo(s):
{todo_list}

Work through your remaining todos using this workflow:
1. Call update_todo(id, status="in_progress") to mark the next item as in-progress
2. Implement that item (write code, edit files, run commands)
3. Call complete_todo(id) when the item is done
4. Repeat for each remaining todo

Do NOT call task_complete until ALL todos are done.

--- file_read_nudge ---
You have made {consecutive_reads} consecutive read-only operations without taking action.

Consider:
1. If you have enough information, proceed with the task
2. If you need clarification, ask the user
3. If you're stuck, explain what's blocking you

Avoid excessive exploration - focus on taking action based on what you've learned.

--- file_exists_warning ---
This file content was injected from a user @ reference. The file exists on disk — do not re-read it with read_file unless you need a refreshed copy.

--- json_retry_simple ---
Your response was not valid JSON. Please respond with ONLY a valid JSON object, no markdown, no explanation.

--- json_retry_with_fields ---
Your response was not valid JSON. Please respond with ONLY a valid JSON object containing the required fields. No markdown, no explanation, just the JSON object.

--- init_complete_signal ---
The AGENTS.md file has been created at {path}. Provide a brief 1-sentence summary confirming what was generated.

--- plan_approved_signal ---
<plan_approved>
Your plan has been approved and {todos_created} todos are ready.

<approved_plan>
{plan_content}
</approved_plan>

Work through the todos in order:
- Mark todo as "doing" (update_todo)
- Implement the step fully — write code, edit files, run commands as needed
- Mark as "done" (complete_todo) only after the implementation is complete
- After ALL todos are done, call task_complete with a brief summary.

Do NOT mark a todo as done without implementing it. Each todo requires actual code changes.
</plan_approved>

--- thinking_analysis_prompt_plan_execution ---
The user's original request: {original_task}

You are executing an approved plan. Analyze the context and provide your reasoning for the next step. Focus on WHAT to implement, not on exploring. Work through the plan steps in order.

--- all_todos_complete_nudge ---
All implementation todos are now complete. Call task_complete with a summary of what was accomplished.

--- docker_command_failed_nudge ---
COMMAND FAILED with exit code {exit_code}. Review the error output above and fix the issue before proceeding. Do not repeat the same command without addressing the root cause.

--- plan_subagent_request ---
User requested planning. Before creating a plan, first understand the codebase:

1. List the current directory structure to see what exists
2. Read relevant files to understand patterns and conventions
3. Then spawn a Planner subagent with your findings and this plan file path: {plan_file_path}

After the Planner returns, call present_plan(plan_file_path="{plan_file_path}").

--- tool_denied_nudge ---
The tool call was denied. Do NOT re-attempt the exact same call. Consider why it was denied and adjust your approach. If unclear, use ask_user to ask the user why the tool call was denied.

--- explore_phase_complete ---
<explore_complete>
You now have codebase context. Spawn a Planner subagent with:
(1) the original task, (2) what you learned from exploring,
(3) plan file path: {plan_file_path}
</explore_complete>

--- plan_file_reference ---
A plan file exists from a previous session at {plan_file_path}. You may read
it with read_file and call present_plan to show it for approval, or spawn a
Planner subagent to revise it.

--- explore_first_nudge ---
Before proceeding with this subagent, you should first explore the codebase using Explore to build context about the relevant code areas. Spawn Explore first to understand the existing code structure, then re-spawn this subagent with the enriched context.

--- explore_delegate_nudge ---
You have been reading files individually to explore the codebase. For multi-file exploration, you MUST delegate to Explore instead of reading files one-by-one.

Spawn a Explore subagent now with a clear question about what you need to understand. Explore is purpose-built for codebase exploration and will be more thorough and efficient.

--- doom_loop_redirect_nudge ---
You are repeating the same operation. STOP and try something different: use a different tool, change your arguments, or ask the user for help. Do NOT repeat the previous tool call.

--- doom_loop_stepback_nudge ---
You have been stuck in a loop despite a previous warning. Your current approach is not working. STOP entirely. Re-read the original task, identify which assumption is wrong, and choose a completely different strategy. If you cannot proceed, explain what is blocking you.

--- truncation_continue_directive ---
Your previous response was truncated due to output token limit. Continue from where you left off.

--- doom_loop_compact_directive ---
You appear to be stuck in a repeating loop. Summarize what you have learned so far, discard irrelevant details, and try a fundamentally different approach.

--- doom_loop_force_stop_message ---
The agent was unable to make progress and has been stopped. Please try rephrasing your request or providing more specific guidance.

--- implicit_completion_nudge ---
Before finishing, verify you have fully addressed the user's complete request:

{original_task}

If there are remaining parts you haven't addressed yet, continue working — use tools to make progress. If everything is truly done, call task_complete with a brief summary of what was accomplished.
