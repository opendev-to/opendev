"""Bullet selection logic for ACE playbook context optimization.

This module implements intelligent bullet selection to replace the "dump all bullets"
approach with query-specific, effectiveness-based selection.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Dict, List, Optional

from .embeddings import EmbeddingCache, cosine_similarity, generate_embeddings
from .playbook import Bullet


@dataclass
class ScoredBullet:
    """Bullet with its calculated relevance score."""

    bullet: Bullet
    score: float
    score_breakdown: Dict[str, float]


class BulletSelector:
    """Selects most relevant bullets for a given query.

    Implements hybrid retrieval with three scoring factors:
    - Effectiveness: Based on helpful/harmful feedback
    - Recency: Prefers recently updated bullets
    - Semantic: Query-to-bullet similarity using embeddings (Phase 2)
    """

    def __init__(
        self,
        weights: Optional[Dict[str, float]] = None,
        embedding_model: str = "text-embedding-3-small",
        cache_file: Optional[str] = None,
    ):
        """Initialize bullet selector.

        Args:
            weights: Scoring weights for different factors
                - effectiveness: Weight for helpful/harmful ratio (default: 0.5)
                - recency: Weight for recent usage (default: 0.3)
                - semantic: Weight for semantic similarity (default: 0.2)
            embedding_model: Model to use for embeddings
            cache_file: Optional path to cache file for persistence
        """
        self.weights = weights or {
            "effectiveness": 0.5,
            "recency": 0.3,
            "semantic": 0.2,
        }
        self.embedding_model = embedding_model
        self.cache_file = cache_file

        # Try to load cache from disk if cache_file provided
        if cache_file:
            loaded_cache = EmbeddingCache.load_from_file(cache_file)
            if loaded_cache:
                self.embedding_cache = loaded_cache
            else:
                self.embedding_cache = EmbeddingCache(model=embedding_model)
        else:
            self.embedding_cache = EmbeddingCache(model=embedding_model)

    def select(
        self,
        bullets: List[Bullet],
        max_count: int = 30,
        query: Optional[str] = None,
    ) -> List[Bullet]:
        """Select top-K most relevant bullets.

        Args:
            bullets: All available bullets
            max_count: Maximum number of bullets to return
            query: User query for semantic matching (enables semantic scoring)

        Returns:
            List of selected bullets, ordered by relevance (highest first)
        """
        try:
            # If we have fewer bullets than max_count, return all
            if len(bullets) <= max_count:
                return bullets

            # Optimization: Batch-generate embeddings if semantic scoring is needed
            if query and self.weights["semantic"] > 0:
                self._batch_generate_embeddings(query, bullets)

            # Score all bullets
            scored_bullets = [self._score_bullet(bullet, query) for bullet in bullets]

            # Sort by score (descending)
            scored_bullets.sort(key=lambda x: x.score, reverse=True)

            # Return top-K bullets
            return [sb.bullet for sb in scored_bullets[:max_count]]
        finally:
            # Always save cache to disk if cache_file is configured
            # This ensures embeddings are persisted regardless of selection path
            if self.cache_file:
                try:
                    self.embedding_cache.save_to_file(self.cache_file)
                except Exception:
                    # Silently fail if save fails - don't break selection
                    pass

    def _batch_generate_embeddings(self, query: str, bullets: List[Bullet]) -> None:
        """Pre-generate embeddings for query and bullets in batch.

        This optimization reduces API calls from N+1 to 1 by generating all
        embeddings in a single API call.

        Args:
            query: User query
            bullets: List of bullets to generate embeddings for
        """
        # Collect texts that need embeddings (not in cache)
        texts_to_generate = []
        text_indices = {}  # Map text to original index

        # Check if query needs embedding
        if self.embedding_cache.get(query) is None:
            texts_to_generate.append(query)
            text_indices[query] = len(texts_to_generate) - 1

        # Check which bullets need embeddings
        for bullet in bullets:
            if self.embedding_cache.get(bullet.content) is None:
                texts_to_generate.append(bullet.content)
                text_indices[bullet.content] = len(texts_to_generate) - 1

        # If all embeddings are cached, nothing to do
        if not texts_to_generate:
            return

        # Batch generate embeddings
        try:
            embeddings = self.embedding_cache.batch_get_or_generate(
                texts=texts_to_generate,
                generator=generate_embeddings,
            )

            # Note: batch_get_or_generate automatically caches the results
            # So we don't need to manually cache them here
        except Exception:
            # If batch generation fails, fallback to individual generation
            # This will happen naturally when _semantic_score is called
            pass

    def _score_bullet(self, bullet: Bullet, query: Optional[str] = None) -> ScoredBullet:
        """Calculate relevance score for a single bullet.

        Args:
            bullet: Bullet to score
            query: User query for semantic matching

        Returns:
            ScoredBullet with score and breakdown
        """
        breakdown = {}

        # Effectiveness score (helpful/harmful ratio)
        effectiveness = self._effectiveness_score(bullet)
        breakdown["effectiveness"] = effectiveness

        # Recency score (prefer recently updated bullets)
        recency = self._recency_score(bullet)
        breakdown["recency"] = recency

        # Semantic score (query-to-bullet similarity)
        semantic = 0.0
        if query and self.weights["semantic"] > 0:
            semantic = self._semantic_score(query, bullet)
        breakdown["semantic"] = semantic

        # Calculate weighted final score
        final_score = (
            self.weights["effectiveness"] * effectiveness +
            self.weights["recency"] * recency +
            self.weights["semantic"] * semantic
        )

        return ScoredBullet(
            bullet=bullet,
            score=final_score,
            score_breakdown=breakdown,
        )

    def _effectiveness_score(self, bullet: Bullet) -> float:
        """Calculate effectiveness score based on helpful/harmful feedback.

        Args:
            bullet: Bullet to score

        Returns:
            Score between 0.0 and 1.0
            - 1.0: Highly effective (many helpful, few harmful)
            - 0.5: Neutral or untested
            - 0.0: Ineffective (many harmful, few helpful)
        """
        total = bullet.helpful + bullet.harmful + bullet.neutral

        # Untested bullets get neutral score
        if total == 0:
            return 0.5

        # Calculate effectiveness ratio
        # helpful = 1.0, harmful = 0.0, neutral = 0.5
        weighted_score = (
            bullet.helpful * 1.0 +
            bullet.neutral * 0.5 +
            bullet.harmful * 0.0
        )

        return weighted_score / total

    def _recency_score(self, bullet: Bullet) -> float:
        """Calculate recency score - prefer recently updated bullets.

        Args:
            bullet: Bullet to score

        Returns:
            Score between 0.0 and 1.0
            - 1.0: Very recent (updated today)
            - 0.5: Moderately recent (updated 7 days ago)
            - 0.0: Very old (updated >30 days ago)
        """
        try:
            # Parse updated_at timestamp
            updated_at = datetime.fromisoformat(bullet.updated_at.replace("Z", "+00:00"))
            now = datetime.now(timezone.utc)

            # Calculate days since last update
            days_old = (now - updated_at).days

            # Decay function: score decreases as bullet gets older
            # score = 1.0 / (1.0 + days_old * decay_rate)
            # With decay_rate=0.1: day 0 → 1.0, day 7 → 0.59, day 30 → 0.25
            decay_rate = 0.1
            score = 1.0 / (1.0 + days_old * decay_rate)

            return score

        except (ValueError, AttributeError):
            # If parsing fails, return neutral score
            return 0.5

    def _semantic_score(self, query: str, bullet: Bullet) -> float:
        """Calculate semantic similarity between query and bullet content.

        Args:
            query: User query text
            bullet: Bullet to compare against

        Returns:
            Score between 0.0 and 1.0
            - 1.0: Highly similar (same semantic meaning)
            - 0.5: Neutral (orthogonal topics)
            - 0.0: Opposite or unrelated
        """
        try:
            # Get or generate embeddings using cache
            query_embedding = self.embedding_cache.get_or_generate(
                text=query,
                generator=generate_embeddings,
            )
            bullet_embedding = self.embedding_cache.get_or_generate(
                text=bullet.content,
                generator=generate_embeddings,
            )

            # Calculate cosine similarity
            similarity = cosine_similarity(query_embedding, bullet_embedding)

            # Normalize from [-1, 1] to [0, 1] range
            # -1 (opposite) → 0.0, 0 (orthogonal) → 0.5, 1 (identical) → 1.0
            normalized_score = (similarity + 1.0) / 2.0

            return normalized_score

        except Exception:
            # If embedding generation fails, return neutral score
            # This allows the selector to continue using effectiveness + recency
            return 0.5

    def get_selection_stats(self, bullets: List[Bullet], selected: List[Bullet]) -> Dict[str, any]:
        """Get statistics about the selection process.

        Args:
            bullets: All available bullets
            selected: Selected bullets

        Returns:
            Dictionary with selection statistics
        """
        selected_ids = {b.id for b in selected}

        # Calculate average scores
        all_scored = [self._score_bullet(b) for b in bullets]
        selected_scored = [sb for sb in all_scored if sb.bullet.id in selected_ids]

        avg_all_score = sum(sb.score for sb in all_scored) / len(all_scored) if all_scored else 0
        avg_selected_score = sum(sb.score for sb in selected_scored) / len(selected_scored) if selected_scored else 0

        return {
            "total_bullets": len(bullets),
            "selected_bullets": len(selected),
            "selection_rate": len(selected) / len(bullets) if bullets else 0,
            "avg_all_score": avg_all_score,
            "avg_selected_score": avg_selected_score,
            "score_improvement": avg_selected_score - avg_all_score,
        }

