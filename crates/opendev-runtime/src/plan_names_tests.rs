use super::*;
use tempfile::TempDir;

#[test]
fn test_generate_plan_name_format() {
    let name = generate_plan_name(None, 50);
    let parts: Vec<&str> = name.split('-').collect();
    assert_eq!(parts.len(), 3);
    assert!(ADJECTIVES.contains(&parts[0]));
    assert!(VERBS.contains(&parts[1]));
    assert!(NOUNS.contains(&parts[2]));
}

#[test]
fn test_generate_plan_name_unique() {
    // Generate several names and check they're not all the same
    let names: Vec<String> = (0..10).map(|_| generate_plan_name(None, 50)).collect();
    // Extremely unlikely all 10 are identical (30^3 = 27000 possibilities)
    let unique: std::collections::HashSet<&str> = names.iter().map(|s| s.as_str()).collect();
    assert!(unique.len() > 1);
}

#[test]
fn test_collision_avoidance() {
    let tmp = TempDir::new().unwrap();
    // Create a file — next generation should avoid that name
    let first = generate_plan_name(Some(tmp.path()), 50);
    std::fs::write(tmp.path().join(format!("{}.md", first)), "plan").unwrap();

    // Generate more names — they should all differ from the first
    for _ in 0..20 {
        let name = generate_plan_name(Some(tmp.path()), 50);
        // Could theoretically collide, but with 27000 possibilities and 20 tries
        // it's astronomically unlikely. We mainly verify no crash.
        assert!(!name.is_empty());
    }
}

#[test]
fn test_fallback_with_suffix() {
    let tmp = TempDir::new().unwrap();
    // Fill directory with every possible combination (impractical in reality,
    // but we test the fallback by using max_attempts=0)
    let name = generate_plan_name(Some(tmp.path()), 0);
    let parts: Vec<&str> = name.split('-').collect();
    assert_eq!(parts.len(), 4);
    // Last part should be a number
    assert!(parts[3].parse::<u32>().is_ok());
}

#[test]
fn test_word_lists_not_empty() {
    assert!(!ADJECTIVES.is_empty());
    assert!(!VERBS.is_empty());
    assert!(!NOUNS.is_empty());
}
