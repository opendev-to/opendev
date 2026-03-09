"""Session management API endpoints."""

import os
from pathlib import Path
from typing import Dict, List, Any

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel

from opendev.web.state import get_state
from opendev.web.routes.chat import MessageResponse, ToolCallInfo, tool_call_to_info
from opendev.web.dependencies.auth import require_authenticated_user

router = APIRouter(
    prefix="/api/sessions",
    tags=["sessions"],
    dependencies=[Depends(require_authenticated_user)],
)


class SessionInfo(BaseModel):
    """Session information model."""
    id: str
    working_dir: str
    created_at: str
    updated_at: str
    message_count: int
    total_tokens: int
    title: str | None = None
    has_session_model: bool = False


class CreateSessionRequest(BaseModel):
    """Request model for creating a new session."""
    workspace: str


@router.post("/create")
async def create_session(
    request: CreateSessionRequest,
    user=Depends(require_authenticated_user),
) -> Dict[str, Any]:
    """Create a new session with specified workspace.

    Args:
        request: Request containing workspace path

    Returns:
        New session information

    Raises:
        HTTPException: If creation fails
    """
    try:
        print(f"[DEBUG] Creating session with workspace: {request.workspace}")
        state = get_state()
        print(f"[DEBUG] Got state: {state}")

        # Create new session with specified workspace, owned by the current user
        state.session_manager.create_session(
            working_directory=request.workspace,
            owner_id=str(user.id),
        )
        print("[DEBUG] Session created")

        session = state.session_manager.get_current_session()

        # Force-save so the session file exists on disk for WebSocket lookups
        state.session_manager.save_session(force=True)
        print(f"[DEBUG] Got current session: {session.id}")

        # Initialize plan file path for plan mode
        from opendev.core.paths import get_paths

        plans_dir = get_paths().global_dir / "plans"
        plans_dir.mkdir(parents=True, exist_ok=True)
        plan_file_path = plans_dir / f"{session.id}.md"
        state.mode_manager.set_plan_file_path(str(plan_file_path))

        return {
            "status": "success",
            "message": "Session created",
            "session": {
                "id": session.id,
                "working_dir": session.working_directory or "",
                "created_at": session.created_at.isoformat(),
                "updated_at": session.updated_at.isoformat(),
                "message_count": len(session.messages),
                "total_tokens": session.total_tokens(),
            }
        }

    except Exception as e:
        print(f"[ERROR] Failed to create session: {e}")
        import traceback
        traceback.print_exc()
        raise HTTPException(status_code=500, detail=str(e))


@router.get("")
async def list_sessions(user=Depends(require_authenticated_user)) -> List[SessionInfo]:
    """List all available sessions for the current user."""
    try:
        state = get_state()
        sessions = state.list_sessions(owner_id=str(user.id))
        return [SessionInfo(**session) for session in sessions]
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/current")
async def get_current_session(user=Depends(require_authenticated_user)) -> Dict[str, Any]:
    """Get the current active session for the user."""
    try:
        state = get_state()
        session = state.session_manager.get_current_session()
        if not session or session.owner_id != str(user.id):
            raise HTTPException(status_code=404, detail="No active session")

        return {
            "id": session.id,
            "working_dir": session.working_directory or "",
            "created_at": session.created_at.isoformat(),
            "updated_at": session.updated_at.isoformat(),
            "message_count": len(session.messages),
            "total_tokens": session.total_tokens(),
        }

    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.post("/{session_id}/resume")
