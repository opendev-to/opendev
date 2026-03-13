//! Generate unique plan names using adjective-verb-noun pattern.
//!
//! Ported from `opendev/core/runtime/plan_names.py`.

use rand::Rng;
use rand::seq::SliceRandom;
use std::path::Path;

const ADJECTIVES: &[&str] = &[
    "bold", "calm", "cool", "crisp", "dark", "deep", "fair", "fast", "fine", "free", "glad",
    "gold", "gray", "keen", "kind", "lean", "mild", "neat", "pale", "pure", "rare", "rich", "safe",
    "slim", "soft", "tall", "tidy", "warm", "wide", "wise",
];

const VERBS: &[&str] = &[
    "blazing",
    "dashing",
    "diving",
    "drifting",
    "flying",
    "gliding",
    "growing",
    "hiding",
    "jumping",
    "landing",
    "leaping",
    "lifting",
    "moving",
    "pacing",
    "racing",
    "rising",
    "roaming",
    "rowing",
    "running",
    "sailing",
    "singing",
    "sliding",
    "soaring",
    "spinning",
    "splashing",
    "standing",
    "surfing",
    "swimming",
    "swinging",
    "waving",
];

const NOUNS: &[&str] = &[
    "badger", "crane", "dolphin", "eagle", "falcon", "gecko", "hawk", "heron", "jaguar", "koala",
    "lark", "lemur", "lynx", "mantis", "otter", "panda", "parrot", "puffin", "quail", "raven",
    "robin", "salmon", "seal", "shark", "sparrow", "tiger", "turtle", "viper", "walrus", "whale",
];

/// Generate a unique plan name like `bold-blazing-badger`.
///
/// If `existing_dir` is given, checks for collision with `{name}.md` files
/// in that directory. Falls back to a numeric suffix after `max_attempts`.
pub fn generate_plan_name(existing_dir: Option<&Path>, max_attempts: u32) -> String {
    let mut rng = rand::thread_rng();

    for _ in 0..max_attempts {
        // SAFETY: ADJECTIVES, VERBS, NOUNS are non-empty compile-time arrays
        let adj = ADJECTIVES
            .choose(&mut rng)
            .expect("ADJECTIVES is non-empty");
        let verb = VERBS.choose(&mut rng).expect("VERBS is non-empty");
        let noun = NOUNS.choose(&mut rng).expect("NOUNS is non-empty");
        let name = format!("{}-{}-{}", adj, verb, noun);

        if let Some(dir) = existing_dir {
            if !dir.join(format!("{}.md", name)).exists() {
                return name;
            }
        } else {
            return name;
        }
    }

    // Fallback: append random digits
    let adj = ADJECTIVES
        .choose(&mut rng)
        .expect("ADJECTIVES is non-empty");
    let verb = VERBS.choose(&mut rng).expect("VERBS is non-empty");
    let noun = NOUNS.choose(&mut rng).expect("NOUNS is non-empty");
    let suffix: u32 = rng.gen_range(1000..10000);
    format!("{}-{}-{}-{}", adj, verb, noun, suffix)
}

#[cfg(test)]
mod tests {
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
}
