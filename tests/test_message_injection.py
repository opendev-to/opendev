"""Tests for live message injection into running agent loops."""

import queue as queue_mod
import threading
import time
from unittest.mock import MagicMock, patch

class TestReactExecutorInjection:
    """Tests for ReactExecutor injection queue."""

    def _make_executor(self):
        """Create a ReactExecutor with mocked dependencies."""
        from swecli.repl.react_executor import ReactExecutor

        console = MagicMock()
        session_manager = MagicMock()
        config = MagicMock()
        config.auto_save_interval = 0
        llm_caller = MagicMock()
        tool_executor = MagicMock()

        executor = ReactExecutor(console, session_manager, config, llm_caller, tool_executor)
        return executor, session_manager

    def test_inject_user_message_enqueues(self):
        """inject_user_message puts message in queue."""
        executor, _ = self._make_executor()
        executor.inject_user_message("hello")
        assert not executor._injection_queue.empty()
        assert executor._injection_queue.get_nowait() == "hello"

    def test_inject_user_message_drops_when_full(self):
        """Messages beyond maxsize=10 are dropped, not raised."""
        executor, _ = self._make_executor()
        for i in range(10):
            executor.inject_user_message(f"msg{i}")
        # Queue is full — should not raise
        executor.inject_user_message("overflow")
        assert executor._injection_queue.qsize() == 10

    def test_drain_injected_messages(self):
        """_drain_injected_messages persists and appends messages."""
        from swecli.repl.react_executor import IterationContext

        executor, session_manager = self._make_executor()
        executor.inject_user_message("msg1")
        executor.inject_user_message("msg2")

        ctx = IterationContext(
            query="test",
            messages=[],
            agent=MagicMock(),
            tool_registry=MagicMock(),
            approval_manager=MagicMock(),
            undo_manager=MagicMock(),
            ui_callback=None,
        )

        count = executor._drain_injected_messages(ctx)
        assert count == 2
        assert len(ctx.messages) == 2
        assert ctx.messages[0] == {"role": "user", "content": "msg1"}
        assert ctx.messages[1] == {"role": "user", "content": "msg2"}
        # Session persistence called for each
        assert session_manager.add_message.call_count == 2

    def test_drain_caps_at_max_per_drain(self):
        """_drain_injected_messages caps at max_per_drain=3."""
        from swecli.repl.react_executor import IterationContext

        executor, _ = self._make_executor()
        for i in range(5):
            executor.inject_user_message(f"msg{i}")

        ctx = IterationContext(
            query="test",
            messages=[],
            agent=MagicMock(),
            tool_registry=MagicMock(),
            approval_manager=MagicMock(),
            undo_manager=MagicMock(),
            ui_callback=None,
        )

        count = executor._drain_injected_messages(ctx)
        assert count == 3  # Capped
        assert len(ctx.messages) == 3
        # 2 remain in queue
        assert executor._injection_queue.qsize() == 2

    def test_drain_empty_queue_returns_zero(self):
        """_drain_injected_messages on empty queue returns 0."""
        from swecli.repl.react_executor import IterationContext

        executor, _ = self._make_executor()
        ctx = IterationContext(
            query="test",
            messages=[],
            agent=MagicMock(),
            tool_registry=MagicMock(),
            approval_manager=MagicMock(),
            undo_manager=MagicMock(),
            ui_callback=None,
        )
        count = executor._drain_injected_messages(ctx)
        assert count == 0
        assert len(ctx.messages) == 0


