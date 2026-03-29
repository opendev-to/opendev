//! Flexible datetime (de)serialization for Python compatibility.
//!
//! Python's `datetime.isoformat()` produces strings like `2024-06-15T10:30:00`
//! (no timezone), while `chrono::DateTime<Utc>` expects RFC3339 with `Z` or offset.
//! This module handles both formats.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{self, Deserialize, Deserializer, Serializer};

const FORMAT: &str = "%Y-%m-%dT%H:%M:%S";
const FORMAT_WITH_FRAC: &str = "%Y-%m-%dT%H:%M:%S%.f";

/// Serialize a `DateTime<Utc>` as ISO 8601 string.
pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&date.to_rfc3339())
}

/// Deserialize a datetime string that may or may not have timezone info.
pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_flexible_datetime(&s).map_err(serde::de::Error::custom)
}

/// Parse a datetime string flexibly, handling:
/// - RFC3339 with Z: `2024-06-15T10:30:00Z`
/// - RFC3339 with offset: `2024-06-15T10:30:00+00:00`
/// - Naive (no timezone): `2024-06-15T10:30:00` (assumed UTC)
/// - With fractional seconds: `2024-06-15T10:30:00.123456`
pub fn parse_flexible_datetime(s: &str) -> Result<DateTime<Utc>, String> {
    // Try RFC3339 first (has timezone)
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Try naive datetime with fractional seconds
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, FORMAT_WITH_FRAC) {
        return Ok(naive.and_utc());
    }

    // Try naive datetime without fractional seconds
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, FORMAT) {
        return Ok(naive.and_utc());
    }

    Err(format!("Cannot parse datetime: {s}"))
}

/// Module for optional datetime fields.
pub mod option {
    use chrono::{DateTime, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(date: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match date {
            Some(dt) => serializer.serialize_str(&dt.to_rfc3339()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => super::parse_flexible_datetime(&s)
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
#[path = "datetime_compat_tests.rs"]
mod tests;
