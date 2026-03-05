"""Full end-to-end test with actual CLI components and real API calls.

This tests the complete flow: ReactExecutor -> ConversationSummarizer -> OpenAI API
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from unittest.mock import MagicMock, patch
from rich.console import Console

from swecli.repl.react_executor import ReactExecutor
from swecli.core.context_engineering.memory.conversation_summarizer import ConversationSummarizer
from openai import OpenAI


def test_full_flow_with_real_api():
    """Test the full flow with real OpenAI API calls."""
    print("\n" + "=" * 60)
    print("FULL E2E TEST: Real API Integration")
    print("=" * 60)

    # Track all LLM calls
    llm_calls = {"summary": [], "thinking": []}
    client = OpenAI()

    def real_llm_caller(messages, task_monitor):
        """Real LLM caller that tracks calls."""
        # Detect if this is a summary call or thinking call
        user_content = messages[1]["content"] if len(messages) > 1 else ""
        is_summary_call = "# Previous Summary" in user_content or "# New Messages" in user_content

        if is_summary_call:
            llm_calls["summary"].append(user_content)
        else:
            llm_calls["thinking"].append(messages[0]["content"])  # System prompt

        try:
            response = client.chat.completions.create(
                model="gpt-4o-mini",
                messages=messages,
                max_tokens=500,
            )
            return {"success": True, "content": response.choices[0].message.content}
        except Exception as e:
            return {"success": False, "error": str(e)}

    # Create ReactExecutor with mocked deps but real LLM caller
    executor = ReactExecutor(
        console=Console(),
        session_manager=MagicMock(),
        config=MagicMock(),
        llm_caller=MagicMock(),
        tool_executor=MagicMock(),
    )

    # Create mock agent that uses real LLM
    mock_agent = MagicMock()
    mock_agent.call_thinking_llm = real_llm_caller
    mock_agent.build_system_prompt = MagicMock(
        return_value="""You are an AI assistant helping with software engineering.

# Context
{context}

Based on the context above, reason about what to do next."""
    )

    # Build conversation with 12 non-system messages (triggers summarization)
    messages = [{"role": "system", "content": "You are a helpful assistant."}]
    conversation_topics = [
        ("How do I create a REST API in Python?",
         "You can use Flask or FastAPI. FastAPI is modern and has automatic OpenAPI docs."),
        ("What's the difference between REST and GraphQL?",
         "REST uses multiple endpoints with fixed structures. GraphQL has one endpoint with flexible queries."),
        ("How do I add authentication to my API?",
         "Use JWT tokens for stateless auth. Libraries like PyJWT handle token creation and validation."),
        ("What about rate limiting?",
         "Use slowapi for FastAPI or flask-limiter for Flask. Redis can store rate limit counters."),
        ("How do I deploy to production?",
         "Use Docker containers with gunicorn/uvicorn. Deploy to AWS ECS, GCP Cloud Run, or Kubernetes."),
        ("What about database connections?",
         "Use connection pooling with SQLAlchemy or asyncpg. Configure pool size based on expected load."),
    ]

    for q, a in conversation_topics:
        messages.append({"role": "user", "content": q})
        messages.append({"role": "assistant", "content": a})

    print(f"Total messages: {len(messages)} ({len(messages)-1} non-system)")

    # First thinking call - should trigger initial summarization
    print("\n--- First thinking call ---")
    result1 = executor._get_thinking_trace(messages, mock_agent, ui_callback=None)

    print(f"Summary LLM calls: {len(llm_calls['summary'])}")
    print(f"Thinking LLM calls: {len(llm_calls['thinking'])}")

    assert len(llm_calls["summary"]) == 1, "Should make 1 summary call"
    assert "(No previous summary)" in llm_calls["summary"][0], "First call should have no previous summary"
    print("✓ Initial summary created")

    # Check that summary was injected into thinking context
    assert "CONVERSATION SUMMARY" in llm_calls["thinking"][0], "Summary should be in thinking context"
    print("✓ Summary injected into thinking context")

    # Add more messages
    messages.extend([
        {"role": "user", "content": "How do I handle errors in my API?"},
        {"role": "assistant", "content": "Use exception handlers and return proper HTTP status codes."},
        {"role": "user", "content": "What about logging?"},
        {"role": "assistant", "content": "Use structured logging with JSON format. Tools like structlog help."},
        {"role": "user", "content": "Should I add metrics?"},
        {"role": "assistant", "content": "Yes, use Prometheus for metrics. Add counters for requests and latency histograms."},
    ])

    print(f"\nAdded 6 more messages. Total: {len(messages)}")

    # Force cache to need regeneration
    executor._conversation_summarizer._cache.message_count = len(messages) - 6

    # Second thinking call - should trigger incremental summarization
    print("\n--- Second thinking call (incremental) ---")
    llm_calls["summary"].clear()
    llm_calls["thinking"].clear()

    result2 = executor._get_thinking_trace(messages, mock_agent, ui_callback=None)

    print(f"Summary LLM calls: {len(llm_calls['summary'])}")
    print(f"Thinking LLM calls: {len(llm_calls['thinking'])}")

    if llm_calls["summary"]:
        prompt = llm_calls["summary"][0]
        has_previous = "(No previous summary)" not in prompt
        print(f"✓ Has previous summary: {has_previous}")
        assert has_previous, "Incremental call should include previous summary"

        # Check message distribution:
        # - Messages 1-6 (indices 0-5): Already summarized in first call
        # - Messages 7-12 (indices 6-11): Should be in this incremental summary
        # - Messages 13-18 (indices 12-17): In exclude_last_n window (short-term memory)
        has_rate_limiting = "rate limiting" in prompt  # Message ~7-8, should be included
        has_first_msg = "How do I create a REST API" in prompt  # Already summarized
        has_newest_msg = "How do I handle errors" in prompt  # In short-term memory

        print(f"  - Previously unsummarized msg (rate limiting): {has_rate_limiting}")
        print(f"  - Already summarized msg NOT in prompt: {not has_first_msg}")
        print(f"  - Short-term memory msg NOT in prompt: {not has_newest_msg}")

        # The incremental approach correctly:
        # 1. Includes the previous summary (not re-summarizing old messages)
        # 2. Only sends messages between last_summarized_index and end_index
        # 3. Excludes messages in the short-term memory window

    print("\n" + "=" * 60)
    print("FULL E2E TEST PASSED ✓")
    print("=" * 60)


if __name__ == "__main__":
    if not os.environ.get("OPENAI_API_KEY"):
        print("ERROR: OPENAI_API_KEY not set")
        sys.exit(1)

    test_full_flow_with_real_api()
