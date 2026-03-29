use super::*;

#[test]
fn test_verb_count() {
    assert!(
        THINKING_VERBS.len() >= 100,
        "Expected 100+ verbs, got {}",
        THINKING_VERBS.len()
    );
}

#[test]
fn test_all_verbs_ascii() {
    for verb in THINKING_VERBS {
        assert!(
            verb.is_ascii(),
            "Verb '{}' contains non-ASCII characters",
            verb
        );
    }
}

#[test]
fn test_fade_intensity_progression() {
    let verb = "Pondering"; // 9 chars, fade_ticks = 18
    // Tick 0: intensity = 0.0
    assert!((compute_fade_intensity(verb, 0) - 0.0).abs() < f32::EPSILON);
    // Tick 9: halfway through fade = 0.5
    assert!((compute_fade_intensity(verb, 9) - 0.5).abs() < f32::EPSILON);
    // Tick 18: fully faded in = 1.0
    assert!((compute_fade_intensity(verb, 18) - 1.0).abs() < f32::EPSILON);
    // Tick 50: still 1.0 during hold
    assert!((compute_fade_intensity(verb, 50) - 1.0).abs() < f32::EPSILON);
}

#[test]
fn test_cycle_ticks() {
    let verb = "Thinking"; // 8 chars
    let expected = 8 * TICKS_PER_CHAR + HOLD_TICKS;
    assert_eq!(cycle_ticks_for(verb), expected);
}

#[test]
fn test_no_immediate_repeat() {
    let first = 0;
    let second = next_verb_index(first);
    assert_ne!(first, second);
    let third = next_verb_index(second);
    assert_ne!(second, third);
}

#[test]
fn test_verb_step_visits_all() {
    // With a prime step coprime to the verb count, we should visit all verbs
    let mut visited = std::collections::HashSet::new();
    let mut idx = 0;
    for _ in 0..THINKING_VERBS.len() {
        visited.insert(idx);
        idx = next_verb_index(idx);
    }
    assert_eq!(
        visited.len(),
        THINKING_VERBS.len(),
        "Prime step should visit all verbs"
    );
}
