"""Authentication dependencies for FastAPI routes."""

from __future__ import annotations

from uuid import UUID

from fastapi import Request

from opendev.models.user import User
from opendev.web.routes.auth import TOKEN_COOKIE, verify_token
from opendev.web.state import get_state

# Fixed anonymous user for unauthenticated local access
_ANONYMOUS_USER: User | None = None


def _get_anonymous_user() -> User:
    global _ANONYMOUS_USER
    if _ANONYMOUS_USER is None:
        _ANONYMOUS_USER = User(
            id=UUID("00000000-0000-0000-0000-000000000000"),
            username="local",
            password_hash="",
        )
    return _ANONYMOUS_USER


async def require_authenticated_user(request: Request) -> User:
    """Validate auth if present, otherwise return anonymous user for local access."""

    token = request.cookies.get(TOKEN_COOKIE)
    if not token:
        user = _get_anonymous_user()
        request.state.user = user
        return user

    try:
        user_id = verify_token(token)
        state = get_state()
        user = state.user_store.get_by_id(user_id)
        if not user:
            user = _get_anonymous_user()
        request.state.user = user
        return user
    except Exception:
        user = _get_anonymous_user()
        request.state.user = user
        return user