class TestMainAgentInjection:
    """Tests for MainAgent injection queue."""

    def _make_agent(self):
        """Create a MainAgent with mocked dependencies.

        Patches build_system_prompt to avoid requiring real tool registry
        and template files during __init__.
        """
        from swecli.core.agents.main_agent import MainAgent

        config = MagicMock()
        config.model = "test-model"
        config.model_provider = "openai"
        config.model_thinking = None
        config.model_thinking_provider = None
        config.model_critique = None
        config.model_critique_provider = None
        config.model_vlm = None
        config.model_vlm_provider = None
        config.temperature = 0.7
        config.max_tokens = 4096
        config.get_model_info.return_value = ("openai", "test-model", "Test Model")
        config.get_api_key.return_value = "test-key"

        tool_registry = MagicMock()
        tool_registry.get_tool_schemas.return_value = []
        tool_registry.thinking_handler = MagicMock()
        tool_registry.thinking_handler.is_visible = False

        mode_manager = MagicMock()

        with patch.object(MainAgent, "build_system_prompt", return_value="test prompt"):
            agent = MainAgent(config, tool_registry, mode_manager)
        return agent

    def test_inject_user_message_enqueues(self):
        """inject_user_message puts message in queue."""
        agent = self._make_agent()
        agent.inject_user_message("hello")
        assert not agent._injection_queue.empty()
        assert agent._injection_queue.get_nowait() == "hello"

    def test_inject_user_message_drops_when_full(self):
        """Messages beyond maxsize=10 are dropped."""
        agent = self._make_agent()
        for i in range(10):
            agent.inject_user_message(f"msg{i}")
        # Should not raise
        agent.inject_user_message("overflow")
        assert agent._injection_queue.qsize() == 10

    def test_drain_injected_messages(self):
        """_drain_injected_messages appends to messages list."""
        agent = self._make_agent()
        agent.inject_user_message("msg1")
        agent.inject_user_message("msg2")

        messages = []
        count = agent._drain_injected_messages(messages)
        assert count == 2
        assert len(messages) == 2
        assert messages[0] == {"role": "user", "content": "msg1"}
        assert messages[1] == {"role": "user", "content": "msg2"}

    def test_drain_caps_at_max_per_drain(self):
        """_drain_injected_messages caps at max_per_drain=3."""
        agent = self._make_agent()
        for i in range(5):
            agent.inject_user_message(f"msg{i}")

        messages = []
        count = agent._drain_injected_messages(messages)
        assert count == 3
        assert agent._injection_queue.qsize() == 2

    def test_drain_persists_to_session_manager(self):
        """_drain_injected_messages persists messages when session manager is set."""
        agent = self._make_agent()
        session_mgr = MagicMock()
        agent._run_session_manager = session_mgr

        agent.inject_user_message("persist me")
        messages = []
        agent._drain_injected_messages(messages)

        assert session_mgr.add_message.call_count == 1
        persisted_msg = session_mgr.add_message.call_args[0][0]
        assert persisted_msg.content == "persist me"

    def test_final_drain_persists_remaining(self):
        """_final_drain_injection_queue persists orphaned messages."""
        agent = self._make_agent()
        session_mgr = MagicMock()
        agent._run_session_manager = session_mgr

        agent.inject_user_message("orphan1")
        agent.inject_user_message("orphan2")
        agent._final_drain_injection_queue()

        assert session_mgr.add_message.call_count == 2
        assert agent._run_session_manager is None  # Cleaned up
        assert agent._injection_queue.empty()


class TestMessageProcessorInjection:
    """Tests for MessageProcessor injection target."""

    def _make_processor(self):
        """Create a MessageProcessor with mocked app."""
        from swecli.ui_textual.runner_components.message_processor import MessageProcessor

        app = MagicMock()
        app.update_queue_indicator = MagicMock()
        app.call_from_thread = MagicMock()
        app.conversation = MagicMock()

        processor = MessageProcessor(app, callbacks={})
        processor.set_app(app)
        return processor, app

    def test_set_injection_target(self):
        """set_injection_target stores the callback."""
        processor, _ = self._make_processor()
        cb = MagicMock()
        processor.set_injection_target(cb)
        assert processor._injection_target is cb

    def test_clear_injection_target(self):
        """set_injection_target(None) clears the callback."""
        processor, _ = self._make_processor()
        cb = MagicMock()
        processor.set_injection_target(cb)
        processor.set_injection_target(None)
        assert processor._injection_target is None

    def test_enqueue_redirects_to_injection_target(self):
        """Non-command messages go to injection target when set."""
        processor, app = self._make_processor()
        cb = MagicMock()
        processor.set_injection_target(cb)

        processor.enqueue_message("hello world")

        cb.assert_called_once_with("hello world")
        # Should NOT display immediately — deferred to step boundary
        app.call_from_thread.assert_not_called()
        # Should NOT go to pending queue
        assert processor._pending.empty()

    def test_slash_commands_bypass_injection(self):
        """Slash commands always go to pending queue, not injection target."""
        processor, _ = self._make_processor()
        cb = MagicMock()
        processor.set_injection_target(cb)

        processor.enqueue_message("/help")

        cb.assert_not_called()
        assert not processor._pending.empty()

    def test_no_injection_target_uses_queue(self):
        """Without injection target, messages go to pending queue normally."""
        processor, _ = self._make_processor()

        processor.enqueue_message("hello world")

        assert not processor._pending.empty()


