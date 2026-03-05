"""File-oriented tool handlers used by the registry."""

from __future__ import annotations

import asyncio
from datetime import datetime
from pathlib import Path
from typing import Union, Any

from swecli.core.context_engineering.tools.context import ToolExecutionContext
from swecli.core.context_engineering.tools.path_utils import sanitize_path
from swecli.models.operation import Operation, OperationType


class FileToolHandler:
    """Handles file read/write/edit operations."""

    def __init__(self, file_ops: Any, write_tool: Any, edit_tool: Any) -> None:
        self._file_ops = file_ops
        self._write_tool = write_tool
        self._edit_tool = edit_tool

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------
    def write_file(self, args: dict[str, Any], context: ToolExecutionContext) -> dict[str, Any]:
        if not self._write_tool:
            return {"success": False, "error": "WriteTool not available"}

        file_path = sanitize_path(args["file_path"])
        content = args["content"]
        create_dirs = args.get("create_dirs", True)

        operation = Operation(
            id=str(hash(f"{file_path}{content}{datetime.now()}")),
            type=OperationType.FILE_WRITE,
            target=file_path,
            parameters={"content": content, "create_dirs": create_dirs},
            created_at=datetime.now(),
        )

        approved_content = self._ensure_write_approval(operation, content, context)
        if approved_content is None:
            return {
                "success": False,
                "interrupted": True,
                "denied": True,
                "output": None,
            }

        write_result = self._write_tool.write_file(
            file_path,
            approved_content,
            create_dirs=create_dirs,
            operation=operation,
        )

        if write_result.success:
            if context.undo_manager:
                context.undo_manager.record_operation(operation)

            # Track file change in session
            if context.session_manager:
                from swecli.models.file_change import FileChange, FileChangeType
                from pathlib import Path

                session = context.session_manager.get_current_session()
                if session:
                    file_change = FileChange(
                        type=FileChangeType.CREATED,
                        file_path=file_path,
                        lines_added=len(content.split("\n")),
                        description=f"Created {Path(file_path).name}",
                        session_id=session.id
                    )
                    session.add_file_change(file_change)

        return {
            "success": write_result.success,
            "output": f"File created: {file_path}" if write_result.success else None,
            "error": (write_result.error or "Write operation failed") if not write_result.success else None,
        }

    def edit_file(self, args: dict[str, Any], context: ToolExecutionContext) -> dict[str, Any]:
        if not self._edit_tool:
            return {"success": False, "error": "EditTool not available"}

        file_path = sanitize_path(args["file_path"])
        old_content = args["old_content"]
        new_content = args["new_content"]
        match_all = args.get("match_all", False)

        # Stale-read detection: reject edit if file changed since last read
        if context.file_time_tracker:
            stale_error = context.file_time_tracker.assert_fresh(file_path)
            if stale_error:
                return {"success": False, "error": stale_error, "output": None}

        operation = Operation(
            id=str(hash(f"{file_path}{old_content}{new_content}{datetime.now()}")),
            type=OperationType.FILE_EDIT,
            target=file_path,
            parameters={
                "old_content": old_content,
                "new_content": new_content,
                "match_all": match_all,
            },
            created_at=datetime.now(),
        )

        preview = self._edit_tool.edit_file(
            file_path,
            old_content,
            new_content,
            match_all=match_all,
            dry_run=True,
        )

        if not preview.success:
            return {
                "success": False,
                "error": preview.error,
                "output": None,
            }

        if not self._is_approved(operation, context, preview.diff):
            return {
                "success": False,
                "interrupted": True,
                "denied": True,
                "output": None,
            }

        edit_result = self._edit_tool.edit_file(
            file_path,
            old_content,
            new_content,
            match_all=match_all,
            backup=True,
        )

        if edit_result.success:
            if context.undo_manager:
                context.undo_manager.record_operation(operation)

            # Invalidate stale-read record: agent must re-read before next edit
            if context.file_time_tracker:
                context.file_time_tracker.invalidate(file_path)

            # Track file change in session
            if context.session_manager:
                from swecli.models.file_change import FileChange, FileChangeType
                from pathlib import Path

                session = context.session_manager.get_current_session()
                if session:
                    file_change = FileChange(
                        type=FileChangeType.MODIFIED,
                        file_path=file_path,
                        lines_added=edit_result.lines_added,
                        lines_removed=edit_result.lines_removed,
                        description=f"Modified {Path(file_path).name} (+{edit_result.lines_added}/-{edit_result.lines_removed})",
                        session_id=session.id
                    )
                    session.add_file_change(file_change)

        output_msg = (
            f"File edited: {file_path} (+{edit_result.lines_added}/-{edit_result.lines_removed})"
            if edit_result.success
            else None
        )

        # LSP diagnostics: check for errors introduced by the edit
        if edit_result.success and output_msg:
            diag_text = self._get_lsp_diagnostics(file_path)
            if diag_text:
                output_msg += diag_text

        return {
            "success": edit_result.success,
            "output": output_msg,
            "error": (edit_result.error or "Edit operation failed") if not edit_result.success else None,
            "file_path": file_path,
            "lines_added": edit_result.lines_added,
            "lines_removed": edit_result.lines_removed,
            "diff": edit_result.diff,
        }

    def read_file(self, args: dict[str, Any], context: ToolExecutionContext | None = None) -> dict[str, Any]:
        if not self._file_ops:
            return {"success": False, "error": "FileOperations not available"}

        file_path = sanitize_path(args["file_path"])
        try:
            content = self._file_ops.read_file(
                file_path,
                offset=args.get("offset"),
                max_lines=args.get("max_lines"),
            )
            # Record read timestamp for stale-read detection
            if context and context.file_time_tracker:
                context.file_time_tracker.record_read(file_path)
            return {"success": True, "output": content, "error": None}
        except Exception as exc:  # noqa: BLE001
            return {"success": False, "error": str(exc), "output": None}

    def list_files(self, args: dict[str, Any]) -> dict[str, Any]:
        if not self._file_ops:
            return {"success": False, "error": "FileOperations not available"}

        raw_path = args.get("path")
        path = sanitize_path(raw_path) if raw_path is not None else "."
        pattern = args.get("pattern")
        max_results = args.get("max_results", 100)

        try:
            base_path = Path(path)
            if not base_path.is_absolute():
                base_path = (self._file_ops.working_dir / base_path).resolve()
        except Exception as exc:
            return {"success": False, "error": f"Invalid path: {exc}", "output": None}

        try:
            entries: list[str] | None = None
            if pattern:
                search_root = base_path if base_path.is_dir() else base_path.parent
                if not search_root.exists():
                    return {
                        "success": True,
                        "output": f"Directory not found: {search_root}",
                        "error": None,
                    }
                files = self._file_ops.glob_files(
                    pattern,
                    max_results=max_results,
                    base_path=search_root,
                )
                output = "\n".join(files) if files else "No files found"
                entries = files
            else:
                output = self._file_ops.list_directory(str(base_path))
                if output and not output.startswith(("Directory not found", "Not a directory")):
                    entries = [line for line in output.splitlines() if line.strip()]

            # Truncate list output to prevent context bloat
            max_list_entries = 500
            if entries and len(entries) > max_list_entries:
                total = len(entries)
                entries = entries[:max_list_entries]
                output = "\n".join(entries)
                output += f"\n... (showing {max_list_entries} of {total} entries)"

            return {"success": True, "output": output, "entries": entries, "error": None}
        except Exception as exc:  # noqa: BLE001
            return {"success": False, "error": str(exc), "output": None}

    def search(self, args: dict[str, Any]) -> dict[str, Any]:
        """Unified search handler supporting text and AST search modes.

        Args:
            pattern: Search pattern (regex for text, AST pattern for structural)
            path: Directory to search
            type: "text" (default) for ripgrep, "ast" for ast-grep
            lang: Language hint for AST mode (auto-detected if not specified)
        """
        if not self._file_ops:
            return {"success": False, "error": "FileOperations not available"}

        pattern = args["pattern"]
        path = sanitize_path(args.get("path", "."))
        search_type = args.get("type", "text")  # "text" or "ast"
        lang = args.get("lang")

        try:
            if search_type == "ast":
                # AST-based structural search using ast-grep
                matches = self._file_ops.ast_grep(pattern, path, lang)
                if not matches:
                    return {"success": True, "output": "No structural matches found", "matches": []}
            else:
                # Default: text/regex search using ripgrep
                matches = self._file_ops.grep_files(pattern, path)
                if not matches:
                    return {"success": True, "output": "No matches found", "matches": []}

            lines = []
            total_chars = 0
            max_output_chars = 30_000
            shown = 0
            for match in matches[:50]:
                line = f"{match['file']}:{match['line']} - {match['content']}"
                total_chars += len(line) + 1  # +1 for newline
                if total_chars > max_output_chars:
                    lines.append(
                        f"\n... (output truncated at {max_output_chars} chars. "
                        f"Showing {shown} of {len(matches)} matches.)"
                    )
                    break
                lines.append(line)
                shown += 1
            else:
                if len(matches) > 50:
                    lines.append(f"\n... and {len(matches) - 50} more matches")
            output = "\n".join(lines)

            return {"success": True, "output": output, "matches": matches}
        except FileNotFoundError:
            if search_type == "ast":
                return {"success": False, "error": "ast-grep (sg) not installed. Install: brew install ast-grep", "output": None}
            return {"success": False, "error": "File or directory not found", "output": None}
        except Exception as exc:  # noqa: BLE001
            error_msg = str(exc)
            if "timeout" in error_msg.lower():
                error_msg = "Search timed out. Try a more specific path."
            return {"success": False, "error": error_msg, "output": None}

    # ------------------------------------------------------------------
    # LSP diagnostics
    # ------------------------------------------------------------------
    @staticmethod
    def _get_lsp_diagnostics(file_path: str) -> str:
        """Check for LSP diagnostics (errors) after editing a file.

        Returns a formatted string to append to tool output, or empty string
        if no LSP server is available or no errors are found.
        """
        try:
            from swecli.core.context_engineering.tools.lsp import get_lsp_wrapper

            wrapper = get_lsp_wrapper()
            diagnostics = wrapper.get_diagnostics(file_path, severity_filter=1)
            if not diagnostics:
                return ""

            lines = ["\n\nLSP errors detected:"]
            for d in diagnostics:
                lines.append(f"  Line {d['line']}: {d['message']}")

            return "\n".join(lines)
        except Exception:
            # LSP not available or failed — gracefully skip
            return ""

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------
    def _ensure_write_approval(
        self,
        operation: Operation,
        content: str,
        context: ToolExecutionContext,
    ) -> Union[str, None]:
        if not self._is_approval_required(operation, context):
            operation.approved = True
            return content

        approval_manager = context.approval_manager
        if not approval_manager:
            operation.approved = True
            return content

        preview = content[:500] + ("..." if len(content) > 500 else "")
        result = self._run_sync_approval(
            approval_manager,
            operation,
            preview,
        )
        if result is None:
            operation.approved = True
            return content

        if not result.approved:
            return None

        if result.edited_content:
            operation.parameters["content"] = result.edited_content
            return result.edited_content
        operation.approved = True
        return content

    def _is_approved(
        self,
        operation: Operation,
        context: ToolExecutionContext,
        preview: Union[str, None],
    ) -> bool:
        if not self._is_approval_required(operation, context):
            operation.approved = True
            return True

        approval_manager = context.approval_manager
        if not approval_manager:
            operation.approved = True
            return True

        result = self._run_sync_approval(
            approval_manager,
            operation,
            preview or "",
        )
        if result is None:
            operation.approved = True
            return True

        return bool(result.approved)

    def _is_approval_required(
        self,
        operation: Operation,
        context: ToolExecutionContext,
    ) -> bool:
        mode_manager = context.mode_manager
        if not mode_manager:
            return True
        return mode_manager.needs_approval(operation)

    @staticmethod
    def _run_sync_approval(approval_manager: Any, operation: Operation, preview: str, **extra_kwargs: Any):
        try:
            asyncio.get_running_loop()
        except RuntimeError:
            # Check if request_approval already returned a result (WebApprovalManager) or needs to be awaited
            approval_result = approval_manager.request_approval(
                operation=operation,
                preview=preview,
                **extra_kwargs,
            )

            # If it's already a result object, use it directly
            if hasattr(approval_result, 'approved'):
                return approval_result
            else:
                # If it's a coroutine, run it
                return asyncio.run(approval_result)

        operation.approved = True
        return None
