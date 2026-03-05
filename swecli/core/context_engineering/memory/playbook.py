"""Native swecli ACE Playbook implementation.

This module re-implements the ACE (Agentic Context Engine) playbook system
natively within swecli, without external dependencies.
"""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, Iterable, List, Optional

from .delta import DeltaBatch, DeltaOperation


@dataclass
class Bullet:
    """Single playbook entry storing a strategy or insight."""

    id: str
    section: str
    content: str
    helpful: int = 0
    harmful: int = 0
    neutral: int = 0
    created_at: str = field(
        default_factory=lambda: datetime.now(timezone.utc).isoformat()
    )
    updated_at: str = field(
        default_factory=lambda: datetime.now(timezone.utc).isoformat()
    )

    def apply_metadata(self, metadata: Dict[str, int]) -> None:
        """Apply metadata updates to counters."""
        for key, value in metadata.items():
            if hasattr(self, key):
                setattr(self, key, int(value))

    def tag(self, tag: str, increment: int = 1) -> None:
        """Increment a counter (helpful/harmful/neutral)."""
        if tag not in ("helpful", "harmful", "neutral"):
            raise ValueError(f"Unsupported tag: {tag}")
        current = getattr(self, tag)
        setattr(self, tag, current + increment)
        self.updated_at = datetime.now(timezone.utc).isoformat()


