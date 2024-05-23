//! Health-check utilities

use anyhow::{Context, Result};

use crate::energy::BaseMeasurements;

/// Health check data
#[derive(Debug, serde::Serialize)]
pub struct Health {
    device_count: u32,
    version: &'static str,
    driver_version: String,
    nvml_version: String,
    campaigns: usize,
    oneshot_enabled: bool,
}

/// Perform a health check
pub fn check(
    nvml: &nvml_wrapper::Nvml,
    campaigns: &BaseMeasurements,
    oneshot_enabled: bool,
) -> Result<Health> {
    let device_count = nvml
        .device_count()
        .context("Could not retrieve device count")?;
    let driver_version = nvml
        .sys_driver_version()
        .context("Could not retrieve driver version")?;
    let nvml_version = nvml
        .sys_nvml_version()
        .context("Could not retrieve NVML version")?;
    let version = env!("CARGO_PKG_VERSION");
    let campaigns = campaigns.len();
    Ok(Health {
        device_count,
        version,
        driver_version,
        nvml_version,
        campaigns,
        oneshot_enabled,
    })
}
