//! Utilities
use std::num::{NonZeroU64, ParseIntError};
use std::time::Duration;

/// Parse a non-zero [Duration] provided in milliseconds
pub fn parse_millis(s: &str) -> Result<Duration, ParseIntError> {
    s.parse().map(NonZeroU64::get).map(Duration::from_millis)
}

/// Parse a non-zero [Duration] provided in seconds
pub fn parse_secs(s: &str) -> Result<Duration, ParseIntError> {
    s.parse().map(NonZeroU64::get).map(Duration::from_secs)
}
