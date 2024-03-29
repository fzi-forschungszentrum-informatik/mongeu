//! Configuration related types and utilities
use std::net::IpAddr;
use std::num::NonZeroUsize;
use std::time::Duration;

pub const DEFAULT_LISTEN_ADDRS: [IpAddr; 2] = [
    IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
    IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
];
pub const DEFAULT_LISTEN_PORT: u16 = 80;

pub const DEFAULT_ONESHOT_DURATION: Duration = Duration::from_millis(500);

pub const DEFAULT_GC_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);
pub const DEFAULT_GC_MIN_CAMPAIGNS: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(1 << 16) };
