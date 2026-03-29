use super::*;

#[test]
fn test_toast_expiry() {
    let toast = Toast::new("test", ToastLevel::Info).with_duration(Duration::from_millis(0));
    assert!(toast.is_expired());
}

#[test]
fn test_toast_not_expired() {
    let toast = Toast::new("test", ToastLevel::Info).with_duration(Duration::from_secs(10));
    assert!(!toast.is_expired());
}

#[test]
fn test_toast_opacity_full() {
    let toast = Toast::new("test", ToastLevel::Success).with_duration(Duration::from_secs(10));
    assert!((toast.opacity() - 1.0).abs() < 0.01);
}

#[test]
fn test_toast_level_colors() {
    assert_ne!(ToastLevel::Info.color(), ToastLevel::Error.color());
    assert_ne!(ToastLevel::Success.icon(), ToastLevel::Warning.icon());
}
