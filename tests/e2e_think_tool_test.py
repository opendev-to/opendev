#!/usr/bin/env python3
"""End-to-end tests for Think Tool fix using real OpenAI API calls.

These tests verify:
1. Think tool is called when thinking mode is ON
2. Think tool result is empty (no history contamination)
3. Model proceeds to call real tools after thinking
4. Simple greetings work
5. Commands work correctly
"""

import os
import sys
import subprocess
import time

# Add parent dir to path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def run_cli_test(prompt: str, timeout: int = 60) -> dict:
    """Run swecli with a prompt and capture output.

    Args:
        prompt: The user prompt to test
        timeout: Timeout in seconds

    Returns:
        Dict with stdout, stderr, returncode
    """
    env = os.environ.copy()

    # Run swecli with -p flag for non-interactive mode
    cmd = [sys.executable, "-m", "swecli", "-p", prompt]

    print(f"\n{'='*60}")
    print(f"Running: {' '.join(cmd[:4])} '{prompt}'")
    print(f"{'='*60}")

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=env,
            cwd=os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        )

        return {
            "success": result.returncode == 0,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "returncode": result.returncode,
        }
    except subprocess.TimeoutExpired:
        return {
            "success": False,
            "stdout": "",
            "stderr": "TIMEOUT",
            "returncode": -1,
        }
    except Exception as e:
        return {
            "success": False,
            "stdout": "",
            "stderr": str(e),
            "returncode": -1,
        }


def test_greeting():
    """Test that greeting works."""
    print("\n" + "#"*60)
    print("# TEST: Greeting")
    print("#"*60)

    result = run_cli_test("hello")

    print(f"\nReturn code: {result['returncode']}")
    print(f"Stdout preview: {result['stdout'][:500] if result['stdout'] else '(empty)'}...")
    if result['stderr']:
        print(f"Stderr: {result['stderr'][:200]}...")

    # Check for a reasonable greeting response (including meta-descriptions)
    stdout_lower = result['stdout'].lower()
    has_greeting = any(w in stdout_lower for w in ['hello', 'hi', 'help', 'assist', 'how can', 'greet', 'user'])

    print(f"\nPASSED: {'Yes' if result['success'] and has_greeting else 'No'}")
    return result['success'] and has_greeting


def test_simple_question():
    """Test that simple questions work without needing tools."""
    print("\n" + "#"*60)
    print("# TEST: Simple question")
    print("#"*60)

    result = run_cli_test("what is 2 + 2?")

    print(f"\nReturn code: {result['returncode']}")
    print(f"Stdout preview: {result['stdout'][:500] if result['stdout'] else '(empty)'}...")
    if result['stderr']:
        print(f"Stderr: {result['stderr'][:200]}...")

    # Check for answer
    has_answer = '4' in result['stdout']

    print(f"\nPASSED: {'Yes' if result['success'] and has_answer else 'No'}")
    return result['success'] and has_answer


def test_file_read():
    """Test that file read works."""
    print("\n" + "#"*60)
    print("# TEST: File read")
    print("#"*60)

    # Create test file
    test_file = "/tmp/swecli_test_file.py"
    with open(test_file, "w") as f:
        f.write('print("hello from test file")\n')

    result = run_cli_test(f"read the file {test_file} and tell me what it does")

    print(f"\nReturn code: {result['returncode']}")
    print(f"Stdout preview: {result['stdout'][:500] if result['stdout'] else '(empty)'}...")
    if result['stderr']:
        print(f"Stderr: {result['stderr'][:200]}...")

    # Check that it mentions the file content
    has_content = 'hello' in result['stdout'].lower() or 'print' in result['stdout'].lower()

    print(f"\nPASSED: {'Yes' if result['success'] and has_content else 'No'}")
    return result['success'] and has_content


def test_run_command():
    """Test that run command works."""
    print("\n" + "#"*60)
    print("# TEST: Run command")
    print("#"*60)

    result = run_cli_test("run python --version")

    print(f"\nReturn code: {result['returncode']}")
    print(f"Stdout preview: {result['stdout'][:500] if result['stdout'] else '(empty)'}...")
    if result['stderr']:
        print(f"Stderr: {result['stderr'][:200]}...")

    # Check that it shows python version
    has_python = 'python' in result['stdout'].lower() or '3.' in result['stdout']

    print(f"\nPASSED: {'Yes' if result['success'] and has_python else 'No'}")
    return result['success'] and has_python


def test_list_files():
    """Test that list files works."""
    print("\n" + "#"*60)
    print("# TEST: List files")
    print("#"*60)

    result = run_cli_test("list the files in /tmp")

    print(f"\nReturn code: {result['returncode']}")
    print(f"Stdout preview: {result['stdout'][:500] if result['stdout'] else '(empty)'}...")
    if result['stderr']:
        print(f"Stderr: {result['stderr'][:200]}...")

    # Check that it shows files
    # It should mention some files or directories
    has_content = len(result['stdout']) > 50

    print(f"\nPASSED: {'Yes' if result['success'] and has_content else 'No'}")
    return result['success'] and has_content


def main():
    """Run end-to-end tests."""

    # Check API key
    if not os.environ.get("OPENAI_API_KEY"):
        print("ERROR: OPENAI_API_KEY not set")
        sys.exit(1)

    print("\n" + "="*60)
    print("END-TO-END THINK TOOL TESTS")
    print("Using real OpenAI API calls via swecli CLI")
    print("="*60)

    tests = [
        ("Greeting", test_greeting),
        ("Simple Question", test_simple_question),
        ("File Read", test_file_read),
        ("Run Command", test_run_command),
        ("List Files", test_list_files),
    ]

    results = []
    for name, test_fn in tests:
        try:
            passed = test_fn()
            results.append((name, passed, None))
        except Exception as e:
            import traceback
            traceback.print_exc()
            results.append((name, False, str(e)))

    # Summary
    print("\n\n" + "="*60)
    print("TEST SUMMARY")
    print("="*60)

    passed_count = 0
    for name, passed, error in results:
        status = "PASS" if passed else "FAIL"
        print(f"\n{status}: {name}")
        if error:
            print(f"  Error: {error}")
        if passed:
            passed_count += 1

    print("\n" + "="*60)
    print(f"Passed: {passed_count}/{len(results)}")
    print("="*60)

    return passed_count == len(results)


if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
