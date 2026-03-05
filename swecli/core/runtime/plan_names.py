"""Generate unique plan names using adjective-verb-noun pattern."""

from __future__ import annotations

import random
from pathlib import Path

_ADJECTIVES = [
    "bold", "calm", "cool", "crisp", "dark", "deep", "fair", "fast",
    "fine", "free", "glad", "gold", "gray", "keen", "kind", "lean",
    "mild", "neat", "pale", "pure", "rare", "rich", "safe", "slim",
    "soft", "tall", "tidy", "warm", "wide", "wise",
]

_VERBS = [
    "blazing", "dashing", "diving", "drifting", "flying", "gliding",
    "growing", "hiding", "jumping", "landing", "leaping", "lifting",
    "moving", "pacing", "racing", "rising", "roaming", "rowing",
    "running", "sailing", "singing", "sliding", "soaring", "spinning",
    "splashing", "standing", "surfing", "swimming", "swinging", "waving",
]

_NOUNS = [
    "badger", "crane", "dolphin", "eagle", "falcon", "gecko", "hawk",
    "heron", "jaguar", "koala", "lark", "lemur", "lynx", "mantis",
    "otter", "panda", "parrot", "puffin", "quail", "raven", "robin",
    "salmon", "seal", "shark", "sparrow", "tiger", "turtle", "viper",
    "walrus", "whale",
]


def generate_plan_name(existing_dir: Path | None = None, max_attempts: int = 50) -> str:
    """Generate a unique plan name like 'bold-blazing-badger'.

    Args:
        existing_dir: Directory to check for collisions (looks for {name}.md files).
        max_attempts: Maximum regeneration attempts before falling back to random suffix.

    Returns:
        Unique plan name string.
    """
    for _ in range(max_attempts):
        name = (
            f"{random.choice(_ADJECTIVES)}-"
            f"{random.choice(_VERBS)}-"
            f"{random.choice(_NOUNS)}"
        )
        if existing_dir is None:
            return name
        if not (existing_dir / f"{name}.md").exists():
            return name

    # Fallback: append random digits
    base = (
        f"{random.choice(_ADJECTIVES)}-"
        f"{random.choice(_VERBS)}-"
        f"{random.choice(_NOUNS)}"
    )
    return f"{base}-{random.randint(1000, 9999)}"
