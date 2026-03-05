#!/usr/bin/env python3
"""Standalone tests for path sanitization logic.

These tests verify the core path sanitization functions work correctly
without requiring the full swecli module imports.
"""

import re
import sys
from pathlib import Path


def _sanitize_local_paths(arguments: dict) -> dict:
    """Copy of _sanitize_local_paths from DockerToolRegistry."""
    sanitized = dict(arguments)
    for key, value in sanitized.items():
        if isinstance(value, str):
            # Match absolute paths starting with /Users/, /home/, /var/, etc.
            match = re.match(r'^(/Users/|/home/|/var/|/tmp/).+/([^/]+)$', value)
            if match:
                filename = match.group(2)
                sanitized[key] = filename
    return sanitized


def _translate_path(path: str, workspace_dir: str = "/workspace") -> str:
    """Copy of _translate_path from DockerToolHandler."""
    if not path:
        return workspace_dir

    # If it's already a container path, use as-is
    if path.startswith("/testbed") or path.startswith("/workspace"):
        return path

    # Relative path - prepend workspace (strip leading ./)
    if not path.startswith("/"):
        clean_path = path.lstrip("./")
        return f"{workspace_dir}/{clean_path}"

    # Absolute host path (e.g., /Users/.../file.py)
    # Extract just the filename
    try:
        p = Path(path)
        return f"{workspace_dir}/{p.name}"
    except Exception:
        pass

    # Fallback: just use the path as-is under workspace
    return f"{workspace_dir}/{path}"


def _rewrite_task_for_docker(
    task: str,
    input_files: list,
    workspace_dir: str,
    local_working_dir: str = "/Users/nghibui/codes/test_opencli"
) -> str:
    """Copy of _rewrite_task_for_docker from SubAgentManager."""
    new_task = task

    # Remove phrases that hint at local filesystem
    new_task = re.sub(r'\blocal\s+', '', new_task, flags=re.IGNORECASE)
    new_task = re.sub(r'\bin this repo\b', f'in {workspace_dir}', new_task, flags=re.IGNORECASE)
    new_task = re.sub(r'\bthis repo\b', workspace_dir, new_task, flags=re.IGNORECASE)

    # Replace any reference to the local working directory with workspace
    if local_working_dir:
        local_dir_str = str(local_working_dir)
        new_task = new_task.replace(local_dir_str, workspace_dir)
        new_task = new_task.replace(local_dir_str.rstrip("/"), workspace_dir)

    # Replace file references with Docker paths
    for local_file in input_files:
        if isinstance(local_file, str):
            local_file = Path(local_file)
        docker_path = f"{workspace_dir}/{local_file.name}"
        # Replace @filename references
        new_task = new_task.replace(f"@{local_file.name}", docker_path)
        # Replace full path references
        new_task = new_task.replace(str(local_file), docker_path)
        # Replace just the filename as a word
        new_task = re.sub(rf'\b{re.escape(local_file.name)}\b', docker_path, new_task)

    # Add Docker context preamble
    docker_context = f"""## CRITICAL: Docker Environment

YOU ARE RUNNING INSIDE A DOCKER CONTAINER.

Working directory: {workspace_dir}
All file paths MUST be relative (e.g., `file.py`, `src/file.py`) or start with {workspace_dir}/.

NEVER use paths like:
- /Users/...
- /home/...
- Any absolute path outside {workspace_dir}

ALWAYS use paths like:
- pyproject.toml
- src/model.py
- config.yaml

Use ONLY the filename or relative path for all file operations.

"""
    return docker_context + new_task


# ============================================================================
# Tests
# ============================================================================

def test_sanitize_users_path():
    """Test that /Users/... paths are sanitized to just filename."""
    args = {"path": "/Users/nghibui/codes/test_opencli/pyproject.toml"}
    result = _sanitize_local_paths(args)
    assert result["path"] == "pyproject.toml", f"Expected 'pyproject.toml', got '{result['path']}'"
    print("✓ test_sanitize_users_path")


