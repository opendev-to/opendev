<!--
name: 'Tool Description: PresentPlan'
description: Present a plan file for user approval
version: 1.0.0
-->

Present a plan file for user approval after the Planner subagent has finished writing it.

## How This Tool Works

- Takes the plan file path as a parameter
- Reads the plan content from the file
- Displays the plan to the user and opens an approval dialog
- Returns the user's decision: approve, modify, or reject

## When to Use This Tool

After a Planner subagent completes and returns a plan file path, call this tool to get user sign-off before implementation.

## Flow

1. Spawn a Planner subagent with a plan file path
2. Planner explores the codebase and writes the plan
3. Call present_plan(plan_file_path="...") to show the plan
4. Handle the result:
   - **approved**: Proceed with implementation
   - **modify**: Re-spawn the Planner with the feedback and the same file path, then call present_plan again
   - **rejected**: Ask the user how to proceed

## Important

- Do NOT use ask_user to ask "Is this plan okay?" — that's what this tool does
- The plan file must exist and be non-empty before calling this tool
