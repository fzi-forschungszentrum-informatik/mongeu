//! Energy consumption measurement and associated utilities
use anyhow::{Context, Result};
use nvml_wrapper as nvml;

/// A measurement across multiple devices
#[derive(Debug, serde::Serialize)]
pub struct Measurement {
    /// Time passed since the start of a campaign in `ms`
    time: u128,
    /// Device data at the specific point in time
    devices: Vec<DeviceData>,
}

/// Data associated with a specific device
#[derive(Copy, Clone, Debug, serde::Serialize)]
pub struct DeviceData {
    /// Index of the device
    id: u32,
    /// Energy consumption of the device in `mJ`
    energy: u64,
}

impl DeviceData {
    /// Create a new total device data for the current time instant
    pub fn new_total(device: nvml::Device) -> Result<Self> {
        let id = device
            .index()
            .context("Could not determine index of device")?;
        let energy = total_energy_consumption(device, id)?;
        Ok(Self { id, energy })
    }

    /// Compute new relative device data from total device data
    pub fn relative(self, nvml: &nvml::Nvml) -> Result<Self> {
        device_by_index(nvml, self.id)
            .and_then(|d| total_energy_consumption(d, self.id))
            .map(|e| Self {
                energy: e.saturating_sub(self.energy),
                ..self
            })
    }
}

/// [nvml::Nvml::device_by_index] with [Context]
fn device_by_index(nvml: &nvml::Nvml, id: u32) -> Result<nvml::Device> {
    nvml.device_by_index(id)
        .with_context(|| format!("Could not retrieve device {id}"))
}

/// [nvml::Device::total_energy_consumption] with [Context]
fn total_energy_consumption(device: nvml::Device, id: u32) -> Result<u64> {
    device
        .total_energy_consumption()
        .with_context(|| format!("Could not retrieve total energy consumption of device {id}"))
}
