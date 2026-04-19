use super::*;
use chrono::Timelike;

#[test]
fn test_parse_rfc3339() {
    let dt = parse_flexible_datetime("2024-06-15T10:30:00Z").unwrap();
    assert_eq!(dt.to_rfc3339(), "2024-06-15T10:30:00+00:00");
}

#[test]
fn test_parse_naive() {
    let dt = parse_flexible_datetime("2024-06-15T10:30:00").unwrap();
    assert_eq!(dt.to_rfc3339(), "2024-06-15T10:30:00+00:00");
}

#[test]
fn test_parse_with_offset() {
    let dt = parse_flexible_datetime("2024-06-15T10:30:00+05:30").unwrap();
    assert_eq!(dt.hour(), 5); // 10:30 IST = 05:00 UTC
}

#[test]
fn test_parse_fractional() {
    let dt = parse_flexible_datetime("2024-06-15T10:30:00.123456").unwrap();
    assert!(dt.to_rfc3339().starts_with("2024-06-15T10:30:00"));
}

#[test]
fn test_parse_invalid() {
    assert!(parse_flexible_datetime("not-a-date").is_err());
}
