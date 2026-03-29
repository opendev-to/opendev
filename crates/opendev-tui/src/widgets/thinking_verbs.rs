//! Animated thinking verb list and fade-in transition logic.
//!
//! Provides 100+ verbs that cycle during LLM processing with a
//! dim-to-bright fade-in color animation.

/// Ticks per character used to compute the fade-in duration.
/// Longer verbs get a proportionally longer fade. At 60ms tick rate,
/// a 10-char verb fades in over 10×2×60ms = 1.2s.
pub const TICKS_PER_CHAR: u64 = 2;

/// Ticks to hold the fully-visible verb before cycling to the next.
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

/// Compute fade-in intensity (0.0 = fully dim, 1.0 = fully bright).
///
/// During the fade-in phase, intensity ramps linearly from 0.0 to 1.0.
/// After the fade completes, returns 1.0 for the hold duration.
pub fn compute_fade_intensity(verb: &str, verb_tick: u64) -> f32 {
    let fade_ticks = verb.len() as u64 * TICKS_PER_CHAR;
    if fade_ticks == 0 {
        return 1.0;
    }
    (verb_tick as f32 / fade_ticks as f32).min(1.0)
}

/// Advance to the next verb index using a prime step for pseudo-random feel.
pub fn next_verb_index(current: usize) -> usize {
    (current + VERB_STEP) % THINKING_VERBS.len()
}

#[cfg(test)]
#[path = "thinking_verbs_tests.rs"]
mod tests;
