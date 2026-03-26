//! Animated thinking verb list and typewriter reveal logic.
//!
//! Provides 100+ verbs that cycle during LLM processing with a
//! character-by-character typewriter reveal animation.

/// Ticks per character during the typewriter reveal phase.
/// At 60ms tick rate, this means 120ms per character.
pub const TICKS_PER_CHAR: u64 = 2;

/// Ticks to hold the fully-revealed verb before cycling to the next.
/// At 60ms tick rate, this is ~3 seconds.
pub const HOLD_TICKS: u64 = 50;

/// Prime step for pseudo-random verb advancement (avoids sequential cycling).
const VERB_STEP: usize = 37;

/// 100+ thinking verbs for the animated spinner.
pub const THINKING_VERBS: &[&str] = &[
    "Thinking",
    "Pondering",
    "Reasoning",
    "Contemplating",
    "Analyzing",
    "Deliberating",
    "Considering",
    "Evaluating",
    "Processing",
    "Reflecting",
    "Musing",
    "Mulling",
    "Weighing",
    "Computing",
    "Calculating",
    "Formulating",
    "Synthesizing",
    "Deducing",
    "Inferring",
    "Hypothesizing",
    "Investigating",
    "Exploring",
    "Examining",
    "Studying",
    "Reviewing",
    "Assessing",
    "Appraising",
    "Scrutinizing",
    "Parsing",
    "Deciphering",
    "Decoding",
    "Interpreting",
    "Comprehending",
    "Absorbing",
    "Digesting",
    "Distilling",
    "Crystallizing",
    "Brainstorming",
    "Ideating",
    "Conceiving",
    "Imagining",
    "Envisioning",
    "Visualizing",
    "Mapping",
    "Charting",
    "Planning",
    "Strategizing",
    "Architecting",
    "Designing",
    "Structuring",
    "Organizing",
    "Prioritizing",
    "Optimizing",
    "Refining",
    "Polishing",
    "Iterating",
    "Converging",
    "Connecting",
    "Linking",
    "Bridging",
    "Harmonizing",
    "Balancing",
    "Calibrating",
    "Tuning",
    "Aligning",
    "Orchestrating",
    "Assembling",
    "Composing",
    "Crafting",
    "Building",
    "Constructing",
    "Modeling",
    "Simulating",
    "Prototyping",
    "Experimenting",
    "Validating",
    "Verifying",
    "Researching",
    "Probing",
    "Querying",
    "Searching",
    "Surveying",
    "Cataloging",
    "Sorting",
    "Filtering",
    "Curating",
    "Selecting",
    "Extrapolating",
    "Interpolating",
    "Correlating",
    "Aggregating",
    "Abstracting",
    "Generalizing",
    "Speculating",
    "Ruminating",
    "Cogitating",
    "Meditating",
    "Introspecting",
    "Rationalizing",
    "Theorizing",
    "Philosophizing",
    "Conceptualizing",
    "Untangling",
    "Unraveling",
    "Deciphering",
    "Navigating",
    "Traversing",
    "Excavating",
    "Unearthing",
    "Discovering",
    "Uncovering",
    "Illuminating",
    "Elucidating",
    "Clarifying",
    "Demystifying",
    "Simplifying",
    "Consolidating",
    "Integrating",
    "Reconciling",
    "Resolving",
    "Debugging",
    "Diagnosing",
    "Dissecting",
    "Deconstructing",
];

/// Total ticks for one verb's full animation cycle (reveal + hold).
pub fn cycle_ticks_for(verb: &str) -> u64 {
    verb.len() as u64 * TICKS_PER_CHAR + HOLD_TICKS
}

/// Compute the visible portion of a verb at the given tick within its cycle.
///
/// During the reveal phase, returns a prefix slice of the verb.
/// After full reveal, returns the complete verb.
pub fn compute_verb_text(verb: &str, verb_tick: u64) -> &str {
    let chars_revealed = (verb_tick / TICKS_PER_CHAR) as usize;
    if chars_revealed >= verb.len() {
        verb
    } else {
        // All verbs are ASCII, so byte slicing is safe
        &verb[..chars_revealed.max(1)]
    }
}

/// Whether the verb is fully revealed at the given tick.
pub fn is_fully_revealed(verb: &str, verb_tick: u64) -> bool {
    (verb_tick / TICKS_PER_CHAR) as usize >= verb.len()
}

/// Advance to the next verb index using a prime step for pseudo-random feel.
pub fn next_verb_index(current: usize) -> usize {
    (current + VERB_STEP) % THINKING_VERBS.len()
}

#[cfg(test)]
mod tests {
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
            assert!(verb.is_ascii(), "Verb '{}' contains non-ASCII characters", verb);
        }
    }

    #[test]
    fn test_typewriter_progression() {
        let verb = "Pondering";
        // Tick 0-1: show first char (0/2=0, clamped to 1)
        assert_eq!(compute_verb_text(verb, 0), "P");
        assert_eq!(compute_verb_text(verb, 1), "P");
        // Tick 2-3: still one char (2/2=1)
        assert_eq!(compute_verb_text(verb, 2), "P");
        assert_eq!(compute_verb_text(verb, 3), "P");
        // Tick 4-5: two chars (4/2=2)
        assert_eq!(compute_verb_text(verb, 4), "Po");
        assert_eq!(compute_verb_text(verb, 5), "Po");
        // Tick 6: three chars (6/2=3)
        assert_eq!(compute_verb_text(verb, 6), "Pon");
    }

    #[test]
    fn test_full_reveal() {
        let verb = "Pondering"; // 9 chars
        let reveal_tick = 9 * TICKS_PER_CHAR; // 18
        assert_eq!(compute_verb_text(verb, reveal_tick), "Pondering");
        assert!(is_fully_revealed(verb, reveal_tick));
        assert!(!is_fully_revealed(verb, reveal_tick - 1));
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
}
