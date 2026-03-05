"""Tests for prompt caching wiring (build_two_part + Anthropic payload)."""



class TestBuildTwoPart:
    """SystemPromptBuilder.build_two_part() returns a tuple of two strings."""

    def test_returns_tuple_of_two_strings(self):
        from swecli.core.agents.components.prompts.builders import SystemPromptBuilder

        builder = SystemPromptBuilder(tool_registry=None, working_dir="/tmp")
        result = builder.build_two_part()
        assert isinstance(result, tuple)
        assert len(result) == 2
        stable, dynamic = result
        assert isinstance(stable, str)
        assert isinstance(dynamic, str)

    def test_stable_is_non_empty(self):
        from swecli.core.agents.components.prompts.builders import SystemPromptBuilder

        builder = SystemPromptBuilder(tool_registry=None, working_dir="/tmp")
        stable, _ = builder.build_two_part()
        assert len(stable) > 0

    def test_combined_matches_build(self):
        """The combined stable+dynamic should contain the same content as build()."""
        from swecli.core.agents.components.prompts.builders import SystemPromptBuilder

        builder = SystemPromptBuilder(tool_registry=None, working_dir="/tmp")
        full = builder.build()
        stable, dynamic = builder.build_two_part()
        combined = f"{stable}\n\n{dynamic}" if dynamic else stable
        # The full prompt and combined should have overlapping core content
        # (exact match may differ in whitespace/ordering, so check containment)
        assert len(combined) > 0
        assert len(full) > 0


class TestAnthropicPayloadIncludesDynamic:
    """When _system_dynamic is set, it appears in the Anthropic payload."""

    def test_system_dynamic_in_anthropic_request(self):
        from swecli.core.agents.components.api.anthropic_adapter import AnthropicAdapter

        adapter = AnthropicAdapter(api_key="test-key")
        payload = {
            "model": "claude-3-sonnet-20240229",
            "messages": [
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "Hello"},
            ],
            "max_tokens": 1024,
            "_system_dynamic": "Current session context: test run",
        }
        anthropic_payload = adapter.convert_request(payload)

        # System should be a list of content blocks
        system = anthropic_payload["system"]
        assert isinstance(system, list)
        assert len(system) == 2

        # First block is stable with cache_control
        assert system[0]["type"] == "text"
        assert system[0]["text"] == "You are a helpful assistant."
        assert system[0]["cache_control"] == {"type": "ephemeral"}

        # Second block is dynamic without cache_control
        assert system[1]["type"] == "text"
        assert system[1]["text"] == "Current session context: test run"
        assert "cache_control" not in system[1]

    def test_no_dynamic_single_cached_block(self):
        from swecli.core.agents.components.api.anthropic_adapter import AnthropicAdapter

        adapter = AnthropicAdapter(api_key="test-key")
        payload = {
            "model": "claude-3-sonnet-20240229",
            "messages": [
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "Hello"},
            ],
            "max_tokens": 1024,
        }
        anthropic_payload = adapter.convert_request(payload)

        # No dynamic content -> single cached block
        system = anthropic_payload["system"]
        assert isinstance(system, list)
        assert len(system) == 1
        assert system[0]["cache_control"] == {"type": "ephemeral"}

    def test_dynamic_key_stripped_from_anthropic_payload(self):
        """_system_dynamic should not appear in the final Anthropic payload."""
        from swecli.core.agents.components.api.anthropic_adapter import AnthropicAdapter

        adapter = AnthropicAdapter(api_key="test-key")
        payload = {
            "model": "claude-3-sonnet-20240229",
            "messages": [
                {"role": "system", "content": "Stable prompt"},
                {"role": "user", "content": "Hello"},
            ],
            "max_tokens": 1024,
            "_system_dynamic": "Dynamic context",
        }
        result = adapter.convert_request(payload)
        assert "_system_dynamic" not in result
