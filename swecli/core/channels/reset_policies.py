"""Per-channel session reset policies.

Different channels have different conversation patterns and expectations:
- Telegram/WhatsApp: Idle timeout (30-60 minutes of inactivity)
- Slack: Daily reset (fresh context each morning)
- Web/CLI: Never reset (user controls session lifecycle)

This module defines reset policies and provides logic to determine when
a session should be reset based on its channel and activity.
"""

from datetime import datetime, timedelta
from typing import Optional

from swecli.models.session import Session

# Reset policy configurations per channel
CHANNEL_RESET_POLICIES: dict[str, dict] = {
    "telegram": {
        "mode": "idle",
        "idle_minutes": 60,
    },
    "whatsapp": {
        "mode": "idle",
        "idle_minutes": 30,
    },
    "slack": {
        "mode": "daily",
        "at_hour_utc": 4,  # Reset at 4 AM UTC (midnight EST, evening PST)
    },
    "discord": {
        "mode": "idle",
        "idle_minutes": 120,
    },
    "web": {
        "mode": "never",
    },
    "cli": {
        "mode": "never",
    },
    # Default for unknown channels
    "default": {
        "mode": "idle",
        "idle_minutes": 60,
    },
}


def should_reset_session(session: Session, now: Optional[datetime] = None) -> bool:
    """Determine if a session should be reset based on its channel's policy.

    Args:
        session: Session to check
        now: Current time (defaults to datetime.utcnow())

    Returns:
        True if session should be reset, False otherwise

    Examples:
        >>> session = Session(channel="telegram", last_activity=datetime(2024, 1, 1, 10, 0))
        >>> should_reset_session(session, now=datetime(2024, 1, 1, 11, 30))
        True  # 90 minutes idle > 60 minute policy

        >>> session = Session(channel="web")
        >>> should_reset_session(session)
        False  # Web never resets
    """
    if now is None:
        now = datetime.utcnow()

    # Get policy for this channel
    policy = CHANNEL_RESET_POLICIES.get(session.channel, CHANNEL_RESET_POLICIES["default"])

    # Never reset
    if policy["mode"] == "never":
        return False

    # Idle timeout mode
    if policy["mode"] == "idle":
        if not session.last_activity:
            # No activity recorded - don't reset
            return False

        idle_duration = now - session.last_activity
        idle_minutes = idle_duration.total_seconds() / 60
        return idle_minutes > policy["idle_minutes"]

    # Daily reset mode
    if policy["mode"] == "daily":
        reset_hour = policy["at_hour_utc"]
        last_activity = session.last_activity or session.created_at

        # Has a day boundary been crossed since last activity?
        if last_activity.date() < now.date():
            # Yes - check if we've passed the reset hour
            return now.hour >= reset_hour

        # Same day - no reset
        return False

    # Unknown mode - default to no reset
    return False


def get_policy_for_channel(channel: str) -> dict:
    """Get the reset policy configuration for a channel.

    Args:
        channel: Channel name

    Returns:
        Policy dictionary with 'mode' and mode-specific fields

    Examples:
        >>> get_policy_for_channel("telegram")
        {'mode': 'idle', 'idle_minutes': 60}

        >>> get_policy_for_channel("web")
        {'mode': 'never'}
    """
    return CHANNEL_RESET_POLICIES.get(channel, CHANNEL_RESET_POLICIES["default"])


def format_policy_description(channel: str) -> str:
    """Get a human-readable description of a channel's reset policy.

    Args:
        channel: Channel name

    Returns:
        Human-readable policy description

    Examples:
        >>> format_policy_description("telegram")
        'Session resets after 60 minutes of inactivity'

        >>> format_policy_description("slack")
        'Session resets daily at 4 AM UTC'

        >>> format_policy_description("web")
        'Session never resets automatically'
    """
    policy = get_policy_for_channel(channel)

    if policy["mode"] == "never":
        return "Session never resets automatically"

    if policy["mode"] == "idle":
        minutes = policy["idle_minutes"]
        if minutes >= 60:
            hours = minutes / 60
            return f"Session resets after {hours:.0f} hour{'s' if hours != 1 else ''} of inactivity"
        return f"Session resets after {minutes} minutes of inactivity"

    if policy["mode"] == "daily":
        hour = policy["at_hour_utc"]
        return f"Session resets daily at {hour} AM UTC"

    return "Unknown reset policy"