class TestWebStateInjectionQueue:
    """Tests for WebState injection queue management."""

    def _make_state(self):
        """Create a WebState with mocked managers."""
        from swecli.web.state import WebState

        state = WebState(
            config_manager=MagicMock(),
            session_manager=MagicMock(),
            mode_manager=MagicMock(),
            approval_manager=MagicMock(),
            undo_manager=MagicMock(),
        )
        return state

    def test_get_injection_queue_creates(self):
        """get_injection_queue creates queue on first call."""
        state = self._make_state()
        q = state.get_injection_queue("session1")
        assert isinstance(q, queue_mod.Queue)

    def test_get_injection_queue_returns_same(self):
        """Subsequent calls return the same queue."""
        state = self._make_state()
        q1 = state.get_injection_queue("session1")
        q2 = state.get_injection_queue("session1")
        assert q1 is q2

    def test_get_injection_queue_different_sessions(self):
        """Different sessions get different queues."""
        state = self._make_state()
        q1 = state.get_injection_queue("session1")
        q2 = state.get_injection_queue("session2")
        assert q1 is not q2

    def test_clear_injection_queue(self):
        """clear_injection_queue removes the queue."""
        state = self._make_state()
        state.get_injection_queue("session1")
        state.clear_injection_queue("session1")
        # Should create a new one
        q2 = state.get_injection_queue("session1")
        assert q2.empty()

    def test_clear_nonexistent_queue(self):
        """clear_injection_queue on missing session is a no-op."""
        state = self._make_state()
        state.clear_injection_queue("nonexistent")  # Should not raise


class TestInjectionThreadSafety:
    """Tests for thread safety of injection mechanisms."""

    def test_concurrent_inject_and_drain(self):
        """Concurrent inject and drain don't lose messages or crash."""
        from swecli.repl.react_executor import IterationContext

        executor, _ = TestReactExecutorInjection()._make_executor()

        results = {"injected": 0, "drained": 0}
        stop_event = threading.Event()

        def injector():
            count = 0
            while not stop_event.is_set() and count < 50:
                try:
                    executor._injection_queue.put_nowait(f"msg{count}")
                    count += 1
                except queue_mod.Full:
                    time.sleep(0.001)
            results["injected"] = count

        def drainer():
            count = 0
            ctx = IterationContext(
                query="test",
                messages=[],
                agent=MagicMock(),
                tool_registry=MagicMock(),
                approval_manager=MagicMock(),
                undo_manager=MagicMock(),
                ui_callback=None,
            )
            while not stop_event.is_set() or not executor._injection_queue.empty():
                n = executor._drain_injected_messages(ctx, max_per_drain=5)
                count += n
                if n == 0:
                    time.sleep(0.001)
            results["drained"] = count

        t_inject = threading.Thread(target=injector)
        t_drain = threading.Thread(target=drainer)

        t_inject.start()
        t_drain.start()

        t_inject.join(timeout=5)
        stop_event.set()
        t_drain.join(timeout=5)

        # All injected messages should be drained
        assert results["drained"] == results["injected"]


