// Copyright (c) 2024 FZI Forschungszentrum Informatik
// SPDX-License-Identifier: Apache-2.0
//! Health-check utilities

use anyhow::{Context, Result};

use crate::energy::BaseMeasurements;

/// Health check data
#[derive(Debug, serde::Serialize)]
pub struct Health {
    device_count: u32,
    device_names: Vec<String>,
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
        let device_names = (0..device_count)
            .map(|i| {
                self.nvml
                    .device_by_index(i)
                    .with_context(|| format!("Could not retrieve device {i}"))?
                    .name()
                    .with_context(|| format!("Could not retrieve name of device {i}"))
            })
            .collect::<Result<_>>()?;
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
            device_names,
            version: env!("CARGO_PKG_VERSION"),
            driver_version,
            nvml_version,
            campaigns: campaigns.len(),
            oneshot_enabled: self.oneshot_enabled,
        })
    }
}
