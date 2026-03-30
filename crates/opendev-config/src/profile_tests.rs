use super::*;

#[test]
fn test_profile_from_str() {
    assert_eq!(Profile::from_str_loose("dev"), Some(Profile::Dev));
    assert_eq!(Profile::from_str_loose("DEV"), Some(Profile::Dev));
    assert_eq!(Profile::from_str_loose("development"), Some(Profile::Dev));
    assert_eq!(Profile::from_str_loose("prod"), Some(Profile::Prod));
    assert_eq!(Profile::from_str_loose("production"), Some(Profile::Prod));
    assert_eq!(Profile::from_str_loose("fast"), Some(Profile::Fast));
    assert_eq!(Profile::from_str_loose("quick"), Some(Profile::Fast));
    assert_eq!(Profile::from_str_loose("unknown"), None);
}

#[test]
fn test_profile_roundtrip() {
    for p in Profile::all() {
        let s = p.as_str();
        let parsed = Profile::from_str_loose(s).unwrap();
        assert_eq!(*p, parsed);
    }
}

#[test]
fn test_apply_dev_profile() {
    let mut config = AppConfig::default();
    config.verbose = false;
    config.debug_logging = false;

    let applied = apply_profile(&mut config, "dev");
    assert!(applied);
    assert!(config.verbose);
    assert!(config.debug_logging);
}

#[test]
fn test_apply_prod_profile() {
    let mut config = AppConfig::default();
    config.verbose = true;

    let applied = apply_profile(&mut config, "prod");
    assert!(applied);
    assert!(!config.verbose);
    // debug_logging stays on (always-on by default)
    assert!(config.debug_logging);
}

#[test]
fn test_apply_fast_profile() {
    let mut config = AppConfig::default();
    config.max_tokens = 16384;
    config.temperature = 0.6;

    let applied = apply_profile(&mut config, "fast");
    assert!(applied);
    assert_eq!(config.max_tokens, 4096);
    assert!((config.temperature - 0.8).abs() < f64::EPSILON);
}

#[test]
fn test_apply_fast_profile_preserves_small_max_tokens() {
    let mut config = AppConfig::default();
    config.max_tokens = 2048;

    apply_profile(&mut config, "fast");
    assert_eq!(config.max_tokens, 2048); // Not increased
}

#[test]
fn test_apply_unknown_profile() {
    let mut config = AppConfig::default();
    let original = config.clone();

    let applied = apply_profile(&mut config, "nonexistent");
    assert!(!applied);
    // Config should be unchanged
    assert_eq!(config.verbose, original.verbose);
    assert_eq!(config.debug_logging, original.debug_logging);
}
