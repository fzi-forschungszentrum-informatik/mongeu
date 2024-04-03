//! Utilities
use std::num::{NonZeroU64, ParseIntError};
use std::time::Duration;

use serde::{Deserialize, Deserializer};

/// Parse a non-zero [Duration] provided in milliseconds
pub fn parse_millis(s: &str) -> Result<Duration, ParseIntError> {
    s.parse().map(NonZeroU64::get).map(Duration::from_millis)
}

/// Parse a non-zero [Duration] provided in seconds
pub fn parse_secs(s: &str) -> Result<Duration, ParseIntError> {
    s.parse().map(NonZeroU64::get).map(Duration::from_secs)
}

/// Deserialize a non-zero [Duration] provided in milliseconds
pub fn deserialize_millis<'d, D: Deserializer<'d>>(deserializer: D) -> Result<Duration, D::Error> {
    NonZeroU64::deserialize(deserializer)
        .map(NonZeroU64::get)
        .map(Duration::from_millis)
}

/// Deserialize a non-zero [Duration] provided in seconds
pub fn deserialize_secs<'d, D: Deserializer<'d>>(deserializer: D) -> Result<Duration, D::Error> {
    NonZeroU64::deserialize(deserializer)
        .map(NonZeroU64::get)
        .map(Duration::from_secs)
}

/// Deserialize an [warp::http::Uri]
pub fn deserialize_uri<'d, D: Deserializer<'d>>(
    deserializer: D,
) -> Result<warp::http::Uri, D::Error> {
    use serde::de::Error;

    String::deserialize(deserializer)?
        .try_into()
        .map_err(|e| D::Error::custom(e))
}
