"""Tests for channel-specific session reset policies."""

from datetime import datetime, timedelta

import pytest

from swecli.core.channels.reset_policies import (
    CHANNEL_RESET_POLICIES,
    format_policy_description,
    get_policy_for_channel,
    should_reset_session,
)
from swecli.models.session import Session


class TestIdleTimeoutPolicy:
    """Test idle timeout reset policy."""

    def test_telegram_resets_after_60_minutes(self):
        """Test Telegram session resets after 60 minutes of inactivity."""
        session = Session(
            id="test",
            working_directory="/tmp",
            channel="telegram",
            channel_user_id="user1",
            last_activity=datetime(2024, 1, 1, 10, 0),
        )

        # 59 minutes idle - should not reset
        now = datetime(2024, 1, 1, 10, 59)
        assert not should_reset_session(session, now)

        # 60 minutes idle - should not reset (exactly at threshold)
        now = datetime(2024, 1, 1, 11, 0)
        assert not should_reset_session(session, now)

        # 61 minutes idle - should reset
        now = datetime(2024, 1, 1, 11, 1)
        assert should_reset_session(session, now)

    def test_whatsapp_resets_after_30_minutes(self):
        """Test WhatsApp session resets after 30 minutes of inactivity."""
        session = Session(
            id="test",
            working_directory="/tmp",
            channel="whatsapp",
            channel_user_id="+1234567890",
            last_activity=datetime(2024, 1, 1, 10, 0),
        )

        # 29 minutes idle - should not reset
        now = datetime(2024, 1, 1, 10, 29)
        assert not should_reset_session(session, now)

        # 31 minutes idle - should reset
        now = datetime(2024, 1, 1, 10, 31)
        assert should_reset_session(session, now)

    def test_discord_resets_after_2_hours(self):
        """Test Discord session resets after 120 minutes of inactivity."""
        session = Session(
            id="test",
            working_directory="/tmp",
            channel="discord",
            channel_user_id="user#1234",
            last_activity=datetime(2024, 1, 1, 10, 0),
        )

        # 119 minutes idle - should not reset
        now = datetime(2024, 1, 1, 11, 59)
        assert not should_reset_session(session, now)

        # 121 minutes idle - should reset
        now = datetime(2024, 1, 1, 12, 1)
        assert should_reset_session(session, now)

    def test_no_activity_recorded_does_not_reset(self):
        """Test that session without last_activity doesn't reset."""
        session = Session(
            id="test",
            working_directory="/tmp",
            channel="telegram",
            channel_user_id="user1",
            last_activity=None,
        )

        # Should not reset even after a long time
        now = datetime(2024, 1, 1, 10, 0)
        assert not should_reset_session(session, now)

    def test_unknown_channel_uses_default_policy(self):
        """Test unknown channels use default idle timeout."""
        session = Session(
            id="test",
            working_directory="/tmp",
            channel="unknown-channel",
            channel_user_id="user1",
            last_activity=datetime(2024, 1, 1, 10, 0),
        )

        # Default is 60 minutes
        now = datetime(2024, 1, 1, 11, 1)
        assert should_reset_session(session, now)


class TestDailyResetPolicy:
    """Test daily reset policy."""

    def test_slack_resets_daily_at_4am_utc(self):
        """Test Slack session resets daily at 4 AM UTC."""
        session = Session(
            id="test",
            working_directory="/tmp",
            channel="slack",
            channel_user_id="U12345",
            last_activity=datetime(2024, 1, 1, 22, 0),  # 10 PM on Jan 1
            created_at=datetime(2024, 1, 1, 22, 0),
        )

        # Same day, before reset hour - should not reset
        now = datetime(2024, 1, 1, 23, 0)  # 11 PM on Jan 1
        assert not should_reset_session(session, now)

        # Next day, before reset hour - should not reset
        now = datetime(2024, 1, 2, 3, 0)  # 3 AM on Jan 2
        assert not should_reset_session(session, now)

        # Next day, at reset hour - should reset
        now = datetime(2024, 1, 2, 4, 0)  # 4 AM on Jan 2
        assert should_reset_session(session, now)

        # Next day, after reset hour - should reset
        now = datetime(2024, 1, 2, 10, 0)  # 10 AM on Jan 2
        assert should_reset_session(session, now)

    def test_daily_reset_same_day_never_resets(self):
        """Test that daily reset doesn't trigger on same day."""
        session = Session(
            id="test",
            working_directory="/tmp",
            channel="slack",
            channel_user_id="U12345",
            last_activity=datetime(2024, 1, 1, 5, 0),  # 5 AM
            created_at=datetime(2024, 1, 1, 5, 0),
        )

        # Later same day, past reset hour - should not reset
        now = datetime(2024, 1, 1, 18, 0)  # 6 PM same day
        assert not should_reset_session(session, now)

    def test_daily_reset_uses_created_at_if_no_activity(self):
        """Test daily reset uses created_at when last_activity is None."""
        session = Session(
            id="test",
            working_directory="/tmp",
            channel="slack",
            channel_user_id="U12345",
            last_activity=None,
            created_at=datetime(2024, 1, 1, 22, 0),
        )

        # Next day after reset hour - should reset
        now = datetime(2024, 1, 2, 10, 0)
        assert should_reset_session(session, now)


