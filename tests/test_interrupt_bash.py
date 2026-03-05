"""Test interrupt functionality for bash commands."""

import threading
import time
from pathlib import Path

from swecli.core.runtime.monitoring import TaskMonitor
from swecli.models.config import AppConfig
from swecli.core.context_engineering.tools.implementations import BashTool


def test_bash_interrupt():
    """Test that ESC interrupts a long-running bash command."""
    # Create bash tool
    config = AppConfig()
    config.permissions.bash.enabled = True
    bash_tool = BashTool(config, working_dir=Path.cwd())

    # Create task monitor
    task_monitor = TaskMonitor()
    task_monitor.start("Testing interrupt", initial_tokens=0)

    # Schedule interrupt after 1 second
    def trigger_interrupt():
        time.sleep(1)
        print("\n🔴 Simulating ESC key press (requesting interrupt)...")
        task_monitor.request_interrupt()

    interrupt_thread = threading.Thread(target=trigger_interrupt, daemon=True)
    interrupt_thread.start()

    # Execute long-running command (sleep 30)
    print("⏳ Starting long-running command: sleep 30")
    start = time.time()
    result = bash_tool.execute(
        "sleep 30",
        timeout=60,
        task_monitor=task_monitor,
    )
    elapsed = time.time() - start

    # Verify interrupt worked
    print(f"\n✅ Command completed in {elapsed:.1f}s")
    print(f"   Success: {result.success}")
    print(f"   Exit code: {result.exit_code}")
    print(f"   Error: {result.error}")

    # Stop task monitor
    stats = task_monitor.stop()
    print(f"   Interrupted: {stats['interrupted']}")

    # Assertions
    assert not result.success, "Command should have failed due to interrupt"
    assert result.error == "Command interrupted by user", f"Expected interrupt error, got: {result.error}"
    assert elapsed < 5, f"Command should have been interrupted in ~1s, but took {elapsed:.1f}s"
    assert result.exit_code == -1, f"Expected exit code -1, got {result.exit_code}"

    print("\n🎉 Test passed! Interrupt is working correctly.")


if __name__ == "__main__":
    test_bash_interrupt()