async def resume_session(
    session_id: str,
    user=Depends(require_authenticated_user),
) -> Dict[str, str]:
    """Resume a specific session.

    Args:
        session_id: ID of the session to resume

    Returns:
        Status response

    Raises:
        HTTPException: If session not found or resume fails
    """
    try:
        print(f"[DEBUG] Resuming session {session_id}")
        state = get_state()

        # Check if this is the current session (newly created but not yet saved)
        current = state.session_manager.get_current_session()
        if current and current.id == session_id:
            if current.owner_id != str(user.id):
                raise HTTPException(status_code=403, detail="Forbidden")
            print(f"[DEBUG] Session {session_id} is already the current session (not yet saved)")
            return {"status": "success", "message": f"Session {session_id} already active"}

        # Try to load from disk with ownership enforcement
        success = state.resume_session(session_id, owner_id=str(user.id))

        if not success:
            print(f"[DEBUG] Session {session_id} not found or not owned by user")
            raise HTTPException(status_code=404, detail=f"Session {session_id} not found")

        # Verify session was loaded
        current = state.session_manager.get_current_session()
        if current:
            print(f"[DEBUG] Session {session_id} loaded with {len(current.messages)} messages")

            # Initialize plan file path for plan mode
            from opendev.core.paths import get_paths

            plans_dir = get_paths().global_dir / "plans"
            plans_dir.mkdir(parents=True, exist_ok=True)
            plan_file_path = plans_dir / f"{current.id}.md"
            state.mode_manager.set_plan_file_path(str(plan_file_path))
        else:
            print(f"[DEBUG] WARNING: Session loaded but current_session is None")

        return {"status": "success", "message": f"Resumed session {session_id}"}

    except HTTPException:
        raise
    except Exception as e:
        print(f"[ERROR] Failed to resume session: {e}")
        import traceback
        traceback.print_exc()
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/{session_id}/messages")
async def get_session_messages(
    session_id: str,
    user=Depends(require_authenticated_user),
) -> List[MessageResponse]:
    """Get messages for a specific session without changing the current session.

    Uses get_session_by_id() which is non-mutating — it does not change
    the session_manager's current_session pointer.

    Args:
        session_id: ID of the session to read messages from

    Returns:
        List of messages

    Raises:
        HTTPException: If session not found
    """
    try:
        state = get_state()

        # Try non-mutating read first
        try:
            session = state.session_manager.get_session_by_id(session_id, owner_id=str(user.id))
        except FileNotFoundError:
            # Session might be newly created but not saved to disk yet
            current = state.session_manager.get_current_session()
            if current and current.id == session_id:
                session = current
            else:
                raise HTTPException(status_code=404, detail=f"Session {session_id} not found")

        visible_messages = [
            m for m in session.messages if not m.metadata.get("display_hidden")
        ]
        return [
            MessageResponse(
                role=msg.role.value,
                content=msg.content,
                timestamp=(
                    msg.timestamp.isoformat()
                    if hasattr(msg, "timestamp") and msg.timestamp
                    else None
                ),
                tool_calls=[tool_call_to_info(tc) for tc in msg.tool_calls]
                if msg.tool_calls
                else None,
                thinking_trace=msg.thinking_trace,
                reasoning_content=msg.reasoning_content,
            )
            for msg in visible_messages
        ]

    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.delete("/{session_id}")
async def delete_session(session_id: str, user=Depends(require_authenticated_user)) -> Dict[str, str]:
    """Delete a specific session.

    Args:
        session_id: ID of the session to delete

    Returns:
        Status response

    Raises:
        HTTPException: If deletion fails
    """
    try:
        state = get_state()

        state = get_state()
        try:
            session = state.session_manager.get_session_by_id(session_id, owner_id=str(user.id))
        except FileNotFoundError:
            raise HTTPException(status_code=404, detail=f"Session {session_id} not found")

        state.session_manager.delete_session(session_id)

        current_session = state.session_manager.get_current_session()
        if current_session and current_session.id == session_id:
            state.session_manager.current_session = None

        return {"status": "success", "message": f"Session {session_id} deleted"}

    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/{session_id}/export")
