"""Tests for bullet selection logic (Phase 1: Effectiveness-based scoring)."""

from datetime import datetime, timedelta, timezone

import pytest

from swecli.core.context_engineering.memory.playbook import Bullet, Playbook
from swecli.core.context_engineering.memory.selector import BulletSelector, ScoredBullet


class TestBulletSelector:
    """Test suite for BulletSelector class."""

    def test_selector_initialization(self):
        """Test selector initializes with default weights."""
        selector = BulletSelector()

        assert selector.weights["effectiveness"] == 0.5
        assert selector.weights["recency"] == 0.3
        assert selector.weights["semantic"] == 0.2
        assert selector.embedding_model == "text-embedding-3-small"

    def test_selector_custom_weights(self):
        """Test selector accepts custom weights."""
        custom_weights = {
            "effectiveness": 0.7,
            "recency": 0.2,
            "semantic": 0.1,
        }
        selector = BulletSelector(weights=custom_weights)

        assert selector.weights == custom_weights

    def test_select_returns_all_when_fewer_than_max(self):
        """Test selector returns all bullets when total < max_count."""
        selector = BulletSelector()
        bullets = [
            Bullet(id="b1", section="Test", content="Bullet 1"),
            Bullet(id="b2", section="Test", content="Bullet 2"),
        ]

        selected = selector.select(bullets, max_count=10)

        assert len(selected) == 2
        assert set(b.id for b in selected) == {"b1", "b2"}

    def test_select_limits_to_max_count(self):
        """Test selector limits results to max_count."""
        selector = BulletSelector()
        bullets = [
            Bullet(id=f"b{i}", section="Test", content=f"Bullet {i}")
            for i in range(50)
        ]

        selected = selector.select(bullets, max_count=10)

        assert len(selected) == 10

    def test_effectiveness_score_all_helpful(self):
        """Test effectiveness score for highly effective bullets."""
        selector = BulletSelector()
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Test",
            helpful=10,
            harmful=0,
            neutral=0,
        )

        score = selector._effectiveness_score(bullet)

        assert score == 1.0  # Perfect effectiveness

    def test_effectiveness_score_all_harmful(self):
        """Test effectiveness score for ineffective bullets."""
        selector = BulletSelector()
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Test",
            helpful=0,
            harmful=10,
            neutral=0,
        )

        score = selector._effectiveness_score(bullet)

        assert score == 0.0  # No effectiveness

    def test_effectiveness_score_mixed(self):
        """Test effectiveness score for mixed feedback bullets."""
        selector = BulletSelector()
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Test",
            helpful=7,
            harmful=3,
            neutral=0,
        )

        score = selector._effectiveness_score(bullet)

        assert score == 0.7  # 70% effective

    def test_effectiveness_score_with_neutral(self):
        """Test effectiveness score includes neutral feedback."""
        selector = BulletSelector()
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Test",
            helpful=5,
            harmful=0,
            neutral=10,
        )

        score = selector._effectiveness_score(bullet)

        # (5 * 1.0 + 10 * 0.5 + 0 * 0.0) / 15 = 10 / 15 = 0.666...
        assert pytest.approx(score, 0.01) == 0.667

    def test_effectiveness_score_untested(self):
        """Test effectiveness score for untested bullets returns neutral."""
        selector = BulletSelector()
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Test",
            helpful=0,
            harmful=0,
            neutral=0,
        )

        score = selector._effectiveness_score(bullet)

        assert score == 0.5  # Neutral score for untested

    def test_recency_score_brand_new(self):
        """Test recency score for just-created bullets."""
        selector = BulletSelector()
        now = datetime.now(timezone.utc)
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Test",
            updated_at=now.isoformat(),
        )

        score = selector._recency_score(bullet)

        assert score == 1.0  # Maximum recency

    def test_recency_score_old_bullet(self):
        """Test recency score decreases for older bullets."""
        selector = BulletSelector()
        old_date = datetime.now(timezone.utc) - timedelta(days=30)
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Test",
            updated_at=old_date.isoformat(),
        )

        score = selector._recency_score(bullet)

        # score = 1.0 / (1.0 + 30 * 0.1) = 1.0 / 4.0 = 0.25
        assert pytest.approx(score, 0.01) == 0.25

    def test_recency_score_invalid_date(self):
        """Test recency score returns neutral for invalid dates."""
        selector = BulletSelector()
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Test",
            updated_at="invalid-date",
        )

        score = selector._recency_score(bullet)

        assert score == 0.5  # Neutral for parsing failures

    def test_score_bullet_combines_factors(self):
        """Test bullet scoring combines effectiveness and recency."""
        selector = BulletSelector()
        now = datetime.now(timezone.utc)
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Test",
            helpful=10,
            harmful=0,
            neutral=0,
            updated_at=now.isoformat(),
        )

        scored = selector._score_bullet(bullet)

        # effectiveness = 1.0, recency = 1.0, semantic = 0.0
        # final = 0.5 * 1.0 + 0.3 * 1.0 + 0.2 * 0.0 = 0.8
        assert isinstance(scored, ScoredBullet)
        assert scored.bullet == bullet
        assert pytest.approx(scored.score, 0.01) == 0.8
        assert "effectiveness" in scored.score_breakdown
        assert "recency" in scored.score_breakdown
        assert "semantic" in scored.score_breakdown

    def test_select_prioritizes_effective_bullets(self):
        """Test selector prioritizes highly effective bullets."""
        selector = BulletSelector()
        now = datetime.now(timezone.utc)

        bullets = [
            Bullet(
                id="b_effective",
                section="Test",
                content="Effective strategy",
                helpful=10,
                harmful=0,
                updated_at=now.isoformat(),
            ),
            Bullet(
                id="b_ineffective",
                section="Test",
                content="Ineffective strategy",
                helpful=0,
                harmful=10,
                updated_at=now.isoformat(),
            ),
            Bullet(
                id="b_neutral",
                section="Test",
                content="Untested strategy",
                helpful=0,
                harmful=0,
                updated_at=now.isoformat(),
            ),
        ]

        selected = selector.select(bullets, max_count=2)

        # Should select effective first, then neutral
        assert len(selected) == 2
        assert selected[0].id == "b_effective"
        # Second should be neutral (0.5 effectiveness) over ineffective (0.0)
        assert selected[1].id == "b_neutral"

    def test_select_prioritizes_recent_bullets(self):
        """Test selector prioritizes recently updated bullets."""
        selector = BulletSelector()
        now = datetime.now(timezone.utc)
        old_date = now - timedelta(days=30)

        bullets = [
            Bullet(
                id="b_old",
                section="Test",
                content="Old strategy",
                helpful=5,
                harmful=5,  # Same effectiveness
                updated_at=old_date.isoformat(),
            ),
            Bullet(
                id="b_recent",
                section="Test",
                content="Recent strategy",
                helpful=5,
                harmful=5,  # Same effectiveness
                updated_at=now.isoformat(),
            ),
        ]

        selected = selector.select(bullets, max_count=1)

        # Should prioritize recent bullet
        assert len(selected) == 1
        assert selected[0].id == "b_recent"

    def test_get_selection_stats(self):
        """Test selection statistics calculation."""
        selector = BulletSelector()
        bullets = [
            Bullet(id=f"b{i}", section="Test", content=f"Bullet {i}", helpful=i)
            for i in range(10)
        ]

        selected = selector.select(bullets, max_count=5)
        stats = selector.get_selection_stats(bullets, selected)

        assert stats["total_bullets"] == 10
        assert stats["selected_bullets"] == 5
        assert stats["selection_rate"] == 0.5
        assert "avg_all_score" in stats
        assert "avg_selected_score" in stats
        assert "score_improvement" in stats
        # Selected bullets should have higher average score
        assert stats["avg_selected_score"] > stats["avg_all_score"]


