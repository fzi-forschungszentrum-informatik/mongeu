//! Configuration related types and utilities
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroUsize;
use std::time::Duration;

use serde::Deserialize;

pub const DEFAULT_LISTEN_ADDRS: [ListenAddr; 2] = [
    ListenAddr::new(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)),
    ListenAddr::new(IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED)),
];
pub const DEFAULT_LISTEN_PORT: u16 = 80;

pub const DEFAULT_ONESHOT_DURATION: Duration = Duration::from_millis(500);

pub const DEFAULT_GC_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);
pub const DEFAULT_GC_MIN_CAMPAIGNS: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(1 << 16) };

/// One single address to listen on
#[derive(Copy, Clone, Debug, PartialEq, Deserialize)]
pub struct ListenAddr {
    /// IP addr to listen on
    ip: IpAddr,
    /// port to listen on (overriding the default port)
    port: Option<u16>,
}

impl ListenAddr {
    /// Create a new [ListenAddr] from an [IpAddr]
    const fn new(ip: IpAddr) -> Self {
        Self { ip, port: None }
    }

    /// Transform into a [SocketAddr], using the specified default for the port
    pub fn socket_addr(self, default_port: u16) -> SocketAddr {
        SocketAddr::new(self.ip, self.port.unwrap_or(default_port))
    }
}

impl std::str::FromStr for ListenAddr {
    type Err = <IpAddr as std::str::FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        IpAddr::from_str(s).map(Self::new)
    }
}
