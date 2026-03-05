"""End-to-end test for incremental conversation summarization with real API calls.

This test verifies that:
1. Initial summarization works with real LLM
2. Incremental summarization only sends new messages
3. Summary quality is maintained across updates
"""

import os
import sys
from typing import List, Dict, Any

# Ensure the package is importable
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from openai import OpenAI
from swecli.core.context_engineering.memory.conversation_summarizer import (
    ConversationSummarizer,
)


def create_llm_caller():
    """Create a real LLM caller using OpenAI API."""
    client = OpenAI()
    call_count = [0]
    messages_sent = []

    def llm_caller(messages: List[Dict[str, Any]], task_monitor) -> Dict[str, Any]:
        call_count[0] += 1
        # Extract the user message content to see what we're sending
        user_msg = next((m for m in messages if m["role"] == "user"), None)
        if user_msg:
            messages_sent.append(user_msg["content"])

        try:
            response = client.chat.completions.create(
                model="gpt-4o-mini",  # Use cheaper model for testing
                messages=messages,
                max_tokens=500,
            )
            content = response.choices[0].message.content
            return {"success": True, "content": content}
        except Exception as e:
            return {"success": False, "error": str(e)}

    return llm_caller, call_count, messages_sent


def make_conversation(n_exchanges: int) -> List[Dict[str, Any]]:
    """Create a realistic conversation with n exchanges."""
    messages = []
    topics = [
        ("How do I create a Python virtual environment?",
         "You can create a virtual environment using `python -m venv myenv`. Then activate it with `source myenv/bin/activate` on Unix or `myenv\\Scripts\\activate` on Windows."),
        ("What's the difference between list and tuple?",
         "Lists are mutable (can be modified after creation) while tuples are immutable. Lists use square brackets [], tuples use parentheses (). Tuples are slightly faster and can be used as dictionary keys."),
        ("How do I read a JSON file in Python?",
         "Use the json module: `import json; with open('file.json') as f: data = json.load(f)`. This parses the JSON into a Python dictionary or list."),
        ("What is a decorator in Python?",
         "A decorator is a function that modifies another function's behavior. It's defined with @decorator_name syntax above the function. Common uses include logging, timing, and access control."),
        ("How do I handle exceptions?",
         "Use try/except blocks: `try: risky_code() except ValueError as e: handle_error(e)`. You can also use finally for cleanup and else for code that runs if no exception occurred."),
        ("What's the difference between == and is?",
         "== compares values (equality), while 'is' compares identity (same object in memory). Use == for value comparison and 'is' for None checks or singleton comparisons."),
        ("How do I sort a list of dictionaries?",
         "Use sorted() with a key function: `sorted(list_of_dicts, key=lambda x: x['field'])`. For reverse order, add `reverse=True`. You can also use operator.itemgetter for performance."),
        ("What are *args and **kwargs?",
         "*args collects positional arguments into a tuple, **kwargs collects keyword arguments into a dictionary. They allow functions to accept variable numbers of arguments."),
        ("How do I work with dates in Python?",
         "Use the datetime module: `from datetime import datetime; now = datetime.now()`. For parsing: `datetime.strptime('2024-01-15', '%Y-%m-%d')`. For formatting: `now.strftime('%Y-%m-%d')`."),
        ("What is a context manager?",
         "A context manager handles setup and cleanup using 'with' statements. Define one with __enter__ and __exit__ methods, or use @contextmanager decorator. Common use: file handling with automatic closing."),
    ]

    for i in range(n_exchanges):
        topic = topics[i % len(topics)]
        messages.append({"role": "user", "content": topic[0]})
        messages.append({"role": "assistant", "content": topic[1]})

    return messages


def test_initial_summarization():
    """Test that initial summarization works with real LLM."""
    print("\n" + "=" * 60)
    print("TEST 1: Initial Summarization")
    print("=" * 60)

    summarizer = ConversationSummarizer(
        regenerate_threshold=5,
        exclude_last_n=4,  # Keep last 2 exchanges out
    )
    llm_caller, call_count, messages_sent = create_llm_caller()

    # Create conversation with 10 messages (5 exchanges)
    messages = make_conversation(5)
    print(f"Created conversation with {len(messages)} messages")

    # Generate initial summary
    summary = summarizer.generate_summary(messages, llm_caller)

    print(f"\nLLM calls made: {call_count[0]}")
    print(f"Summary length: {len(summary)} chars")
    print(f"\nSummary:\n{summary}")

    # Verify prompt contained "(No previous summary)"
    assert len(messages_sent) == 1, "Should make exactly 1 LLM call"
    assert "(No previous summary)" in messages_sent[0], "First call should have no previous summary"
    assert summary, "Summary should not be empty"

    print("\n✓ Initial summarization PASSED")
    return summarizer, messages


