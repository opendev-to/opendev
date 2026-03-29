use super::*;

#[test]
fn test_spinner_cycles() {
    let mut spinner = SpinnerState::new();
    let first = spinner.tick();
    assert_eq!(first, '\u{280b}');

    // Cycle through all frames
    for _ in 1..SPINNER_FRAMES.len() {
        spinner.tick();
    }
    // Should be back to first frame
    assert_eq!(spinner.current(), '\u{280b}');
}

#[test]
fn test_spinner_tick_count() {
    let mut spinner = SpinnerState::new();
    assert_eq!(spinner.tick_count(), 0);
    spinner.tick();
    spinner.tick();
    assert_eq!(spinner.tick_count(), 2);
}

#[test]
fn test_spinner_reset() {
    let mut spinner = SpinnerState::new();
    spinner.tick();
    spinner.tick();
    spinner.reset();
    assert_eq!(spinner.tick_count(), 0);
    assert_eq!(spinner.current(), SPINNER_FRAMES[0]);
    assert_eq!(spinner.current_verb(), "Thinking"); // reset to first verb
    assert!((spinner.verb_fade_intensity() - 0.0).abs() < f32::EPSILON); // tick 0 = dim
}

#[test]
fn test_verb_advances() {
    let mut spinner = SpinnerState::new();
    let first_verb = THINKING_VERBS[0];
    // Tick past the first verb's full cycle
    let cycle = thinking_verbs::cycle_ticks_for(first_verb);
    for _ in 0..cycle {
        spinner.tick();
    }
    // Should now be on a different verb
    assert_ne!(spinner.current_verb(), first_verb);
}

#[test]
fn test_verb_no_immediate_repeat() {
    let mut spinner = SpinnerState::new();
    let first_idx = spinner.verb_index;
    let cycle = thinking_verbs::cycle_ticks_for(THINKING_VERBS[first_idx]);
    for _ in 0..cycle {
        spinner.tick();
    }
    assert_ne!(spinner.verb_index, first_idx);
}
