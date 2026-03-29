use super::*;

#[test]
fn test_operation_mode_display() {
    assert_eq!(OperationMode::Normal.to_string(), "Normal");
    assert_eq!(OperationMode::Plan.to_string(), "Plan");
}

#[test]
fn test_operation_mode_from_str_loose() {
    assert_eq!(
        OperationMode::from_str_loose("plan"),
        Some(OperationMode::Plan)
    );
    assert_eq!(
        OperationMode::from_str_loose("Normal"),
        Some(OperationMode::Normal)
    );
    assert_eq!(OperationMode::from_str_loose("bogus"), None);
}

#[test]
fn test_autonomy_level_from_str_loose() {
    assert_eq!(
        AutonomyLevel::from_str_loose("auto"),
        Some(AutonomyLevel::Auto)
    );
    assert_eq!(
        AutonomyLevel::from_str_loose("Semi-Auto"),
        Some(AutonomyLevel::SemiAuto)
    );
    assert_eq!(
        AutonomyLevel::from_str_loose("manual"),
        Some(AutonomyLevel::Manual)
    );
    assert_eq!(AutonomyLevel::from_str_loose("bogus"), None);
}

#[test]
fn test_reasoning_level_cycle() {
    assert_eq!(ReasoningLevel::Off.next(), ReasoningLevel::Low);
    assert_eq!(ReasoningLevel::Low.next(), ReasoningLevel::Medium);
    assert_eq!(ReasoningLevel::Medium.next(), ReasoningLevel::High);
    assert_eq!(ReasoningLevel::High.next(), ReasoningLevel::Off);
}

#[test]
fn test_reasoning_level_from_str() {
    assert_eq!(ReasoningLevel::from_str_loose("none"), ReasoningLevel::Off);
    assert_eq!(ReasoningLevel::from_str_loose("low"), ReasoningLevel::Low);
    assert_eq!(
        ReasoningLevel::from_str_loose("medium"),
        ReasoningLevel::Medium
    );
    assert_eq!(ReasoningLevel::from_str_loose("high"), ReasoningLevel::High);
}

#[test]
fn test_reasoning_level_to_config() {
    assert_eq!(ReasoningLevel::Off.to_config_string(), None);
    assert_eq!(
        ReasoningLevel::Low.to_config_string(),
        Some("low".to_string())
    );
    assert_eq!(
        ReasoningLevel::Medium.to_config_string(),
        Some("medium".to_string())
    );
}