async def export_session(session_id: str, user=Depends(require_authenticated_user)) -> Dict[str, Any]:
    """Export a session as JSON.

    Args:
        session_id: ID of the session to export

    Returns:
        Session data

    Raises:
        HTTPException: If export fails
    """
    try:
        state = get_state()

        original_session_id = state.get_current_session_id()
        if not state.resume_session(session_id, owner_id=str(user.id)):
            raise HTTPException(status_code=404, detail=f"Session {session_id} not found")

        session = state.session_manager.get_current_session()

        if original_session_id:
            state.resume_session(original_session_id, owner_id=str(user.id))

        return {
            "id": session.id,
            "working_dir": session.working_directory or "",
            "created_at": session.created_at.isoformat(),
            "updated_at": session.updated_at.isoformat(),
            "messages": [
                {
                    "role": msg.role.value,
                    "content": msg.content,
                    "timestamp": msg.timestamp.isoformat() if hasattr(msg, 'timestamp') and msg.timestamp else None,
                }
                for msg in session.messages
            ],
            "token_usage": session.token_usage,
        }

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.post("/verify-path")
async def verify_path(path_data: Dict[str, str]) -> Dict[str, Any]:
    """Verify if a directory path exists and is accessible.

    Args:
        path_data: Dictionary with 'path' key

    Returns:
        Dictionary with exists, is_directory, and error fields

    Raises:
        HTTPException: If verification fails
    """
    try:
        path = path_data.get("path", "").strip()

        if not path:
            return {
                "exists": False,
                "is_directory": False,
                "error": "Path cannot be empty"
            }

        path_obj = Path(path).expanduser().resolve()

        if not path_obj.exists():
            return {
                "exists": False,
                "is_directory": False,
                "error": "Path does not exist"
            }

        if not path_obj.is_dir():
            return {
                "exists": True,
                "is_directory": False,
                "error": "Path is not a directory"
            }

        # Check if we have read access
        if not os.access(path_obj, os.R_OK):
            return {
                "exists": True,
                "is_directory": True,
                "error": "No read access to directory"
            }

        return {
            "exists": True,
            "is_directory": True,
            "path": str(path_obj),
            "error": None
        }

    except Exception as e:
        return {
            "exists": False,
            "is_directory": False,
            "error": f"Failed to verify path: {str(e)}"
        }


class BrowseDirectoryRequest(BaseModel):
    """Request model for browsing directories."""
    path: str = ""
    show_hidden: bool = False


@router.post("/browse-directory")
async def browse_directory(request: BrowseDirectoryRequest) -> Dict[str, Any]:
    """Browse directories at a given path for the workspace picker.

    Args:
        request: Request with path (defaults to home dir) and show_hidden flag

    Returns:
        Dictionary with current_path, parent_path, directories list, and error
    """
    try:
        raw = request.path.strip()
        if not raw:
            target = Path.home()
        else:
            target = Path(raw).expanduser().resolve()

        if not target.exists():
            return {
                "current_path": str(target),
                "parent_path": str(target.parent) if target.parent != target else None,
                "directories": [],
                "error": "Path does not exist",
            }

        if not target.is_dir():
            return {
                "current_path": str(target),
                "parent_path": str(target.parent) if target.parent != target else None,
                "directories": [],
                "error": "Path is not a directory",
            }

        if not os.access(target, os.R_OK):
            return {
                "current_path": str(target),
                "parent_path": str(target.parent) if target.parent != target else None,
                "directories": [],
                "error": "No read access to directory",
            }

        parent = target.parent
        parent_path = str(parent) if parent != target else None

        dirs = []
        try:
            for entry in target.iterdir():
                if not entry.is_dir():
                    continue
                if entry.name.startswith(".") and not request.show_hidden:
                    continue
                if not os.access(entry, os.R_OK):
                    continue
                dirs.append({"name": entry.name, "path": str(entry)})
        except PermissionError:
            return {
                "current_path": str(target),
                "parent_path": parent_path,
                "directories": [],
                "error": "Permission denied reading directory contents",
            }

        dirs.sort(key=lambda d: d["name"].lower())

        return {
            "current_path": str(target),
            "parent_path": parent_path,
            "directories": dirs,
            "error": None,
        }

    except Exception as e:
        return {
            "current_path": request.path,
            "parent_path": None,
            "directories": [],
            "error": f"Failed to browse directory: {str(e)}",
        }


