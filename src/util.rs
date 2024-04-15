//! Utilities
use std::num::{NonZeroU64, ParseIntError};
use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use warp::http::Uri;

/// Parse a non-zero [Duration] provided in milliseconds
pub fn parse_millis(s: &str) -> Result<Duration, ParseIntError> {
    s.parse().map(NonZeroU64::get).map(Duration::from_millis)
}

/// Parse a non-zero [Duration] provided in seconds
pub fn parse_secs(s: &str) -> Result<Duration, ParseIntError> {
    s.parse().map(NonZeroU64::get).map(Duration::from_secs)
}

/// Serialize a [Duration] as a number of milliseconds
pub fn serialize_millis<S: Serializer>(
    duration: &Duration,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    duration.as_millis().serialize(serializer)
}

/// Deserialize a non-zero [Duration] provided in milliseconds
pub fn deserialize_millis<'d, D: Deserializer<'d>>(deserializer: D) -> Result<Duration, D::Error> {
    NonZeroU64::deserialize(deserializer)
        .map(NonZeroU64::get)
        .map(Duration::from_millis)
}

/// Deserialize a `Option<Duration>` provided in milliseconds
pub fn deserialize_opt_millis<'d, D: Deserializer<'d>>(
    deserializer: D,
) -> Result<Option<Duration>, D::Error> {
    let res = Option::deserialize(deserializer)?
        .map(NonZeroU64::get)
        .map(Duration::from_millis);
    Ok(res)
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
        .map_err(D::Error::custom)
}

/// Sanitize the given URI, making it usable as a base URI
fn sanitize_base_uri(uri: Uri) -> anyhow::Result<Uri> {
    anyhow::ensure!(uri.query().is_none(), "Base URI '{uri}' has query!",);

    if !uri.path().ends_with('/') {
        format!("{uri}/")
            .try_into()
            .context("Could not sanitize base URI")
    } else {
        Ok(uri)
    }
}

/// Rejection for failure to retrieve an [nvml_wrapper::Device]
#[derive(Debug)]
pub struct DeviceRetrievalError(pub u32);

impl warp::reject::Reject for DeviceRetrievalError {}
