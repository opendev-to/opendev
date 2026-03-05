# Context Management System

ACE-inspired context management for swecli that maintains useful learnings across sessions without accumulating noisy conversation history.

## Overview

Traditional chat agents save all conversation messages to session history, leading to:
- **Context pollution**: Irrelevant messages from previous queries
- **Verbose noise**: "I'll do X", "I'm doing X", "I've done X" descriptions
- **Poor reusability**: Raw messages don't generalize to new situations
- **Context overflow**: Unbounded growth of session history

This module solves these problems using structured strategy storage inspired by [ACE (Agentic Context Engine)](https://arxiv.org/abs/2510.04618).

## Architecture

```
context_management/
├── __init__.py           # Module exports
├── playbook.py           # Strategy storage (Playbook + Strategy classes)
├── reflection/           # Pattern extraction
│   ├── __init__.py
│   └── reflector.py     # ExecutionReflector for learning from tool calls
└── README.md            # This file
```

## Core Concepts

### 1. Strategy (Distilled Learning)

Instead of storing raw messages like:
```python
{"role": "assistant", "content": "I'll delete the file. First let me list the directory..."}
```

We store distilled strategies:
```python
Strategy(
    id="fil-00042",
    category="file_operations",
    content="List directory before file deletion to confirm file exists",
    helpful_count=5,
    harmful_count=0
)
```

**Benefits**:
- Concise (52 chars vs 86 chars)
- Reusable across sessions
- Effectiveness tracking
- Structured and searchable

### 2. SessionPlaybook (Strategy Store)

Container for learned strategies with operations:
- `add_strategy()` - Add new learning
- `tag_strategy()` - Mark as helpful/harmful/neutral
- `prune_harmful_strategies()` - Remove low-value patterns
- `as_context()` - Format for system prompt inclusion

**Example usage**:
```python
from swecli.core.context_management import SessionPlaybook

playbook = SessionPlaybook()
strategy = playbook.add_strategy(
    category="file_operations",
    content="List directory before reading files"
)

# Later, after successful execution:
playbook.tag_strategy(strategy.id, "helpful")

# Include in system prompt:
context = playbook.as_context()
```

### 3. ExecutionReflector (Pattern Extraction)

Analyzes tool execution sequences to extract learnable patterns:

```python
from swecli.core.context_management import ExecutionReflector

reflector = ExecutionReflector()

# Tool calls from a query
tool_calls = [
    ToolCall(name="list_files", parameters={"path": "."}),
    ToolCall(name="read_file", parameters={"file_path": "test.py"})
]

# Extract learning
result = reflector.reflect(
    query="check the test file",
    tool_calls=tool_calls,
    outcome="success"
)

if result:
    # result.category: "file_operations"
    # result.content: "List directory before reading files..."
    # result.confidence: 0.75
    playbook.add_strategy(result.category, result.content)
```

## Pattern Categories

The reflector identifies patterns in these categories:

### File Operations
- List before read
- Read before write
- Multiple related file reads

### Code Navigation
- Search before read
- Multiple searches for exploration
- Grep patterns then read

### Testing
- Run tests after changes
- Read test files before execution
- TDD workflows

### Shell Commands
- Install dependencies before run
- Build before test
- Check git status before operations

### Error Handling
- List directory on file access errors
- Verify environment on command failures
- Check preconditions before operations

## Integration Flow

### 1. During Tool Execution (ReAct Loop)

```python
# In async_query_processor.py
async def process_query(self, query: str):
    # ... existing ReAct loop ...

    # After tool calls execute successfully:
    if tool_calls and not any(result.error for result in tool_results):
        # Extract learning
        reflection = self.reflector.reflect(
            query=query,
            tool_calls=tool_calls,
            outcome="success"
        )

        if reflection and reflection.confidence >= 0.7:
            # Add to playbook
            session.playbook.add_strategy(
                category=reflection.category,
                content=reflection.content
            )
```

### 2. Preparing Messages for LLM

```python
# In async_query_processor.py
def _prepare_messages(self, query: str) -> list:
    """Prepare messages with playbook context."""

    # Load only tool-calling messages (not text-only responses)
    messages = session.to_api_messages()

    # Build system prompt with playbook
    system_content = self.agent.system_prompt

    # Add learned strategies
    playbook_context = session.playbook.as_context()
    if playbook_context:
        system_content += playbook_context

    messages.insert(0, {"role": "system", "content": system_content})

    return messages
```

### 3. Session Storage

```python
# Session now includes playbook
@dataclass
class Session:
    # ... existing fields ...
    playbook: SessionPlaybook = field(default_factory=SessionPlaybook)

    def to_dict(self) -> dict:
        return {
            # ... existing serialization ...
            "playbook": self.playbook.to_dict()
        }

    @classmethod
    def from_dict(cls, data: dict) -> "Session":
        session = cls(...)
        session.playbook = SessionPlaybook.from_dict(data.get("playbook", {}))
        return session
```

## Effectiveness Tracking

Strategies track their usefulness over time:

```python
strategy = playbook.strategies["fil-00042"]

# After successful use:
strategy.tag("helpful")  # helpful_count += 1

# After causing issues:
strategy.tag("harmful")  # harmful_count += 1

# Calculate effectiveness (-1 to 1):
score = strategy.effectiveness_score
# score = (helpful - harmful) / total

# Auto-prune harmful strategies:
removed = playbook.prune_harmful_strategies(threshold=-0.3)
```

## Playbook Context Format

When included in system prompts:

```markdown
## Learned Strategies

### File Operations
- [fil-00042] List directory before reading files (helpful=5, harmful=0)
- [fil-00043] Read file before writing to preserve data (helpful=3, harmful=0)

### Code Navigation
- [cod-00015] Search for keywords before reading files (helpful=8, harmful=1)
- [cod-00016] Read multiple related files for complete picture (helpful=4, harmful=0)

### Testing
- [tes-00007] Run tests after code changes (helpful=12, harmful=0)
```

**Benefits**:
- Clear structure by category
- Effectiveness visible to LLM
- Unique IDs for tracking
- Concise and actionable

## Configuration

```python
# Initialize with custom settings
playbook = SessionPlaybook()
reflector = ExecutionReflector(
    min_tool_calls=2,     # Minimum tools to trigger reflection
    min_confidence=0.6    # Minimum confidence to save strategy
)

# Playbook operations
playbook.add_strategy(category, content)
playbook.get_strategies_by_category("file_operations")
playbook.prune_harmful_strategies(threshold=-0.3)
playbook.stats()  # Get statistics

# Format for prompt (max strategies)
context = playbook.as_context(max_strategies=50)
```

## Testing

```python
# Test strategy creation
strategy = Strategy(
    id="test-001",
    category="testing",
    content="Run tests after changes"
)
assert strategy.effectiveness_score == 0.0

# Test effectiveness tracking
strategy.tag("helpful")
strategy.tag("helpful")
strategy.tag("harmful")
assert strategy.effectiveness_score == (2-1)/3  # 0.33

# Test playbook operations
playbook = SessionPlaybook()
s1 = playbook.add_strategy("file_ops", "List before read")
assert len(playbook) == 1
assert playbook.get_strategy(s1.id) == s1

# Test reflection
reflector = ExecutionReflector()
tool_calls = [
    ToolCall(name="list_files", parameters={"path": "."}),
    ToolCall(name="read_file", parameters={"file_path": "test.py"})
]
result = reflector.reflect("check file", tool_calls, "success")
assert result is not None
assert result.category == "file_operations"
assert result.confidence >= 0.7
```

## Migration Plan

### Phase 1: Current State ✅
- Text-only responses not saved to session
- Tool-calling messages still saved

### Phase 2: Add Playbook (In Progress)
- Add `playbook` field to Session model
- Initialize empty playbook for new sessions
- Load playbook from existing sessions

### Phase 3: Integrate Reflection
- Add reflector to query processor
- Extract strategies after tool execution
- Store in session playbook

### Phase 4: Use in System Prompt
- Include playbook context in system message
- Format strategies by category
- Show effectiveness counters

### Phase 5: Advanced Features
- LLM-powered reflection (instead of pattern matching)
- Cross-session strategy sharing
- Auto-pruning of low-value strategies
- Strategy deduplication and merging

## Comparison: Before vs After

| Aspect | Before (Raw Messages) | After (Strategies) |
|--------|----------------------|-------------------|
| **Storage** | Every assistant message | Distilled patterns only |
| **Verbosity** | "I'll do X... I'm doing... Done!" | "Do X before Y" |
| **Reusability** | Low (specific to query) | High (generalizable) |
| **Context Size** | Grows linearly | Bounded by strategy count |
| **Effectiveness** | Unknown | Tracked (helpful/harmful) |
| **Search** | Unstructured text | Categorized, ID-based |
| **Noise** | High | Low |

## References

- **ACE Paper**: [Agentic Context Engineering](https://arxiv.org/abs/2510.04618)
- **ACE Repository**: [kayba-ai/agentic-context-engine](https://github.com/kayba-ai/agentic-context-engine)
- **Related**: [Dynamic Cheatsheet](https://arxiv.org/abs/2504.07952)
- **swecli Docs**: `docs/ACE_ARCHITECTURE_ANALYSIS.md`

## Key Takeaways

1. **Structure over Verbosity**: Distilled strategies beat raw messages
2. **Track Effectiveness**: Know what helps vs. hurts
3. **Reflect to Learn**: Post-execution analysis extracts value
4. **Bounded Growth**: Strategies don't accumulate linearly like messages
5. **Reusable Knowledge**: Patterns generalize across queries

---

**Status**: In active development
**Next**: Integrate playbook into Session model and query processor
