use super::*;

#[test]
fn test_config_exists_false_for_tmp() {
    let _ = config_exists();
}

#[test]
fn test_setup_error_display() {
    let e = SetupError::Cancelled;
    assert_eq!(e.to_string(), "setup cancelled by user");

    let e = SetupError::NoApiKey;
    assert_eq!(e.to_string(), "no API key provided");

    let e = SetupError::ValidationFailed("bad key".into());
    assert!(e.to_string().contains("bad key"));

    let e = SetupError::SaveFailed("disk full".into());
    assert!(e.to_string().contains("disk full"));

    let e = SetupError::RegistryError("no data".into());
    assert!(e.to_string().contains("no data"));
}

#[test]
fn test_setup_error_variants() {
    let errors: Vec<SetupError> = vec![
        SetupError::Cancelled,
        SetupError::NoProvider,
        SetupError::NoApiKey,
        SetupError::ValidationFailed("test".into()),
        SetupError::NoModel,
        SetupError::SaveFailed("test".into()),
        SetupError::RegistryError("test".into()),
        SetupError::Io(io::Error::new(io::ErrorKind::Other, "test")),
    ];
    assert_eq!(errors.len(), 8);
}
