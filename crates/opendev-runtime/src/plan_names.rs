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
#[path = "plan_names_tests.rs"]
mod tests;