def test_sanitize_home_path():
    """Test that /home/... paths are sanitized to just filename."""
    args = {"file_path": "/home/user/project/src/model.py"}
    result = _sanitize_local_paths(args)
    assert result["file_path"] == "model.py", f"Expected 'model.py', got '{result['file_path']}'"
    print("✓ test_sanitize_home_path")


def test_sanitize_var_path():
    """Test that /var/... paths are sanitized to just filename."""
    args = {"path": "/var/tmp/data/config.yaml"}
    result = _sanitize_local_paths(args)
    assert result["path"] == "config.yaml", f"Expected 'config.yaml', got '{result['path']}'"
    print("✓ test_sanitize_var_path")


def test_sanitize_tmp_path():
    """Test that /tmp/... paths are sanitized to just filename."""
    args = {"path": "/tmp/working/file.txt"}
    result = _sanitize_local_paths(args)
    assert result["path"] == "file.txt", f"Expected 'file.txt', got '{result['path']}'"
    print("✓ test_sanitize_tmp_path")


def test_preserve_relative_path():
    """Test that relative paths are preserved."""
    args = {"path": "src/model.py"}
    result = _sanitize_local_paths(args)
    assert result["path"] == "src/model.py", f"Expected 'src/model.py', got '{result['path']}'"
    print("✓ test_preserve_relative_path")


def test_preserve_workspace_path():
    """Test that /workspace/... paths are preserved."""
    args = {"path": "/workspace/src/model.py"}
    result = _sanitize_local_paths(args)
    assert result["path"] == "/workspace/src/model.py", f"Expected '/workspace/src/model.py', got '{result['path']}'"
    print("✓ test_preserve_workspace_path")


def test_sanitize_pdf_path():
    """Test sanitizing PDF file paths."""
    args = {"path": "/Users/nghibui/codes/test_opencli/2303.11366v4.pdf"}
    result = _sanitize_local_paths(args)
    assert result["path"] == "2303.11366v4.pdf", f"Expected '2303.11366v4.pdf', got '{result['path']}'"
    print("✓ test_sanitize_pdf_path")


def test_sanitize_multiple_args():
    """Test sanitizing multiple path arguments."""
    args = {
        "file_path": "/Users/nghibui/codes/test/main.py",
        "content": "print('hello')",  # Non-path, should be preserved
        "output_path": "/home/user/output.txt",
    }
    result = _sanitize_local_paths(args)
    assert result["file_path"] == "main.py", f"Expected 'main.py', got '{result['file_path']}'"
    assert result["content"] == "print('hello')", f"Content was modified unexpectedly"
    assert result["output_path"] == "output.txt", f"Expected 'output.txt', got '{result['output_path']}'"
    print("✓ test_sanitize_multiple_args")


def test_translate_relative_path():
    """Test that relative paths are prefixed with workspace."""
    result = _translate_path("src/model.py")
    assert result == "/workspace/src/model.py", f"Expected '/workspace/src/model.py', got '{result}'"
    print("✓ test_translate_relative_path")


def test_translate_workspace_path():
    """Test that /workspace paths are preserved."""
    result = _translate_path("/workspace/src/model.py")
    assert result == "/workspace/src/model.py", f"Expected '/workspace/src/model.py', got '{result}'"
    print("✓ test_translate_workspace_path")


def test_translate_absolute_host_path():
    """Test that absolute host paths are converted to just filename."""
    result = _translate_path("/Users/nghibui/codes/test/main.py")
    assert result == "/workspace/main.py", f"Expected '/workspace/main.py', got '{result}'"
    print("✓ test_translate_absolute_host_path")


def test_translate_filename_only():
    """Test that just a filename gets workspace prefix."""
    result = _translate_path("config.yaml")
    assert result == "/workspace/config.yaml", f"Expected '/workspace/config.yaml', got '{result}'"
    print("✓ test_translate_filename_only")