class TestPlaybookIntegration:
    """Test playbook integration with bullet selector."""

    def test_as_context_returns_all_when_small(self):
        """Test as_context returns all bullets when playbook is small."""
        playbook = Playbook()
        playbook.add_bullet("Test", "Bullet 1")
        playbook.add_bullet("Test", "Bullet 2")

        context = playbook.as_context(query="test", max_strategies=10)

        assert "Bullet 1" in context
        assert "Bullet 2" in context

    def test_as_context_limits_large_playbook(self):
        """Test as_context limits bullets for large playbooks."""
        playbook = Playbook()
        for i in range(50):
            playbook.add_bullet("Test", f"Bullet {i}")

        context = playbook.as_context(query="test", max_strategies=10)

        # Should only include 10 bullets
        bullet_count = context.count("Bullet")
        assert bullet_count == 10

    def test_as_context_fallback_to_as_prompt(self):
        """Test as_context falls back to as_prompt when selection disabled."""
        playbook = Playbook()
        for i in range(20):
            playbook.add_bullet("Test", f"Bullet {i}")

        context_disabled = playbook.as_context(
            query="test",
            max_strategies=5,
            use_selection=False,
        )
        prompt_output = playbook.as_prompt()

        assert context_disabled == prompt_output

    def test_as_context_with_none_max_strategies(self):
        """Test as_context returns all when max_strategies is None."""
        playbook = Playbook()
        for i in range(10):
            playbook.add_bullet("Test", f"Bullet {i}")

        context = playbook.as_context(query="test", max_strategies=None)
        prompt = playbook.as_prompt()

        assert context == prompt

    def test_as_context_preserves_format(self):
        """Test as_context output format matches as_prompt."""
        playbook = Playbook()
        playbook.add_bullet("Section A", "Strategy A1", metadata={"helpful": 5})
        playbook.add_bullet("Section B", "Strategy B1", metadata={"helpful": 3})

        context = playbook.as_context(query="test", max_strategies=2)

        # Should have section headers
        assert "## Section A" in context or "## Section B" in context
        # Should have bullet IDs (format: [section-00001])
        assert "[section-" in context or "[" in context
        # Should have counters
        assert "(helpful=" in context

    def test_as_context_groups_by_section(self):
        """Test as_context maintains section grouping."""
        playbook = Playbook()
        playbook.add_bullet("Error Handling", "Strategy 1", metadata={"helpful": 10})
        playbook.add_bullet("Testing", "Strategy 2", metadata={"helpful": 8})
        playbook.add_bullet("Error Handling", "Strategy 3", metadata={"helpful": 6})

        context = playbook.as_context(query="test", max_strategies=3)

        # Should have both section headers
        assert "## Error Handling" in context
        assert "## Testing" in context
        # Strategies should be under their sections
        sections = context.split("## ")
        for section in sections:
            if "Error Handling" in section:
                assert "Strategy 1" in section or "Strategy 3" in section


