// Copyright (c) 2024 FZI Forschungszentrum Informatik
// SPDX-License-Identifier: Apache-2.0
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

/// A health checker
#[derive(Clone, Debug)]
pub struct Checker<'a> {
    nvml: &'a nvml_wrapper::Nvml,
    oneshot_enabled: bool,
}

impl<'a> Checker<'a> {
    /// Create a new health checker
    pub fn new(nvml: &'a nvml_wrapper::Nvml, oneshot_enabled: bool) -> Result<Self> {
        Ok(Self {
            nvml,
            oneshot_enabled,
        })
    }

    /// Perform a health check, producing a [Health] info if healthy
    pub fn check(&self, campaigns: &BaseMeasurements) -> Result<Health> {
        let device_count = self
            .nvml
            .device_count()
            .context("Could not retrieve device count")?;
        let driver_version = self
            .nvml
            .sys_driver_version()
            .context("Could not retrieve driver version")?;
        let nvml_version = self
            .nvml
            .sys_nvml_version()
            .context("Could not retrieve NVML version")?;
        Ok(Health {
            device_count,
            version: env!("CARGO_PKG_VERSION"),
            driver_version,
            nvml_version,
            campaigns: campaigns.len(),
            oneshot_enabled: self.oneshot_enabled,
        })
    }
}
