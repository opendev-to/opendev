//! Bullet selection logic for ACE playbook context optimization.
//!
//! Mirrors `opendev/core/context_engineering/memory/selector.py`.

use std::collections::HashMap;

use crate::embeddings::{EmbeddingCache, cosine_similarity};
use crate::playbook::Bullet;

/// Bullet with its calculated relevance score.
#[derive(Debug, Clone)]
pub struct ScoredBullet {
    pub bullet: Bullet,
    pub score: f64,
    pub score_breakdown: HashMap<String, f64>,
}

/// Selects most relevant bullets for a given query.
///
/// Implements hybrid retrieval with three scoring factors:
/// - Effectiveness: Based on helpful/harmful feedback
/// - Recency: Prefers recently updated bullets
/// - Semantic: Query-to-bullet similarity using embeddings
pub struct BulletSelector {
    pub weights: HashMap<String, f64>,
    pub embedding_model: String,
    pub cache_file: Option<String>,
    pub embedding_cache: EmbeddingCache,
}

impl BulletSelector {
    /// Create a new bullet selector.
    pub fn new(
        weights: Option<HashMap<String, f64>>,
        embedding_model: &str,
        cache_file: Option<&str>,
    ) -> Self {
        let weights = weights.unwrap_or_else(|| {
            let mut w = HashMap::new();
            w.insert("effectiveness".to_string(), 0.6);
            w.insert("recency".to_string(), 0.4);
            w.insert("semantic".to_string(), 0.0);
            w
        });

        let embedding_cache = cache_file
            .and_then(|p| EmbeddingCache::load_from_file(std::path::Path::new(p)))
            .unwrap_or_else(|| EmbeddingCache::new(embedding_model));

        Self {
            weights,
            embedding_model: embedding_model.to_string(),
            cache_file: cache_file.map(String::from),
            embedding_cache,
        }
    }

    /// Select top-K most relevant bullets.
    pub fn select(&self, bullets: &[Bullet], max_count: usize, query: Option<&str>) -> Vec<Bullet> {
        if bullets.len() <= max_count {
            return bullets.to_vec();
        }

        let mut scored: Vec<ScoredBullet> = bullets
            .iter()
            .map(|b| self.score_bullet(b, query))
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored
            .into_iter()
            .take(max_count)
            .map(|sb| sb.bullet)
            .collect()
    }

    /// Score a single bullet.
    pub fn score_bullet(&self, bullet: &Bullet, query: Option<&str>) -> ScoredBullet {
        let mut breakdown = HashMap::new();

        let effectiveness = self.effectiveness_score(bullet);
        breakdown.insert("effectiveness".to_string(), effectiveness);

        let recency = self.recency_score(bullet);
        breakdown.insert("recency".to_string(), recency);

        let semantic = match query {
            Some(q) if self.weights.get("semantic").copied().unwrap_or(0.0) > 0.0 => {
                self.semantic_score(q, bullet)
            }
            _ => 0.0,
        };
        breakdown.insert("semantic".to_string(), semantic);

        let final_score = self.weights.get("effectiveness").unwrap_or(&0.6) * effectiveness
            + self.weights.get("recency").unwrap_or(&0.4) * recency
            + self.weights.get("semantic").unwrap_or(&0.0) * semantic;

        ScoredBullet {
            bullet: bullet.clone(),
            score: final_score,
            score_breakdown: breakdown,
        }
    }

    /// Effectiveness score based on helpful/harmful feedback.
    /// Returns 0.0..1.0. Untested bullets get 0.5.
    fn effectiveness_score(&self, bullet: &Bullet) -> f64 {
        let total = bullet.helpful + bullet.harmful + bullet.neutral;
        if total == 0 {
            return 0.5;
        }
        let weighted =
            bullet.helpful as f64 * 1.0 + bullet.neutral as f64 * 0.5 + bullet.harmful as f64 * 0.0;
        weighted / total as f64
    }

    /// Recency score -- prefer recently updated bullets.
    /// Returns 0.0..1.0 using exponential decay.
    fn recency_score(&self, bullet: &Bullet) -> f64 {
        let updated_at = bullet
            .updated_at
            .replace("Z", "+00:00")
            .parse::<chrono::DateTime<chrono::Utc>>()
            .or_else(|_| {
                chrono::DateTime::parse_from_rfc3339(&bullet.updated_at)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            });

        match updated_at {
            Ok(dt) => {
                let days_old = (chrono::Utc::now() - dt).num_days().max(0) as f64;
                let decay_rate = 0.1;
                1.0 / (1.0 + days_old * decay_rate)
            }
            Err(_) => 0.5,
        }
    }

    /// Semantic similarity score using cached embeddings.
    fn semantic_score(&self, query: &str, bullet: &Bullet) -> f64 {
        if self.weights.get("semantic").copied().unwrap_or(0.0) <= 0.0 {
            return 0.0;
        }

        let query_emb = self.embedding_cache.peek(query, None);
        let bullet_emb = self.embedding_cache.peek(&bullet.content, None);

        match (query_emb, bullet_emb) {
            (Some(q), Some(b)) => {
                let sim = cosine_similarity(q, b);
                (sim + 1.0) / 2.0 // Normalize from [-1, 1] to [0, 1]
            }
            _ => 0.5,
        }
    }

    /// Get statistics about a selection.
    pub fn selection_stats(
        &self,
        all_bullets: &[Bullet],
        selected: &[Bullet],
    ) -> HashMap<String, f64> {
        let all_scored: Vec<ScoredBullet> = all_bullets
            .iter()
            .map(|b| self.score_bullet(b, None))
            .collect();

        let selected_ids: std::collections::HashSet<&str> =
            selected.iter().map(|b| b.id.as_str()).collect();

        let avg_all = if all_scored.is_empty() {
            0.0
        } else {
            all_scored.iter().map(|s| s.score).sum::<f64>() / all_scored.len() as f64
        };

        let selected_scores: Vec<f64> = all_scored
            .iter()
            .filter(|s| selected_ids.contains(s.bullet.id.as_str()))
            .map(|s| s.score)
            .collect();

        let avg_selected = if selected_scores.is_empty() {
            0.0
        } else {
            selected_scores.iter().sum::<f64>() / selected_scores.len() as f64
        };

        let mut stats = HashMap::new();
        stats.insert("total_bullets".to_string(), all_bullets.len() as f64);
        stats.insert("selected_bullets".to_string(), selected.len() as f64);
        stats.insert(
            "selection_rate".to_string(),
            if all_bullets.is_empty() {
                0.0
            } else {
                selected.len() as f64 / all_bullets.len() as f64
            },
        );
        stats.insert("avg_all_score".to_string(), avg_all);
        stats.insert("avg_selected_score".to_string(), avg_selected);
        stats.insert("score_improvement".to_string(), avg_selected - avg_all);
        stats
    }
}

impl Default for BulletSelector {
    fn default() -> Self {
        Self::new(None, "text-embedding-3-small", None)
    }
}

#[cfg(test)]
#[path = "selector_tests.rs"]
mod tests;
