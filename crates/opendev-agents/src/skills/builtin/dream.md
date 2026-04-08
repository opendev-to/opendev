---
name: dream
description: "Consolidate and prune memory files. Merges old session notes into durable topic memories, removes duplicates, and keeps MEMORY.md index clean."
---

# Memory Consolidation (Dream)

## Overview
This skill triggers memory consolidation: merging session notes, pruning stale entries, and keeping the memory directory organized.

## When to Use This Skill
- When the user asks to consolidate, clean up, or organize memories
- When the user references /dream
- When memory files have accumulated and need pruning

## Instructions

### Step 1: Survey Memory State
List all memory files and check their types and ages:
```
memory list --scope project
```

### Step 2: Identify Consolidation Candidates
Look for:
- Multiple `type: session` files that can be merged
- Duplicate information across files
- Stale files (>30 days old) that may need updating or removal
- Files missing proper frontmatter (type/description)

### Step 3: Merge Session Notes
For each group of session files covering overlapping work:
1. Read all session files
2. Extract durable facts: architecture decisions, conventions, bugs found, learnings
3. Discard ephemeral details: timestamps, greetings, intermediate debugging steps
4. Write a consolidated `type: project` memory with the merged content

### Step 4: Prune Consolidated Files
After merging, remove the original session files that were fully consolidated.

### Step 5: Fix Missing Frontmatter
For any files missing `type:` or `description:` in frontmatter, add them.

### Step 6: Verify MEMORY.md Index
The index auto-updates on writes. Verify it accurately reflects the current state.

## Rules
- NEVER delete or modify `type: user` memories (personal preferences are atomic)
- NEVER delete or modify `type: reference` memories (external pointers are atomic)
- ALWAYS back up files before modifying them (copy to a backup location first)
- Prefer merging overlapping content over keeping duplicates
- Convert relative dates to absolute dates when consolidating
- Keep MEMORY.md index entries under 150 characters each
