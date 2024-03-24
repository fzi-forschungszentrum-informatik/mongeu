//! Energy consumption measurement and associated utilities

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
