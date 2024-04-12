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

/// Helper type for handling names of device properties
#[derive(Copy, Clone, Debug)]
pub enum DeviceProperty {
    Name,
    Uuid,
    Serial,
    PowerUsage,
}

impl std::str::FromStr for DeviceProperty {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "name" => Ok(Self::Name),
            "uuid" => Ok(Self::Uuid),
            "serial" => Ok(Self::Serial),
            "power_usage" => Ok(Self::PowerUsage),
            e => Err(e.into()),
        }
    }
}