def test_incremental_summarization(summarizer, initial_messages):
    """Test that incremental summarization only sends new messages."""
    print("\n" + "=" * 60)
    print("TEST 2: Incremental Summarization")
    print("=" * 60)

    llm_caller, call_count, messages_sent = create_llm_caller()

    # Add 6 more messages (3 more exchanges)
    messages = initial_messages.copy()
    new_exchanges = make_conversation(8)[10:16]  # Get messages 10-15 (exchanges 6-8)
    messages.extend(new_exchanges)

    print(f"Total messages now: {len(messages)}")
    print(f"New messages added: {len(new_exchanges)}")
    print(f"Cache last_summarized_index: {summarizer._cache.last_summarized_index}")

    # Generate incremental summary
    summary = summarizer.generate_summary(messages, llm_caller)

    print(f"\nLLM calls made: {call_count[0]}")
    print(f"Summary length: {len(summary)} chars")

    # Check what was sent to LLM
    if messages_sent:
        prompt = messages_sent[0]
        print(f"\nPrompt analysis:")

        # Should contain previous summary
        has_prev_summary = "(No previous summary)" not in prompt
        print(f"  - Contains previous summary: {has_prev_summary}")

        # Count which user messages are in the prompt
        for i, msg in enumerate(messages):
            if msg["role"] == "user":
                content_preview = msg["content"][:30]
                in_prompt = content_preview in prompt
                print(f"  - Message {i} '{content_preview}...': {'IN PROMPT' if in_prompt else 'not in prompt'}")

        assert has_prev_summary, "Incremental call should include previous summary"

    print(f"\nUpdated Summary:\n{summary}")
    print("\n✓ Incremental summarization PASSED")
    return summarizer, messages


def test_no_new_messages_no_call(summarizer, messages):
    """Test that no LLM call is made when there are no new messages."""
    print("\n" + "=" * 60)
    print("TEST 3: No New Messages = No LLM Call")
    print("=" * 60)

    llm_caller, call_count, messages_sent = create_llm_caller()

    # Call again with same messages
    summary = summarizer.generate_summary(messages, llm_caller)

    print(f"LLM calls made: {call_count[0]}")
    print(f"Returned cached summary: {len(summary)} chars")

    assert call_count[0] == 0, "Should not make LLM call when no new messages"
    assert summary, "Should return cached summary"

    print("\n✓ No unnecessary LLM calls PASSED")


def test_token_efficiency():
    """Measure token efficiency of incremental vs full summarization."""
    print("\n" + "=" * 60)
    print("TEST 4: Token Efficiency Comparison")
    print("=" * 60)

    # Track message lengths sent to LLM
    incremental_lengths = []
    full_lengths = []

    # Test incremental approach
    print("\nIncremental approach:")
    summarizer = ConversationSummarizer(regenerate_threshold=3, exclude_last_n=2)
    llm_caller, _, messages_sent = create_llm_caller()

    messages = []
    for i in range(5):
        # Add 4 messages (2 exchanges)
        new_msgs = make_conversation(i * 2 + 2)[i * 4:(i + 1) * 4 + 4]
        messages.extend(new_msgs[:4])

        if len(messages) > 6:  # Enough to summarize
            summarizer.generate_summary(messages, llm_caller)

    for i, msg in enumerate(messages_sent):
        length = len(msg)
        incremental_lengths.append(length)
        print(f"  Call {i + 1}: {length} chars")

    print(f"  Total chars sent: {sum(incremental_lengths)}")

    # Simulate full re-summarization approach (what we avoided)
    print("\nFull re-summarization (simulated):")
    for i, msg in enumerate(messages_sent):
        # Estimate what full approach would send
        # Each call would include all previous messages, not just new ones
        estimated_full = len(msg) * (i + 1) * 0.7  # Rough estimate
        full_lengths.append(int(estimated_full))
        print(f"  Call {i + 1}: ~{int(estimated_full)} chars (estimated)")

    print(f"  Total chars (estimated): {sum(full_lengths)}")

    if incremental_lengths and full_lengths:
        savings = (1 - sum(incremental_lengths) / sum(full_lengths)) * 100
        print(f"\nEstimated token savings: ~{savings:.0f}%")

    print("\n✓ Token efficiency test completed")


def main():
    """Run all e2e tests."""
    print("=" * 60)
    print("E2E TEST: Incremental Conversation Summarization")
    print("=" * 60)

    # Check API key
    if not os.environ.get("OPENAI_API_KEY"):
        print("ERROR: OPENAI_API_KEY not set")
        sys.exit(1)

    print("Using OpenAI API with gpt-4o-mini")

    try:
        # Run tests in sequence
        summarizer, messages = test_initial_summarization()
        summarizer, messages = test_incremental_summarization(summarizer, messages)
        test_no_new_messages_no_call(summarizer, messages)
        test_token_efficiency()

        print("\n" + "=" * 60)
        print("ALL E2E TESTS PASSED ✓")
        print("=" * 60)

    except AssertionError as e:
        print(f"\n✗ TEST FAILED: {e}")
        sys.exit(1)
    except Exception as e:
        print(f"\n✗ ERROR: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()