@router.get("/{session_id}/file-changes")
async def get_session_file_changes(
    session_id: str,
    user=Depends(require_authenticated_user),
) -> Dict[str, Any]:
    """Get file change history for a session."""
    try:
        state = get_state()
        try:
            session = state.session_manager.get_session_by_id(session_id, owner_id=str(user.id))
        except FileNotFoundError:
            raise HTTPException(status_code=404, detail=f"Session {session_id} not found")

        return {
            "file_changes": [fc.model_dump() for fc in session.file_changes],
            "message": "File change history retrieved successfully"
        }

    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/files")
async def list_files(query: str = "") -> Dict[str, Any]:
    """List files in the current session's working directory.

    Args:
        query: Optional search query to filter files

    Returns:
        Dictionary with files array

    Raises:
        HTTPException: If listing fails
    """
    try:
        state = get_state()
        session = state.session_manager.get_current_session()

        if not session or not session.working_directory:
            return {"files": []}

        working_dir = Path(session.working_directory)
        if not working_dir.exists() or not working_dir.is_dir():
            return {"files": []}

        # Fallback ignore patterns if no .gitignore exists
        # Tier 1: Always exclude (obviously generated, never source code)
        always_exclude = {
            # Version Control
            '.git', '.hg', '.svn', '.bzr', '_darcs', '.fossil',
            # OS Generated
            '.DS_Store', '.Spotlight-V100', '.Trashes', 'Thumbs.db', 'desktop.ini', '$RECYCLE.BIN',
            # Python
            '__pycache__', '.pytest_cache', '.mypy_cache', '.pytype', '.pyre',
            '.hypothesis', '.tox', '.nox', 'cython_debug', '.eggs',
            # Node/JS
            'node_modules', '.npm', '.yarn', '.pnpm-store',
            '.next', '.nuxt', '.output', '.svelte-kit', '.angular', '.parcel-cache', '.turbo',
            # IDE/Editor
            '.idea', '.vscode', '.vs', '.settings',
            # Java/Kotlin
            '.gradle',
            # Elixir
            '_build', 'deps', '.elixir_ls',
            # iOS
            'Pods', 'DerivedData', 'xcuserdata',
            # Ruby
            '.bundle',
            # Virtual Environments
            '.venv', 'venv',
            # Misc caches
            '.cache', '.sass-cache', '.eslintcache', '.tmp', '.temp', 'tmp', 'temp',
        }
        # Tier 2: Likely exclude (common build output dirs)
        likely_exclude = {
            'dist', 'build', 'out', 'bin', 'obj', 'target',
            'coverage', 'htmlcov', 'cover', 'logs',
            'vendor', 'packages', 'bower_components',
        }
        fallback_ignore_patterns = always_exclude | likely_exclude

        # Try to load gitignore parser
        gitignore_parser = None
        gitignore_path = working_dir / ".gitignore"
        if gitignore_path.exists():
            from opendev.ui_textual.autocomplete_internal.gitignore import GitIgnoreParser
            gitignore_parser = GitIgnoreParser(working_dir)

        def should_skip_dir(dir_path: Path, dir_name: str) -> bool:
            """Check if a directory should be skipped."""
            if gitignore_parser:
                return gitignore_parser.should_skip_dir(dir_path)
            return dir_name in fallback_ignore_patterns

        def should_skip_file(file_path: Path) -> bool:
            """Check if a file should be skipped."""
            if gitignore_parser:
                return gitignore_parser.is_ignored(file_path)
            return False

        files = []
        try:
            # Use os.walk for more efficient traversal with pruning
            for root, dirs, filenames in os.walk(working_dir):
                root_path = Path(root)

                # Modify dirs in-place to skip ignored directories
                dirs[:] = [d for d in dirs if not should_skip_dir(root_path / d, d)]

                for filename in filenames:
                    file_path = root_path / filename

                    # Skip ignored files
                    if should_skip_file(file_path):
                        continue

                    # Get relative path
                    try:
                        rel_path = file_path.relative_to(working_dir)
                        path_str = str(rel_path)

                        # Filter by query if provided
                        if query and query.lower() not in path_str.lower():
                            continue

                        files.append({
                            'path': path_str,
                            'name': filename,
                            'is_file': True
                        })
                    except ValueError:
                        continue

                    # Limit early if we have enough results
                    if len(files) >= 100:
                        break

                if len(files) >= 100:
                    break

        except PermissionError:
            pass  # Skip directories we can't access

        # Sort files by path
        files.sort(key=lambda x: x['path'])

        # Limit to 100 results for performance
        files = files[:100]

        return {"files": files}

    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to list files: {str(e)}")


