"""BridgeSessionGuard — proxy that protects the TUI session in bridge mode.

When the TUI is the execution authority and the Web UI mirrors it, web routes
must not be allowed to create, switch, or delete the active session.  This
proxy wraps ``state.session_manager`` and intercepts mutation methods while
transparently delegating all reads and safe writes (save, add_message, etc.).
"""

from __future__ import annotations

import logging
from typing import Any, Optional

logger = logging.getLogger(__name__)


class BridgeSessionGuard:
    """Proxy around SessionManager that blocks session mutations in bridge mode.

    Intercepted methods:
        create_session   — returns the existing TUI session (no-op)
        load_session     — only allows reloading the bridge session
        load_latest_session — returns the current session
        delete_session   — blocks deletion of the bridge session
        fork_session     — blocks (returns None)

    Everything else (get_current_session, get_session_by_id, save_session,
    add_message, list_all_sessions, …) is delegated via __getattr__.
    """

    def __init__(self, session_manager: Any, bridge_session_id: str) -> None:
        # Use object.__setattr__ to avoid triggering our own __setattr__
        object.__setattr__(self, "_inner", session_manager)
        object.__setattr__(self, "_bridge_session_id", bridge_session_id)

    # ------------------------------------------------------------------
    # Intercepted mutations
    # ------------------------------------------------------------------

    def create_session(self, **kwargs: Any) -> Any:
        """Block session creation; return the existing TUI session."""
        logger.debug("BridgeSessionGuard: create_session blocked, returning TUI session")
        return self._inner.get_current_session()

    def load_session(self, session_id: str, **kwargs: Any) -> Any:
        """Only allow loading the bridge session; others return current."""
        if session_id == self._bridge_session_id:
            return self._inner.load_session(session_id, **kwargs)
        logger.debug(
            "BridgeSessionGuard: load_session(%s) blocked, returning current", session_id
        )
        return self._inner.get_current_session()

    def load_latest_session(self, **kwargs: Any) -> Any:
        """Return the current session instead of loading from disk."""
        logger.debug("BridgeSessionGuard: load_latest_session redirected to current")
        return self._inner.get_current_session()

    def delete_session(self, session_id: str, **kwargs: Any) -> Any:
        """Block deletion of the bridge session; allow others."""
        if session_id == self._bridge_session_id:
            logger.debug("BridgeSessionGuard: delete_session(%s) blocked", session_id)
            return None
        return self._inner.delete_session(session_id, **kwargs)

    def fork_session(self, **kwargs: Any) -> None:
        """Block forking in bridge mode."""
        logger.debug("BridgeSessionGuard: fork_session blocked")
        return None

    # ------------------------------------------------------------------
    # Property proxy for current_session setter
    # ------------------------------------------------------------------

    @property
    def current_session(self) -> Any:
        return self._inner.current_session

    @current_session.setter
    def current_session(self, value: Any) -> None:
        """Block setting current_session to None or a different session."""
        if value is None:
            logger.debug("BridgeSessionGuard: blocked setting current_session to None")
            return
        sid = getattr(value, "id", None)
        if sid and sid != self._bridge_session_id:
            logger.debug(
                "BridgeSessionGuard: blocked setting current_session to %s", sid
            )
            return
        self._inner.current_session = value

    # ------------------------------------------------------------------
    # Pass-through for everything else
    # ------------------------------------------------------------------

    def __getattr__(self, name: str) -> Any:
        return getattr(self._inner, name)

    def __setattr__(self, name: str, value: Any) -> None:
        # _inner and _bridge_session_id are set via object.__setattr__ in __init__
        # Everything else delegates to the inner session manager
        setattr(self._inner, name, value)