def test_rewrite_removes_local_keyword():
    """Test that 'local' keyword is removed from task."""
    task = "Implement the local PDF paper"
    result = _rewrite_task_for_docker(task, [], "/workspace")
    # The word "local " should be stripped
    assert "local PDF" not in result, f"'local PDF' should be removed, got task body containing 'local PDF'"
    assert "PDF paper" in result, f"'PDF paper' should remain in task"
    print("✓ test_rewrite_removes_local_keyword")


def test_rewrite_replaces_in_this_repo():
    """Test that 'in this repo' is replaced with workspace path."""
    task = "Find all Python files in this repo"
    result = _rewrite_task_for_docker(task, [], "/workspace")
    assert "in /workspace" in result, f"Expected 'in /workspace' in result"
    assert "in this repo" not in result, f"'in this repo' should be replaced"
    print("✓ test_rewrite_replaces_in_this_repo")


def test_rewrite_replaces_local_directory_path():
    """Test that local directory paths are replaced with workspace."""
    task = "Read the file at /Users/nghibui/codes/test_opencli/main.py"
    result = _rewrite_task_for_docker(task, [], "/workspace", "/Users/nghibui/codes/test_opencli")
    assert "/Users/nghibui/codes/test_opencli" not in result, f"Local path should be replaced"
    assert "/workspace" in result, f"Workspace path should be present"
    print("✓ test_rewrite_replaces_local_directory_path")


def test_rewrite_includes_docker_preamble():
    """Test that Docker context preamble is included."""
    task = "Write a simple Python script"
    result = _rewrite_task_for_docker(task, [], "/workspace")
    assert "CRITICAL" in result, f"Expected 'CRITICAL' in preamble"
    assert "DOCKER" in result.upper(), f"Expected 'Docker' in preamble"
    assert "NEVER use paths like" in result, f"Expected path warnings in preamble"
    print("✓ test_rewrite_includes_docker_preamble")


def test_rewrite_replaces_input_file_paths():
    """Test that input file paths are replaced with Docker paths."""
    input_files = [Path("/Users/nghibui/codes/test_opencli/paper.pdf")]
    task = "Implement the paper at /Users/nghibui/codes/test_opencli/paper.pdf"
    result = _rewrite_task_for_docker(task, input_files, "/workspace", "/Users/nghibui/codes/test_opencli")
    assert "/workspace/paper.pdf" in result, f"Expected '/workspace/paper.pdf' in result"
    assert "/Users/nghibui" not in result, f"Local path should not be in result"
    print("✓ test_rewrite_replaces_input_file_paths")


def main():
    """Run all tests."""
    print("\n" + "=" * 60)
    print("Path Sanitization Tests")
    print("=" * 60 + "\n")

    tests = [
        test_sanitize_users_path,
        test_sanitize_home_path,
        test_sanitize_var_path,
        test_sanitize_tmp_path,
        test_preserve_relative_path,
        test_preserve_workspace_path,
        test_sanitize_pdf_path,
        test_sanitize_multiple_args,
        test_translate_relative_path,
        test_translate_workspace_path,
        test_translate_absolute_host_path,
        test_translate_filename_only,
        test_rewrite_removes_local_keyword,
        test_rewrite_replaces_in_this_repo,
        test_rewrite_replaces_local_directory_path,
        test_rewrite_includes_docker_preamble,
        test_rewrite_replaces_input_file_paths,
    ]

    passed = 0
    failed = 0

    for test in tests:
        try:
            test()
            passed += 1
        except AssertionError as e:
            print(f"✗ {test.__name__}: {e}")
            failed += 1
        except Exception as e:
            print(f"✗ {test.__name__}: Exception - {e}")
            failed += 1

    print("\n" + "=" * 60)
    print(f"Results: {passed} passed, {failed} failed")
    print("=" * 60 + "\n")

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
