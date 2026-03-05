"""Test the interrupt flow from ESC key press to task monitor."""

import threading
import time
from unittest.mock import Mock, MagicMock, patch

from swecli.core.runtime.monitoring import TaskMonitor
from swecli.repl.llm_caller import LLMCaller
from swecli.repl.query_processor import QueryProcessor


def test_task_monitor_interrupt():
    """Test that TaskMonitor correctly handles interrupt requests."""
    monitor = TaskMonitor()
    monitor.start("Testing", initial_tokens=0)

    # Initially should not be interrupted
    assert not monitor.should_interrupt()

    # Request interrupt
    monitor.request_interrupt()

    # Now should be interrupted
    assert monitor.should_interrupt()


def test_llm_caller_interrupt():
    """Test that LLMCaller correctly propagates interrupt to task monitor."""
    console = Mock()
    llm_caller = LLMCaller(console)

    # Initially no task monitor
    assert llm_caller._current_task_monitor is None
    assert not llm_caller.request_interrupt()

    # Set task monitor
    task_monitor = TaskMonitor()
    task_monitor.start("Testing", initial_tokens=0)
    llm_caller._current_task_monitor = task_monitor

    # Request interrupt
    result = llm_caller.request_interrupt()

    # Should have interrupted
    assert result is True
    assert task_monitor.should_interrupt()


def test_query_processor_interrupt_via_llm_caller():
    """Test that QueryProcessor correctly propagates interrupt to llm_caller."""
    console = Mock()
    session_manager = Mock()
    config = Mock()
    config.playbook_strategies = []
    config_manager = Mock()
    mode_manager = Mock()
    file_ops = Mock()
    output_formatter = Mock()
    status_line = Mock()
    message_printer = Mock()

    qp = QueryProcessor(
        console=console,
        session_manager=session_manager,
        config=config,
        config_manager=config_manager,
        mode_manager=mode_manager,
        file_ops=file_ops,
        output_formatter=output_formatter,
        status_line=status_line,
        message_printer_callback=message_printer,
    )

    # Verify _llm_caller exists
    assert qp._llm_caller is not None

    # Set a task monitor on the llm_caller (simulating active LLM call)
    task_monitor = TaskMonitor()
    task_monitor.start("Testing", initial_tokens=0)
    qp._llm_caller._current_task_monitor = task_monitor

    # Request interrupt
    result = qp.request_interrupt()

    # Should have interrupted
    assert result is True
    assert task_monitor.should_interrupt()


def test_interrupt_flow_during_llm_call():
    """Test that interrupt works during an actual simulated LLM call.

    This test verifies that when request_interrupt() is called,
    the task_monitor.should_interrupt() returns True.
    """
    # Create task monitor
    task_monitor = TaskMonitor()
    task_monitor.start("Testing", initial_tokens=0)

    # Track if interrupt was detected
    interrupt_detected = [False]

    # Simulate slow operation in background thread
    def slow_operation():
        for _ in range(50):  # 5 seconds at 100ms intervals
            time.sleep(0.1)
            if task_monitor.should_interrupt():
                interrupt_detected[0] = True
                return True  # Interrupted
        return False  # Completed without interrupt

    op_thread = threading.Thread(target=slow_operation)
    op_thread.start()

    # Wait a bit for the operation to start
    time.sleep(0.3)

    # Request interrupt
    task_monitor.request_interrupt()

    # Wait for thread to finish
    op_thread.join(timeout=2)

    # Verify interrupt was detected
    assert interrupt_detected[0], "Interrupt should have been detected"


def test_react_executor_uses_same_llm_caller():
    """Test that ReactExecutor uses the same llm_caller instance as QueryProcessor."""
    console = Mock()
    session_manager = Mock()
    config = Mock()
    config.playbook_strategies = []
    config_manager = Mock()
    mode_manager = Mock()
    file_ops = Mock()
    output_formatter = Mock()
    status_line = Mock()
    message_printer = Mock()

    qp = QueryProcessor(
        console=console,
        session_manager=session_manager,
        config=config,
        config_manager=config_manager,
        mode_manager=mode_manager,
        file_ops=file_ops,
        output_formatter=output_formatter,
        status_line=status_line,
        message_printer_callback=message_printer,
    )

    # Verify they share the same llm_caller
    assert qp._llm_caller is qp._react_executor._llm_caller
    print(f"QueryProcessor._llm_caller id: {id(qp._llm_caller)}")
    print(f"ReactExecutor._llm_caller id: {id(qp._react_executor._llm_caller)}")


def test_query_processor_interrupt_via_react_executor():
    """Test that QueryProcessor correctly propagates interrupt to react_executor."""
    console = Mock()
    session_manager = Mock()
    config = Mock()
    config.playbook_strategies = []
    config_manager = Mock()
    mode_manager = Mock()
    file_ops = Mock()
    output_formatter = Mock()
    status_line = Mock()
    message_printer = Mock()

    qp = QueryProcessor(
        console=console,
        session_manager=session_manager,
        config=config,
        config_manager=config_manager,
        mode_manager=mode_manager,
        file_ops=file_ops,
        output_formatter=output_formatter,
        status_line=status_line,
        message_printer_callback=message_printer,
    )

    # Verify _react_executor exists
    assert qp._react_executor is not None

    # Set a task monitor on the react_executor (simulating active thinking phase)
    task_monitor = TaskMonitor()
    task_monitor.start("Testing", initial_tokens=0)
    qp._react_executor._current_task_monitor = task_monitor

    # Request interrupt
    result = qp.request_interrupt()

    # Should have interrupted
    assert result is True
    assert task_monitor.should_interrupt()


if __name__ == "__main__":
    print("Running test_task_monitor_interrupt...")
    test_task_monitor_interrupt()
    print("PASSED\n")

    print("Running test_llm_caller_interrupt...")
    test_llm_caller_interrupt()
    print("PASSED\n")

    print("Running test_query_processor_interrupt_via_llm_caller...")
    test_query_processor_interrupt_via_llm_caller()
    print("PASSED\n")

    print("Running test_interrupt_flow_during_llm_call...")
    test_interrupt_flow_during_llm_call()
    print("PASSED\n")

    print("Running test_react_executor_uses_same_llm_caller...")
    test_react_executor_uses_same_llm_caller()
    print("PASSED\n")

    print("Running test_query_processor_interrupt_via_react_executor...")
    test_query_processor_interrupt_via_react_executor()
    print("PASSED\n")

    print("All tests passed!")