class TestSemanticSimilarity:
    """Test suite for Phase 2 semantic similarity features."""

    def test_semantic_score_calculation(self):
        """Test semantic similarity scoring with mock embeddings."""
        selector = BulletSelector()
        now = datetime.now(timezone.utc)

        # Mock embeddings for query and bullet
        # Similar embeddings → high similarity
        query = "Fix authentication error"
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Handle authentication failures gracefully",
            updated_at=now.isoformat(),
        )

        # Mock the embedding cache with similar vectors
        # Vectors pointing in same direction → cosine similarity ≈ 1.0
        query_embedding = [1.0, 0.0, 0.0]  # Unit vector along x-axis
        bullet_embedding = [0.9, 0.1, 0.0]  # Similar direction
        selector.embedding_cache.set(query, query_embedding)
        selector.embedding_cache.set(bullet.content, bullet_embedding)

        score = selector._semantic_score(query, bullet)

        # Score should be high (> 0.9) for similar embeddings
        assert score > 0.9
        assert score <= 1.0

    def test_semantic_score_dissimilar(self):
        """Test semantic score for dissimilar content."""
        selector = BulletSelector()
        now = datetime.now(timezone.utc)

        query = "Fix authentication error"
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Optimize database query performance",
            updated_at=now.isoformat(),
        )

        # Mock with orthogonal vectors → cosine similarity ≈ 0.0
        query_embedding = [1.0, 0.0, 0.0]
        bullet_embedding = [0.0, 1.0, 0.0]
        selector.embedding_cache.set(query, query_embedding)
        selector.embedding_cache.set(bullet.content, bullet_embedding)

        score = selector._semantic_score(query, bullet)

        # Score should be around 0.5 for orthogonal vectors
        # cosine(orthogonal) = 0.0 → normalized = 0.5
        assert pytest.approx(score, 0.1) == 0.5

    def test_semantic_score_opposite(self):
        """Test semantic score for opposite/contradictory content."""
        selector = BulletSelector()
        now = datetime.now(timezone.utc)

        query = "Enable feature X"
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Disable feature X completely",
            updated_at=now.isoformat(),
        )

        # Mock with opposite vectors → cosine similarity ≈ -1.0
        query_embedding = [1.0, 0.0, 0.0]
        bullet_embedding = [-1.0, 0.0, 0.0]
        selector.embedding_cache.set(query, query_embedding)
        selector.embedding_cache.set(bullet.content, bullet_embedding)

        score = selector._semantic_score(query, bullet)

        # Score should be low (≈ 0.0) for opposite vectors
        # cosine(opposite) = -1.0 → normalized = 0.0
        assert score < 0.1

    def test_semantic_score_with_cache(self):
        """Test that semantic scoring uses embedding cache."""
        selector = BulletSelector()
        now = datetime.now(timezone.utc)

        query = "Test query"
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Test content",
            updated_at=now.isoformat(),
        )

        # Pre-populate cache
        query_embedding = [1.0, 0.0, 0.0]
        bullet_embedding = [1.0, 0.0, 0.0]
        selector.embedding_cache.set(query, query_embedding)
        selector.embedding_cache.set(bullet.content, bullet_embedding)

        # Calculate score - should use cached embeddings
        score = selector._semantic_score(query, bullet)

        # Identical vectors → score = 1.0
        assert pytest.approx(score, 0.01) == 1.0

        # Verify cache was used (size should still be 2)
        assert selector.embedding_cache.size() == 2

    def test_semantic_score_fallback_on_error(self):
        """Test semantic score works with fallback random embeddings."""
        selector = BulletSelector()
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Test",
        )

        # Don't pre-populate cache - will use generate_embeddings
        # which has a fallback to random embeddings when API fails
        score = selector._semantic_score("query", bullet)

        # Should return a valid score (0.0 to 1.0) even with fallback embeddings
        assert 0.0 <= score <= 1.0

        # Verify embeddings were cached for future use
        assert selector.embedding_cache.size() == 2  # query + bullet

    def test_select_with_semantic_similarity(self):
        """Test selection prioritizes semantically relevant bullets."""
        selector = BulletSelector(weights={
            "effectiveness": 0.0,  # Disable effectiveness
            "recency": 0.0,         # Disable recency
            "semantic": 1.0,        # Only semantic
        })
        now = datetime.now(timezone.utc)

        query = "authentication bug"
        bullets = [
            Bullet(
                id="b_relevant",
                section="Test",
                content="Fix authentication error handling",
                helpful=0,  # No effectiveness data
                updated_at=now.isoformat(),
            ),
            Bullet(
                id="b_irrelevant",
                section="Test",
                content="Optimize CSS styling",
                helpful=0,
                updated_at=now.isoformat(),
            ),
        ]

        # Mock embeddings - relevant bullet similar to query
        query_emb = [1.0, 0.0, 0.0]
        relevant_emb = [0.9, 0.1, 0.0]  # Similar
        irrelevant_emb = [0.0, 1.0, 0.0]  # Orthogonal
        selector.embedding_cache.set(query, query_emb)
        selector.embedding_cache.set(bullets[0].content, relevant_emb)
        selector.embedding_cache.set(bullets[1].content, irrelevant_emb)

        selected = selector.select(bullets, max_count=1, query=query)

        # Should select the semantically relevant bullet
        assert len(selected) == 1
        assert selected[0].id == "b_relevant"

    def test_hybrid_scoring_all_factors(self):
        """Test hybrid scoring combines all three factors."""
        selector = BulletSelector(weights={
            "effectiveness": 0.4,
            "recency": 0.3,
            "semantic": 0.3,
        })
        now = datetime.now(timezone.utc)
        old_date = now - timedelta(days=30)

        query = "fix bug"
        bullets = [
            Bullet(
                id="b_balanced",
                section="Test",
                content="Fix authentication bug",
                helpful=5,
                harmful=5,  # Medium effectiveness (0.5)
                updated_at=now.isoformat(),  # Recent (1.0)
            ),
            Bullet(
                id="b_effective_old",
                section="Test",
                content="Handle errors properly",
                helpful=10,
                harmful=0,  # High effectiveness (1.0)
                updated_at=old_date.isoformat(),  # Old (0.25)
            ),
        ]

        # Mock embeddings
        query_emb = [1.0, 0.0, 0.0]
        balanced_emb = [0.9, 0.1, 0.0]  # High semantic (≈0.95)
        effective_emb = [0.5, 0.5, 0.0]  # Medium semantic (≈0.75)
        selector.embedding_cache.set(query, query_emb)
        selector.embedding_cache.set(bullets[0].content, balanced_emb)
        selector.embedding_cache.set(bullets[1].content, effective_emb)

        scored_bullets = [selector._score_bullet(b, query) for b in bullets]

        # Verify all factors are included in breakdown
        for sb in scored_bullets:
            assert "effectiveness" in sb.score_breakdown
            assert "recency" in sb.score_breakdown
            assert "semantic" in sb.score_breakdown
            assert sb.score_breakdown["semantic"] > 0  # Semantic should be calculated

    def test_semantic_scoring_disabled_without_query(self):
        """Test semantic scoring is disabled when no query provided."""
        selector = BulletSelector()
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Test bullet",
            helpful=5,
        )

        # Score without query
        scored = selector._score_bullet(bullet, query=None)

        # Semantic score should be 0.0
        assert scored.score_breakdown["semantic"] == 0.0

    def test_semantic_scoring_disabled_with_zero_weight(self):
        """Test semantic scoring is skipped when weight is 0."""
        selector = BulletSelector(weights={
            "effectiveness": 0.7,
            "recency": 0.3,
            "semantic": 0.0,  # Disabled
        })
        bullet = Bullet(
            id="b1",
            section="Test",
            content="Test bullet",
        )

        # Score with query but zero weight
        scored = selector._score_bullet(bullet, query="test query")

        # Semantic score should be 0.0 (not calculated)
        assert scored.score_breakdown["semantic"] == 0.0

    def test_embedding_cache_persistence(self):
        """Test embedding cache can be saved and loaded."""
        cache = selector = BulletSelector().embedding_cache

        # Add some embeddings
        cache.set("text1", [1.0, 0.0, 0.0])
        cache.set("text2", [0.0, 1.0, 0.0])

        # Serialize
        cache_dict = cache.to_dict()

        # Create new cache from serialized data
        restored_cache = type(cache).from_dict(cache_dict)

        # Verify embeddings are restored
        assert restored_cache.get("text1") == [1.0, 0.0, 0.0]
        assert restored_cache.get("text2") == [0.0, 1.0, 0.0]
        assert restored_cache.size() == 2

    def test_embedding_cache_file_persistence(self, tmp_path):
        """Test embedding cache can be saved to and loaded from file."""
        import tempfile
        cache_file = tmp_path / "test_cache.json"

        # Create cache and add embeddings
        cache = BulletSelector().embedding_cache
        cache.set("query1", [1.0, 0.0, 0.0])
        cache.set("bullet1", [0.9, 0.1, 0.0])

        # Save to file
        cache.save_to_file(str(cache_file))

        # Verify file exists
        assert cache_file.exists()

        # Load from file
        from swecli.core.context_engineering.memory.embeddings import EmbeddingCache
        loaded_cache = EmbeddingCache.load_from_file(str(cache_file))

        # Verify embeddings are restored
        assert loaded_cache is not None
        assert loaded_cache.get("query1") == [1.0, 0.0, 0.0]
        assert loaded_cache.get("bullet1") == [0.9, 0.1, 0.0]
        assert loaded_cache.size() == 2

    def test_selector_with_cache_file(self, tmp_path):
        """Test BulletSelector with cache file persistence."""
        cache_file = tmp_path / "selector_cache.json"
        now = datetime.now(timezone.utc)

        # Create selector with cache file
        selector = BulletSelector(cache_file=str(cache_file))

        # Pre-populate cache
        query = "test query"
        bullet = Bullet(
            id="b1",
            section="Test",
            content="test content",
            updated_at=now.isoformat(),
        )

        query_emb = [1.0, 0.0, 0.0]
        bullet_emb = [1.0, 0.0, 0.0]
        selector.embedding_cache.set(query, query_emb)
        selector.embedding_cache.set(bullet.content, bullet_emb)

        # Select bullets (this should trigger save)
        bullets = [bullet]
        selected = selector.select(bullets, max_count=1, query=query)

        # Verify cache file was created
        assert cache_file.exists()

        # Create new selector that loads from cache
        selector2 = BulletSelector(cache_file=str(cache_file))

        # Verify cache was loaded
        assert selector2.embedding_cache.size() == 2
        assert selector2.embedding_cache.get(query) == query_emb
        assert selector2.embedding_cache.get(bullet.content) == bullet_emb


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