class TestNeverResetPolicy:
    """Test never reset policy."""

    def test_web_never_resets(self):
        """Test Web sessions never reset."""
        session = Session(
            id="test",
            working_directory="/tmp",
            channel="web",
            channel_user_id="web-user",
            last_activity=datetime(2020, 1, 1, 10, 0),  # Very old activity
        )

        # Even years later - should not reset
        now = datetime(2024, 1, 1, 10, 0)
        assert not should_reset_session(session, now)

    def test_cli_never_resets(self):
        """Test CLI sessions never reset."""
        session = Session(
            id="test",
            working_directory="/tmp",
            channel="cli",
            channel_user_id="cli-user",
            last_activity=datetime(2020, 1, 1, 10, 0),
        )

        # Even years later - should not reset
        now = datetime(2024, 1, 1, 10, 0)
        assert not should_reset_session(session, now)


class TestGetPolicyForChannel:
    """Test getting policy configuration."""

    def test_returns_telegram_policy(self):
        """Test getting Telegram policy."""
        policy = get_policy_for_channel("telegram")
        assert policy["mode"] == "idle"
        assert policy["idle_minutes"] == 60

    def test_returns_slack_policy(self):
        """Test getting Slack policy."""
        policy = get_policy_for_channel("slack")
        assert policy["mode"] == "daily"
        assert policy["at_hour_utc"] == 4

    def test_returns_web_policy(self):
        """Test getting Web policy."""
        policy = get_policy_for_channel("web")
        assert policy["mode"] == "never"

    def test_returns_default_for_unknown_channel(self):
        """Test unknown channels get default policy."""
        policy = get_policy_for_channel("unknown-channel")
        assert policy["mode"] == "idle"
        assert policy["idle_minutes"] == 60


class TestFormatPolicyDescription:
    """Test policy description formatting."""

    def test_formats_idle_policy_in_minutes(self):
        """Test formatting idle policy under 60 minutes."""
        desc = format_policy_description("whatsapp")
        assert desc == "Session resets after 30 minutes of inactivity"

    def test_formats_idle_policy_in_hours(self):
        """Test formatting idle policy 60+ minutes."""
        desc = format_policy_description("telegram")
        assert desc == "Session resets after 1 hour of inactivity"

    def test_formats_idle_policy_multiple_hours(self):
        """Test formatting idle policy for multiple hours."""
        desc = format_policy_description("discord")
        assert desc == "Session resets after 2 hours of inactivity"

    def test_formats_daily_policy(self):
        """Test formatting daily reset policy."""
        desc = format_policy_description("slack")
        assert desc == "Session resets daily at 4 AM UTC"

    def test_formats_never_policy(self):
        """Test formatting never reset policy."""
        desc = format_policy_description("web")
        assert desc == "Session never resets automatically"


class TestPolicyIntegration:
    """Integration tests for reset policies."""

    def test_all_defined_channels_have_policies(self):
        """Test that all channels in CHANNEL_RESET_POLICIES are valid."""
        for channel, policy in CHANNEL_RESET_POLICIES.items():
            assert "mode" in policy
            mode = policy["mode"]
            assert mode in ["idle", "daily", "never"]

            if mode == "idle":
                assert "idle_minutes" in policy
                assert policy["idle_minutes"] > 0

            if mode == "daily":
                assert "at_hour_utc" in policy
                assert 0 <= policy["at_hour_utc"] < 24

    def test_session_with_recent_activity_never_resets(self):
        """Test that very recent activity prevents all resets."""
        now = datetime.utcnow()
        one_minute_ago = now - timedelta(minutes=1)

        for channel in ["telegram", "whatsapp", "slack", "discord"]:
            session = Session(
                id="test",
                working_directory="/tmp",
                channel=channel,
                channel_user_id="user1",
                last_activity=one_minute_ago,
                created_at=one_minute_ago,
            )
            assert not should_reset_session(session, now)
