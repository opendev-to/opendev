use super::*;

fn make_bullet(id: &str, section: &str, helpful: i64, harmful: i64) -> Bullet {
    Bullet {
        id: id.to_string(),
        section: section.to_string(),
        content: format!("Content of {id}"),
        helpful,
        harmful,
        neutral: 0,
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}

fn make_old_bullet(id: &str, days_ago: i64) -> Bullet {
    let ts = (chrono::Utc::now() - chrono::Duration::days(days_ago)).to_rfc3339();
    Bullet {
        id: id.to_string(),
        section: "test".to_string(),
        content: format!("Content of {id}"),
        helpful: 0,
        harmful: 0,
        neutral: 0,
        created_at: ts.clone(),
        updated_at: ts,
    }
}

#[test]
fn test_select_returns_all_when_under_limit() {
    let selector = BulletSelector::default();
    let bullets = vec![
        make_bullet("a", "test", 0, 0),
        make_bullet("b", "test", 0, 0),
    ];
    let selected = selector.select(&bullets, 5, None);
    assert_eq!(selected.len(), 2);
}

#[test]
fn test_select_limits_to_max_count() {
    let selector = BulletSelector::default();
    let bullets: Vec<Bullet> = (0..10)
        .map(|i| make_bullet(&format!("b-{i}"), "test", i, 0))
        .collect();

    let selected = selector.select(&bullets, 3, None);
    assert_eq!(selected.len(), 3);
}

#[test]
fn test_select_prefers_helpful_bullets() {
    let selector = BulletSelector::default();
    let bullets = vec![
        make_bullet("low", "test", 0, 5),      // harmful
        make_bullet("high", "test", 10, 0),    // very helpful
        make_bullet("mid", "test", 3, 3),      // mixed
        make_bullet("untested", "test", 0, 0), // neutral (0.5)
    ];

    let selected = selector.select(&bullets, 2, None);
    let ids: Vec<&str> = selected.iter().map(|b| b.id.as_str()).collect();
    assert!(ids.contains(&"high"));
}

#[test]
fn test_effectiveness_score() {
    let selector = BulletSelector::default();

    // Untested bullet -> 0.5
    let untested = make_bullet("u", "t", 0, 0);
    assert!((selector.effectiveness_score(&untested) - 0.5).abs() < 1e-10);

    // All helpful -> 1.0
    let helpful = make_bullet("h", "t", 10, 0);
    assert!((selector.effectiveness_score(&helpful) - 1.0).abs() < 1e-10);

    // All harmful -> 0.0
    let harmful = make_bullet("x", "t", 0, 10);
    assert!((selector.effectiveness_score(&harmful) - 0.0).abs() < 1e-10);

    // Equal helpful/harmful -> 0.5
    let mixed = make_bullet("m", "t", 5, 5);
    assert!((selector.effectiveness_score(&mixed) - 0.5).abs() < 1e-10);
}

#[test]
fn test_recency_score() {
    let selector = BulletSelector::default();

    // Recent bullet should score high
    let recent = make_old_bullet("r", 0);
    let recent_score = selector.recency_score(&recent);
    assert!(recent_score > 0.9);

    // Old bullet should score low
    let old = make_old_bullet("o", 30);
    let old_score = selector.recency_score(&old);
    assert!(old_score < 0.3);

    // Recent > Old
    assert!(recent_score > old_score);
}

#[test]
fn test_recency_score_invalid_timestamp() {
    let selector = BulletSelector::default();
    let mut bullet = make_bullet("b", "t", 0, 0);
    bullet.updated_at = "not-a-date".to_string();
    assert!((selector.recency_score(&bullet) - 0.5).abs() < 1e-10);
}

#[test]
fn test_selection_stats() {
    let selector = BulletSelector::default();
    let all = vec![
        make_bullet("a", "t", 10, 0),
        make_bullet("b", "t", 0, 10),
        make_bullet("c", "t", 5, 5),
    ];
    let selected = vec![all[0].clone()];

    let stats = selector.selection_stats(&all, &selected);
    assert_eq!(stats["total_bullets"], 3.0);
    assert_eq!(stats["selected_bullets"], 1.0);
    assert!(stats["score_improvement"] > 0.0);
}

#[test]
fn test_score_bullet_breakdown() {
    let selector = BulletSelector::default();
    let bullet = make_bullet("b", "test", 5, 0);

    let scored = selector.score_bullet(&bullet, None);
    assert!(scored.score_breakdown.contains_key("effectiveness"));
    assert!(scored.score_breakdown.contains_key("recency"));
    assert!(scored.score_breakdown.contains_key("semantic"));
    assert_eq!(scored.score_breakdown["semantic"], 0.0);
}
