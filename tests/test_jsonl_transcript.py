"""Tests for JSONL transcript format."""

import json
import tempfile
from pathlib import Path

import pytest

from swecli.core.context_engineering.history.session_manager import SessionManager
from swecli.models.message import ChatMessage, Role


# Module-level function for multiprocessing (must be picklable)
def _append_message_subprocess(session_dir: str, session_id: str, content: str):
    """Append a message in a separate process."""
    manager = SessionManager(session_dir=Path(session_dir))
    msg = ChatMessage(role=Role.USER, content=content)
    manager.append_message_to_transcript(session_id, msg)


@pytest.fixture
def temp_session_dir():
    """Create a temporary session directory."""
    with tempfile.TemporaryDirectory() as tmpdir:
        yield Path(tmpdir)


def test_save_session_creates_jsonl_and_json(temp_session_dir):
    """Test that saving creates both .json (metadata) and .jsonl (messages) files."""
    manager = SessionManager(session_dir=temp_session_dir)
    session = manager.create_session(working_directory="/test")

    session.add_message(ChatMessage(role=Role.USER, content="Hello"))
    session.add_message(ChatMessage(role=Role.ASSISTANT, content="Hi there!"))

    manager.save_session(session, use_jsonl=True)

    # Both files should exist
    json_file = temp_session_dir / f"{session.id}.json"
    jsonl_file = temp_session_dir / f"{session.id}.jsonl"

    assert json_file.exists()
    assert jsonl_file.exists()

    # JSON file should not contain messages (key is removed in JSONL mode)
    with open(json_file) as f:
        json_data = json.load(f)
    assert "messages" not in json_data or json_data.get("messages") == []

    # JSONL file should contain 2 messages (one per line)
    with open(jsonl_file) as f:
        lines = f.readlines()
    assert len(lines) == 2


def test_append_message_to_transcript(temp_session_dir):
    """Test appending individual messages to JSONL transcript."""
    manager = SessionManager(session_dir=temp_session_dir)
    session = manager.create_session(working_directory="/test")
    session.add_message(ChatMessage(role=Role.USER, content="Hello"))

    # Save initial session
    manager.save_session(session, use_jsonl=True)

    # Append new message directly to transcript (concurrent-safe)
    new_msg = ChatMessage(role=Role.ASSISTANT, content="Response")
    manager.append_message_to_transcript(session.id, new_msg)

    # Load transcript
    messages = manager.load_transcript(session.id)

    assert len(messages) == 2
    assert messages[0].content == "Hello"
    assert messages[1].content == "Response"


def test_load_transcript_from_jsonl(temp_session_dir):
    """Test loading messages from JSONL transcript."""
    manager = SessionManager(session_dir=temp_session_dir)
    session = manager.create_session(working_directory="/test")

    session.add_message(ChatMessage(role=Role.USER, content="Message 1"))
    session.add_message(ChatMessage(role=Role.ASSISTANT, content="Message 2"))
    session.add_message(ChatMessage(role=Role.USER, content="Message 3"))

    manager.save_session(session, use_jsonl=True)

    # Load transcript
    messages = manager.load_transcript(session.id)

    assert len(messages) == 3
    assert messages[0].content == "Message 1"
    assert messages[1].content == "Message 2"
    assert messages[2].content == "Message 3"


def test_load_session_from_jsonl_format(temp_session_dir):
    """Test loading a complete session from JSONL format."""
    manager = SessionManager(session_dir=temp_session_dir)
    session = manager.create_session(
        working_directory="/test", channel="telegram", channel_user_id="@alice"
    )

    session.add_message(ChatMessage(role=Role.USER, content="Test"))
    manager.save_session(session, use_jsonl=True)

    # Load session
    loaded = manager.load_session(session.id)

    assert loaded.id == session.id
    assert loaded.working_directory == "/test"
    assert loaded.channel == "telegram"
    assert loaded.channel_user_id == "@alice"
    assert len(loaded.messages) == 1
    assert loaded.messages[0].content == "Test"


def test_load_legacy_json_format(temp_session_dir):
    """Test that legacy .json format (full session) still works."""
    manager = SessionManager(session_dir=temp_session_dir)
    session = manager.create_session(working_directory="/test")

    session.add_message(ChatMessage(role=Role.USER, content="Legacy"))
    manager.save_session(session, use_jsonl=False)  # Use legacy format

    # Load session
    loaded = manager.load_session(session.id)

    assert loaded.id == session.id
    assert len(loaded.messages) == 1
    assert loaded.messages[0].content == "Legacy"


def test_migrate_json_to_jsonl(temp_session_dir):
    """Test migration from legacy JSON to JSONL format."""
    manager = SessionManager(session_dir=temp_session_dir)

    # Create sessions in legacy format
    session1 = manager.create_session(working_directory="/test1")
    session1.add_message(ChatMessage(role=Role.USER, content="Session 1"))
    manager.save_session(session1, use_jsonl=False)

    session2 = manager.create_session(working_directory="/test2")
    session2.add_message(ChatMessage(role=Role.USER, content="Session 2"))
    manager.save_session(session2, use_jsonl=False)

    # Verify only .json files exist
    assert (temp_session_dir / f"{session1.id}.json").exists()
    assert not (temp_session_dir / f"{session1.id}.jsonl").exists()

    # Run migration
    migrated_count = manager.migrate_json_to_jsonl()

    assert migrated_count == 2

    # Verify .jsonl files now exist
    assert (temp_session_dir / f"{session1.id}.jsonl").exists()
    assert (temp_session_dir / f"{session2.id}.jsonl").exists()

    # Verify backup files exist
    assert (temp_session_dir / f"{session1.id}.json.bak").exists()

    # Verify sessions can still be loaded
    loaded1 = manager.load_session(session1.id)
    assert loaded1.messages[0].content == "Session 1"


def test_jsonl_handles_corrupted_lines(temp_session_dir):
    """Test that load_transcript skips corrupted lines gracefully."""
    manager = SessionManager(session_dir=temp_session_dir)
    session = manager.create_session(working_directory="/test")
    session.add_message(ChatMessage(role=Role.USER, content="Good message"))
    manager.save_session(session, use_jsonl=True)

    # Manually corrupt the JSONL file
    jsonl_file = temp_session_dir / f"{session.id}.jsonl"
    with open(jsonl_file, "a") as f:
        f.write("{ invalid json }\n")
        f.write('{"valid": "but incomplete"\n')

    # Add another good message
    manager.append_message_to_transcript(
        session.id, ChatMessage(role=Role.ASSISTANT, content="Another good one")
    )

    # Should load only valid messages
    messages = manager.load_transcript(session.id)

    assert len(messages) == 2
    assert messages[0].content == "Good message"
    assert messages[1].content == "Another good one"


def test_concurrent_append_to_same_transcript(temp_session_dir):
    """Test that concurrent appends to same transcript are safe (via locking)."""
    from multiprocessing import Process

    manager = SessionManager(session_dir=temp_session_dir)
    session = manager.create_session(working_directory="/test")
    manager.save_session(session, use_jsonl=True)

    # Spawn multiple processes that append concurrently
    processes = []
    for i in range(5):
        p = Process(
            target=_append_message_subprocess,
            args=(str(temp_session_dir), session.id, f"Message {i}"),
        )
        p.start()
        processes.append(p)

    for p in processes:
        p.join(timeout=10)

    # All messages should be in the transcript
    messages = manager.load_transcript(session.id)
    assert len(messages) == 5

    # All messages should be intact (not corrupted)
    contents = {msg.content for msg in messages}
    assert len(contents) == 5  # No duplicates or corruption
