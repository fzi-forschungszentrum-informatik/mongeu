//! Utilities for handling (request) parameters of all kind
use serde::Deserialize;

use crate::util;

/// Helper type for representing a duration in `ms` in a paramater
#[derive(Copy, Clone, Debug, Deserialize)]
pub struct Duration {
    #[serde(deserialize_with = "util::deserialize_opt_millis")]
    #[serde(default)]
    pub duration: Option<std::time::Duration>,
}