# ========================================================================
# Session Model Overlay Endpoints
# ========================================================================


class SessionModelUpdate(BaseModel):
    """Request body for updating session model overlay."""
    model_provider: str | None = None
    model: str | None = None
    model_thinking_provider: str | None = None
    model_thinking: str | None = None
    model_vlm_provider: str | None = None
    model_vlm: str | None = None
    model_critique_provider: str | None = None
    model_critique: str | None = None
    model_compact_provider: str | None = None
    model_compact: str | None = None


@router.get("/{session_id}/model")
async def get_session_model_overlay(
    session_id: str,
    user=Depends(require_authenticated_user),
) -> Dict[str, Any]:
    try:
        state = get_state()
        session = state.session_manager.get_session_by_id(session_id, owner_id=str(user.id))

        overlay = session.metadata.get("session_model") or {}
        return overlay

    except FileNotFoundError:
        raise HTTPException(status_code=404, detail=f"Session {session_id} not found")
    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.put("/{session_id}/model")
async def update_session_model(
    session_id: str,
    body: SessionModelUpdate,
    user=Depends(require_authenticated_user),
) -> Dict[str, str]:
    """Set or update the session-model overlay."""
    try:
        from opendev.core.runtime.session_model import (
            SESSION_MODEL_FIELDS,
            SessionModelManager,
            set_session_model,
        )

        state = get_state()

        # Build overlay from non-None fields
        overlay: Dict[str, str] = {}
        for field_name in SESSION_MODEL_FIELDS:
            value = getattr(body, field_name, None)
            if value is not None:
                overlay[field_name] = value

        if not overlay:
            raise HTTPException(status_code=400, detail="No model fields provided")

        # Load the session
        current = state.session_manager.get_current_session()
        is_current = current and current.id == session_id

        if is_current:
            session = current
        else:
            try:
                session = state.session_manager.get_session_by_id(session_id, owner_id=str(user.id))
            except FileNotFoundError:
                raise HTTPException(
                    status_code=404, detail=f"Session {session_id} not found"
                )

        # Apply overlay to live config if this is the active session
        if is_current:
            config = state.config_manager.get_config()
            # Restore previous overlay if any
            if hasattr(state, "_session_model_manager") and state._session_model_manager:
                state._session_model_manager.restore()
            mgr = SessionModelManager(config)
            mgr.apply(overlay)
            state._session_model_manager = mgr

        # Persist to session metadata
        set_session_model(session, overlay)
        state.session_manager.save_session(session)

        return {"status": "success", "message": "Session model updated"}

    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.delete("/{session_id}/model")
async def delete_session_model(session_id: str, user=Depends(require_authenticated_user)) -> Dict[str, str]:
    try:
        state = get_state()

        current = state.session_manager.get_current_session()
        is_current = current and current.id == session_id

        if is_current:
            session = current
            # Restore live config
            if hasattr(state, "_session_model_manager") and state._session_model_manager:
                state._session_model_manager.restore()
                state._session_model_manager = None
        else:
            try:
                session = state.session_manager.get_session_by_id(session_id, owner_id=str(user.id))
            except FileNotFoundError:
                raise HTTPException(
                    status_code=404, detail=f"Session {session_id} not found"
                )

        clear_session_model(session)
        state.session_manager.save_session(session)

        return {"status": "success", "message": "Session model cleared"}

    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
