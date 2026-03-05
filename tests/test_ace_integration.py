"""Tests for ACE (Agentic Context Engine) integration."""

import pytest
from swecli.core.context_engineering.memory import (
    Playbook,
    Bullet,
    AgentResponse,
    Reflector,
    Curator,
)
from swecli.models.session import Session


class TestACEIntegration:
    """Test ACE components integration with swecli."""

    def test_playbook_creation(self):
        """Test creating and using ACE Playbook."""
        playbook = Playbook()
        assert len(playbook.bullets()) == 0

        # Add a bullet
        bullet = playbook.add_bullet(
            section="file_operations",
            content="List directory before reading files"
        )
        assert len(playbook.bullets()) == 1
        assert bullet.content == "List directory before reading files"
        assert bullet.section == "file_operations"

    def test_playbook_serialization(self):
        """Test Playbook can be serialized to/from dict for Session storage."""
        playbook = Playbook()
        playbook.add_bullet(
            section="testing",
            content="Run tests after code changes",
            metadata={"helpful": 5, "harmful": 0}
        )

        # Serialize
        data = playbook.to_dict()
        assert isinstance(data, dict)
        assert "bullets" in data
        assert "sections" in data

        # Deserialize
        loaded = Playbook.from_dict(data)
        assert len(loaded.bullets()) == 1
        bullet = loaded.bullets()[0]
        assert bullet.content == "Run tests after code changes"
        assert bullet.helpful == 5

    def test_session_playbook_integration(self):
        """Test Session can store and load ACE Playbook."""
        session = Session()

        # Get empty playbook
        playbook = session.get_playbook()
        assert len(playbook.bullets()) == 0

        # Add strategies
        playbook.add_bullet("code_review", "Check for type safety")
        playbook.add_bullet("git", "Review git status before commits")

        # Save to session
        session.update_playbook(playbook)

        # Verify serialization
        assert session.playbook is not None
        assert isinstance(session.playbook, dict)

        # Load back
        loaded_playbook = session.get_playbook()
        assert len(loaded_playbook.bullets()) == 2

    def test_bullet_tagging(self):
        """Test bullet tagging for effectiveness tracking."""
        playbook = Playbook()
        bullet = playbook.add_bullet("test", "Test strategy")

        # Tag as helpful
        playbook.tag_bullet(bullet.id, "helpful")
        updated = playbook.get_bullet(bullet.id)
        assert updated.helpful == 1
        assert updated.harmful == 0

        # Tag as harmful
        playbook.tag_bullet(bullet.id, "harmful")
        updated = playbook.get_bullet(bullet.id)
        assert updated.harmful == 1

    def test_playbook_as_prompt(self):
        """Test playbook formatting for system prompt."""
        playbook = Playbook()
        playbook.add_bullet("file_ops", "List before read", metadata={"helpful": 3})
        playbook.add_bullet("testing", "Run tests after changes", metadata={"helpful": 5})

        prompt = playbook.as_prompt()
        assert "file_ops" in prompt.lower()
        assert "List before read" in prompt
        assert "helpful=" in prompt

    def test_native_ace_roles_direct_integration(self):
        """Test native ACE roles work directly with swecli LLM client."""
        # Mock swecli client
        class MockSwecliClient:
            model_name = "test-model"

            def chat_completion(self, messages):
                return {"content": "Test response"}

        mock_client = MockSwecliClient()

        # Test that native ACE roles can be initialized with swecli client
        reflector = Reflector(mock_client)
        curator = Curator(mock_client)

        assert reflector.llm_client is mock_client
        assert curator.llm_client is mock_client

    def test_agent_response_creation(self):
        """Test AgentResponse dataclass works correctly."""
        response = AgentResponse(
            content="Test response content",
            tool_calls=[{"function": {"name": "read_file"}}]
        )

        assert response.content == "Test response content"
        assert len(response.tool_calls) == 1
        assert response.tool_calls[0]["function"]["name"] == "read_file"

    def test_playbook_delta_operations(self):
        """Test delta operations for playbook evolution."""
        from swecli.core.context_engineering.memory import DeltaOperation, DeltaBatch

        playbook = Playbook()
        bullet = playbook.add_bullet("test", "Original content")

        # Create delta batch with operations
        delta = DeltaBatch(
            reasoning="Testing delta operations",
            operations=[
                DeltaOperation(
                    type="ADD",
                    section="new_section",
                    content="New strategy"
                ),
                DeltaOperation(
                    type="TAG",
                    section="test",
                    bullet_id=bullet.id,
                    metadata={"helpful": 1}
                )
            ]
        )

        # Apply delta
        playbook.apply_delta(delta)

        # Verify changes
        assert len(playbook.bullets()) == 2  # Original + new
        tagged_bullet = playbook.get_bullet(bullet.id)
        assert tagged_bullet.helpful == 1


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