class Playbook:
    """Structured context store for accumulated strategies and insights.

    The Playbook replaces traditional message history with a curated collection
    of strategy entries (bullets) that evolve based on execution feedback.
    """

    def __init__(self) -> None:
        self._bullets: Dict[str, Bullet] = {}
        self._sections: Dict[str, List[str]] = {}
        self._next_id = 0

    def __repr__(self) -> str:
        """Concise representation for debugging."""
        return f"Playbook(bullets={len(self._bullets)}, sections={list(self._sections.keys())})"

    def __str__(self) -> str:
        """Human-readable representation showing content."""
        if not self._bullets:
            return "Playbook(empty)"
        return self.as_prompt()

    # ------------------------------------------------------------------ #
    # CRUD operations
    # ------------------------------------------------------------------ #
    def add_bullet(
        self,
        section: str,
        content: str,
        bullet_id: Optional[str] = None,
        metadata: Optional[Dict[str, int]] = None,
    ) -> Bullet:
        """Add a new bullet to the playbook."""
        bullet_id = bullet_id or self._generate_id(section)
        metadata = metadata or {}
        bullet = Bullet(id=bullet_id, section=section, content=content)
        bullet.apply_metadata(metadata)
        self._bullets[bullet_id] = bullet
        self._sections.setdefault(section, []).append(bullet_id)
        return bullet

    def update_bullet(
        self,
        bullet_id: str,
        *,
        content: Optional[str] = None,
        metadata: Optional[Dict[str, int]] = None,
    ) -> Optional[Bullet]:
        """Update an existing bullet."""
        bullet = self._bullets.get(bullet_id)
        if bullet is None:
            return None
        if content is not None:
            bullet.content = content
        if metadata:
            bullet.apply_metadata(metadata)
        bullet.updated_at = datetime.now(timezone.utc).isoformat()
        return bullet

    def tag_bullet(
        self, bullet_id: str, tag: str, increment: int = 1
    ) -> Optional[Bullet]:
        """Tag a bullet to update its counters."""
        bullet = self._bullets.get(bullet_id)
        if bullet is None:
            return None
        bullet.tag(tag, increment=increment)
        return bullet

    def remove_bullet(self, bullet_id: str) -> None:
        """Remove a bullet from the playbook."""
        bullet = self._bullets.pop(bullet_id, None)
        if bullet is None:
            return
        section_list = self._sections.get(bullet.section)
        if section_list:
            self._sections[bullet.section] = [
                bid for bid in section_list if bid != bullet_id
            ]
            if not self._sections[bullet.section]:
                del self._sections[bullet.section]

    def get_bullet(self, bullet_id: str) -> Optional[Bullet]:
        """Get a bullet by ID."""
        return self._bullets.get(bullet_id)

    def bullets(self) -> List[Bullet]:
        """Get all bullets."""
        return list(self._bullets.values())

    # ------------------------------------------------------------------ #
    # Serialization
    # ------------------------------------------------------------------ #
    def to_dict(self) -> Dict[str, object]:
        """Convert to dictionary for serialization."""
        return {
            "bullets": {
                bullet_id: asdict(bullet) for bullet_id, bullet in self._bullets.items()
            },
            "sections": self._sections,
            "next_id": self._next_id,
        }

    @classmethod
    def from_dict(cls, payload: Dict[str, object]) -> Playbook:
        """Load from dictionary."""
        instance = cls()
        bullets_payload = payload.get("bullets", {})
        if isinstance(bullets_payload, dict):
            for bullet_id, bullet_value in bullets_payload.items():
                if isinstance(bullet_value, dict):
                    instance._bullets[bullet_id] = Bullet(**bullet_value)
        sections_payload = payload.get("sections", {})
        if isinstance(sections_payload, dict):
            instance._sections = {
                section: list(ids) if isinstance(ids, Iterable) else []
                for section, ids in sections_payload.items()
            }
        instance._next_id = int(payload.get("next_id", 0))
        return instance

    def dumps(self) -> str:
        """Convert to JSON string."""
        return json.dumps(self.to_dict(), ensure_ascii=False, indent=2)

    @classmethod
    def loads(cls, data: str) -> Playbook:
        """Load from JSON string."""
        payload = json.loads(data)
        if not isinstance(payload, dict):
            raise ValueError("Playbook serialization must be a JSON object.")
        return cls.from_dict(payload)

    def save_to_file(self, path: str) -> None:
        """Save playbook to JSON file."""
        file_path = Path(path)
        file_path.parent.mkdir(parents=True, exist_ok=True)
        with file_path.open("w", encoding="utf-8") as f:
            f.write(self.dumps())

    @classmethod
    def load_from_file(cls, path: str) -> Playbook:
        """Load playbook from JSON file."""
        file_path = Path(path)
        if not file_path.exists():
            raise FileNotFoundError(f"Playbook file not found: {path}")
        with file_path.open("r", encoding="utf-8") as f:
            return cls.loads(f.read())

    # ------------------------------------------------------------------ #
    # Delta operations
    # ------------------------------------------------------------------ #
    def apply_delta(self, delta: DeltaBatch) -> None:
        """Apply a batch of delta operations."""
        bullets_before = len(self._bullets)

        for operation in delta.operations:
            self._apply_operation(operation)

        bullets_after = len(self._bullets)

    def _apply_operation(self, operation: DeltaOperation) -> None:
        """Apply a single delta operation."""
        op_type = operation.type.upper()
        if op_type == "ADD":
            self.add_bullet(
                section=operation.section,
                content=operation.content or "",
                bullet_id=operation.bullet_id,
                metadata=operation.metadata,
            )
        elif op_type == "UPDATE":
            if operation.bullet_id is None:
                return
            self.update_bullet(
                operation.bullet_id,
                content=operation.content,
                metadata=operation.metadata,
            )
        elif op_type == "TAG":
            if operation.bullet_id is None:
                return
            # Only apply valid tag names
            valid_tags = {"helpful", "harmful", "neutral"}
            for tag, increment in operation.metadata.items():
                if tag in valid_tags:
                    self.tag_bullet(operation.bullet_id, tag, increment)
        elif op_type == "REMOVE":
            if operation.bullet_id is None:
                return
            self.remove_bullet(operation.bullet_id)

    # ------------------------------------------------------------------ #
    # Presentation helpers
    # ------------------------------------------------------------------ #
    def as_prompt(self) -> str:
        """Return playbook as formatted string for LLM prompting.

        Returns ALL bullets without selection. Use as_context() for intelligent selection.
        """
        parts: List[str] = []
        for section, bullet_ids in sorted(self._sections.items()):
            parts.append(f"## {section}")
            for bullet_id in bullet_ids:
                bullet = self._bullets[bullet_id]
                counters = f"(helpful={bullet.helpful}, harmful={bullet.harmful}, neutral={bullet.neutral})"
                parts.append(f"- [{bullet.id}] {bullet.content} {counters}")
        return "\n".join(parts)

    def as_context(
        self,
        query: Optional[str] = None,
        max_strategies: Optional[int] = 30,
        use_selection: bool = True,
        weights: Optional[Dict[str, float]] = None,
        embedding_model: str = "text-embedding-3-small",
        cache_file: Optional[str] = None,
    ) -> str:
        """Return intelligently selected bullets for LLM context.

        This method implements ACE's hybrid retrieval approach, selecting only the
        most relevant bullets instead of including all bullets in context.

        Args:
            query: User query for semantic matching (Phase 2+, currently unused)
            max_strategies: Maximum number of bullets to include (None = all)
            use_selection: Whether to use intelligent selection (False = same as as_prompt())
            weights: Custom scoring weights (effectiveness, recency, semantic)
            embedding_model: Model to use for embeddings
            cache_file: Optional path to cache file for embedding persistence

        Returns:
            Formatted string with selected bullets

        Examples:
            >>> playbook.as_context(query="fix authentication bug", max_strategies=20)
            Returns top 20 most relevant bullets for authentication debugging
        """
        from .selector import BulletSelector

        # Get all bullets
        all_bullets = self.bullets()

        # If no bullets or selection disabled, return all
        if not all_bullets or not use_selection:
            return self.as_prompt()

        # If max_strategies is None or >= total bullets, return all
        if max_strategies is None or max_strategies >= len(all_bullets):
            return self.as_prompt()

        # Select top-K bullets
        selector = BulletSelector(
            weights=weights,
            embedding_model=embedding_model,
            cache_file=cache_file,
        )
        selected_bullets = selector.select(
            bullets=all_bullets,
            max_count=max_strategies,
            query=query,
        )

        # Format selected bullets (same format as as_prompt())
        # Group by section
        bullets_by_section: Dict[str, List[Bullet]] = {}
        for bullet in selected_bullets:
            bullets_by_section.setdefault(bullet.section, []).append(bullet)

        # Format output
        parts: List[str] = []
        for section in sorted(bullets_by_section.keys()):
            parts.append(f"## {section}")
            for bullet in bullets_by_section[section]:
                counters = f"(helpful={bullet.helpful}, harmful={bullet.harmful}, neutral={bullet.neutral})"
                parts.append(f"- [{bullet.id}] {bullet.content} {counters}")

        return "\n".join(parts)

    def stats(self) -> Dict[str, object]:
        """Get playbook statistics."""
        return {
            "sections": len(self._sections),
            "bullets": len(self._bullets),
            "tags": {
                "helpful": sum(b.helpful for b in self._bullets.values()),
                "harmful": sum(b.harmful for b in self._bullets.values()),
                "neutral": sum(b.neutral for b in self._bullets.values()),
            },
        }

    # ------------------------------------------------------------------ #
    # Internal helpers
    # ------------------------------------------------------------------ #
    def _generate_id(self, section: str) -> str:
        """Generate unique bullet ID."""
        self._next_id += 1
        section_prefix = section.split()[0].lower()
        return f"{section_prefix}-{self._next_id:05d}"


# Backward compatibility aliases
Strategy = Bullet
SessionPlaybook = Playbook