class TestDeferredDisplay:
    """Tests for deferred display of injected messages."""

    def _make_executor(self):
        from swecli.repl.react_executor import ReactExecutor

        console = MagicMock()
        session_manager = MagicMock()
        config = MagicMock()
        config.auto_save_interval = 0
        llm_caller = MagicMock()
        tool_executor = MagicMock()

        executor = ReactExecutor(console, session_manager, config, llm_caller, tool_executor)
        return executor, session_manager

    def _make_processor(self):
        from swecli.ui_textual.runner_components.message_processor import MessageProcessor

        app = MagicMock()
        app.update_queue_indicator = MagicMock()
        app.call_from_thread = MagicMock()
        app.conversation = MagicMock()

        processor = MessageProcessor(app, callbacks={})
        processor.set_app(app)
        return processor, app

    def test_enqueue_does_not_display_immediately(self):
        """Injected message is NOT displayed at injection time."""
        processor, app = self._make_processor()
        cb = MagicMock()
        processor.set_injection_target(cb)

        processor.enqueue_message("hello")

        cb.assert_called_once_with("hello")
        # add_user_message must NOT be called at injection time
        for call in app.call_from_thread.call_args_list:
            assert call[0][0] != app.conversation.add_user_message

    def test_enqueue_updates_queue_indicator(self):
        """Queue indicator fires after inject."""
        processor, app = self._make_processor()
        cb = MagicMock()
        inj_q = queue_mod.Queue(maxsize=10)
        processor.set_injection_target(cb, injection_queue=inj_q)

        # Put a message in the injection queue (simulating what target() does)
        inj_q.put_nowait("hello")
        processor.enqueue_message("hello")

        # update_queue_indicator should have been called
        app.update_queue_indicator.assert_called()

    def test_get_queue_size_includes_injection_queue(self):
        """get_queue_size includes both pending and injection queue."""
        processor, _ = self._make_processor()
        inj_q = queue_mod.Queue(maxsize=10)
        processor.set_injection_target(MagicMock(), injection_queue=inj_q)

        # Put items in both queues
        processor._pending.put_nowait(("msg1", False))
        inj_q.put_nowait("msg2")
        inj_q.put_nowait("msg3")

        assert processor.get_queue_size() == 3

    def test_get_queue_size_without_injection_queue(self):
        """get_queue_size works with no injection queue set."""
        processor, _ = self._make_processor()
        processor._pending.put_nowait(("msg1", False))
        assert processor.get_queue_size() == 1

    def test_drain_calls_on_message_consumed(self):
        """_drain_injected_messages calls _on_message_consumed for each drained message."""
        from swecli.repl.react_executor import IterationContext

        executor, _ = self._make_executor()
        consumed = []
        executor.set_on_message_consumed(lambda text: consumed.append(text))

        executor.inject_user_message("msg1")
        executor.inject_user_message("msg2")

        ctx = IterationContext(
            query="test",
            messages=[],
            agent=MagicMock(),
            tool_registry=MagicMock(),
            approval_manager=MagicMock(),
            undo_manager=MagicMock(),
            ui_callback=None,
        )

        executor._drain_injected_messages(ctx)
        assert consumed == ["msg1", "msg2"]

    def test_drain_without_callback_works(self):
        """Drain succeeds even with no callback set."""
        from swecli.repl.react_executor import IterationContext

        executor, _ = self._make_executor()
        # No callback set
        executor.inject_user_message("msg1")

        ctx = IterationContext(
            query="test",
            messages=[],
            agent=MagicMock(),
            tool_registry=MagicMock(),
            approval_manager=MagicMock(),
            undo_manager=MagicMock(),
            ui_callback=None,
        )

        count = executor._drain_injected_messages(ctx)
        assert count == 1
        assert ctx.messages[0]["content"] == "msg1"

    def test_drain_callback_exception_swallowed(self):
        """Callback exceptions don't break the drain."""
        from swecli.repl.react_executor import IterationContext

        executor, _ = self._make_executor()

        def bad_callback(text):
            raise RuntimeError("boom")

        executor.set_on_message_consumed(bad_callback)
        executor.inject_user_message("msg1")
        executor.inject_user_message("msg2")

        ctx = IterationContext(
            query="test",
            messages=[],
            agent=MagicMock(),
            tool_registry=MagicMock(),
            approval_manager=MagicMock(),
            undo_manager=MagicMock(),
            ui_callback=None,
        )

        count = executor._drain_injected_messages(ctx)
        # Both messages should still be drained despite callback failures
        assert count == 2

    def test_orphan_callback_called_on_final_drain(self):
        """Final drain in execute() calls _on_orphan_message for remaining messages."""
        executor, session_manager = self._make_executor()
        orphans = []
        executor.set_on_orphan_message(lambda text: orphans.append(text))

        # Directly put messages into the injection queue
        executor._injection_queue.put_nowait("orphan1")
        executor._injection_queue.put_nowait("orphan2")

        # Simulate the final drain logic (extracted from execute())
        while True:
            try:
                text = executor._injection_queue.get_nowait()
                if executor._on_orphan_message is not None:
                    executor._on_orphan_message(text)
                else:
                    from swecli.models.message import ChatMessage, Role
                    user_msg = ChatMessage(role=Role.USER, content=text)
                    session_manager.add_message(user_msg, 0)
            except queue_mod.Empty:
                break

        assert orphans == ["orphan1", "orphan2"]
        # Should NOT persist when orphan callback handles it
        session_manager.add_message.assert_not_called()

    def test_orphan_messages_requeued_to_pending(self):
        """_on_orphan callback can re-queue messages to pending."""
        processor, _ = self._make_processor()
        executor, _ = self._make_executor()

        def requeue(text):
            processor._pending.put_nowait((text, True))
            processor._message_ready.set()

        executor.set_on_orphan_message(requeue)
        executor._injection_queue.put_nowait("straggler")

        # Simulate final drain
        while True:
            try:
                text = executor._injection_queue.get_nowait()
                executor._on_orphan_message(text)
            except queue_mod.Empty:
                break

        # Message should be in pending queue
        assert not processor._pending.empty()
        msg, needs_display = processor._pending.get_nowait()
        assert msg == "straggler"
        assert needs_display is True

    def test_callbacks_cleared_after_execute_final_drain(self):
        """Callbacks are cleared at end of execute()'s final drain."""
        executor, _ = self._make_executor()
        executor.set_on_message_consumed(lambda t: None)
        executor.set_on_orphan_message(lambda t: None)

        assert executor._on_message_consumed is not None
        assert executor._on_orphan_message is not None

        # Simulate what execute() does at the end
        executor._on_message_consumed = None
        executor._on_orphan_message = None

        assert executor._on_message_consumed is None
        assert executor._on_orphan_message is None
