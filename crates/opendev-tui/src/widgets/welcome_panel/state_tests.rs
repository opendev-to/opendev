use super::*;

#[test]
fn test_state_tick_gradient() {
    let mut state = WelcomePanelState::new();
    assert_eq!(state.gradient_offset, 0);
    state.tick(80, 24);
    assert_eq!(state.gradient_offset, 3);
    state.tick(80, 24);
    assert_eq!(state.gradient_offset, 6);

    // Wraps at 360
    state.gradient_offset = 358;
    state.tick(80, 24);
    assert_eq!(state.gradient_offset, 1); // (358+3) % 360 = 1
}

#[test]
fn test_braille_cycles() {
    let mut state = WelcomePanelState::new();
    assert_eq!(state.braille_offset, 0);
    state.tick(80, 24); // braille_tick = 1
    assert_eq!(state.braille_offset, 0); // not yet
    state.tick(80, 24); // braille_tick wraps, offset advances
    assert_eq!(state.braille_offset, 1);
}

#[test]
fn test_fade_completes() {
    let mut state = WelcomePanelState::new();
    assert!(!state.fade_complete);
    state.start_fade();
    // fade_progress starts at 1.0, decrements 0.1 per tick
    for _ in 0..10 {
        state.tick(80, 24);
    }
    assert!(state.fade_complete);
    assert!(state.fade_progress <= 0.0);
}

#[test]
fn test_rain_init() {
    let mut state = WelcomePanelState::new();
    state.ensure_rain_field(40, 10);
    assert_eq!(state.rain_columns.len(), 40);
    for col in &state.rain_columns {
        assert!(col.speed >= 0.10);
        assert!(col.speed <= 0.50);
        assert!(col.trail_len >= 4 && col.trail_len <= 9);
    }
}

#[test]
fn test_rain_step() {
    let mut state = WelcomePanelState::new();
    state.ensure_rain_field(5, 10);
    let initial_ys: Vec<f32> = state.rain_columns.iter().map(|c| c.y).collect();
    state.step_rain();
    for (i, col) in state.rain_columns.iter().enumerate() {
        // Either advanced or reset (if it went off-screen)
        assert!(
            col.y != initial_ys[i]
                || col.speed == 0.0
                || initial_ys[i] > 10.0 + col.trail_len as f32
        );
    }
}
