//! Health-check utilities

use anyhow::{Context, Result};

use crate::energy::BaseMeasurements;

/// Health check data
#[derive(Debug, serde::Serialize)]
pub struct Health {
    device_count: u32,
    driver_version: String,
    nvml_version: String,
    campaigns: usize,
}

/// Perform a health check
pub fn check(nvml: &nvml_wrapper::Nvml, campaigns: &BaseMeasurements) -> Result<Health> {
    let device_count = nvml
        .device_count()
        .context("Could not retrieve device count")?;
    let driver_version = nvml
        .sys_driver_version()
        .context("Could not retrieve driver version")?;
    let nvml_version = nvml
        .sys_nvml_version()
        .context("Could not retrieve NVML version")?;
    let campaigns = campaigns.len();
    Ok(Health {
        device_count,
        driver_version,
        nvml_version,
        campaigns,
    })
}
