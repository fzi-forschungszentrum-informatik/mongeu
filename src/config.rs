//! Configuration related types and utilities
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroUsize;
use std::time::Duration;

use clap::Args;
use serde::Deserialize;

pub const DEFAULT_LISTEN_ADDRS: [ListenAddr; 2] = [
    ListenAddr::new(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)),
    ListenAddr::new(IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED)),
];
pub const DEFAULT_LISTEN_PORT: u16 = 80;

pub const DEFAULT_ONESHOT_DURATION: Duration = Duration::from_millis(500);

pub const DEFAULT_GC_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);
pub const DEFAULT_GC_MIN_CAMPAIGNS: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(1 << 16) };

/// Network configuration
#[derive(Clone, Args, Deserialize)]
#[serde(default)]
pub struct Network {
    /// Addresses to listen on for connections
    #[arg(short, long, value_name("IP"))]
    listen: Vec<ListenAddr>,

    /// Port to default to when not declared for a `listen` entry
    #[arg(short, long, help = "Port to listen on")]
    port: u16,
}

impl Network {
    /// Retriefe the addresses to listen on for connections
    pub fn listen_addrs(&self) -> impl Iterator<Item = SocketAddr> + '_ {
        self.listen.iter().map(move |l| l.socket_addr(self.port))
    }
}

impl Default for Network {
    fn default() -> Self {
        Self {
            listen: DEFAULT_LISTEN_ADDRS.into(),
            port: DEFAULT_LISTEN_PORT,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn network_toml_smoke() {
        let network: Network = toml::from_str(concat!(
            "port = 8080\n",
            "[[listen]]\n",
            "ip = \"1.2.3.4\"\n",
            "port = 80\n",
            "[[listen]]\n",
            "ip = \"5.6.7.8\"\n",
        ))
        .expect("Could not parse TOML");
        assert_eq!(network.port, 8080);
        let addrs = [
            SocketAddr::new(Ipv4Addr::new(1, 2, 3, 4).into(), 80),
            SocketAddr::new(Ipv4Addr::new(5, 6, 7, 8).into(), 8080),
        ];
        assert_eq!(network.listen_addrs().collect::<Vec<_>>(), addrs);
    }

    #[test]
    fn network_toml_empty() {
        let network: Network = toml::from_str("").expect("Could not parse TOML");
        assert_eq!(network.listen, DEFAULT_LISTEN_ADDRS);
        assert_eq!(network.port, DEFAULT_LISTEN_PORT);
        let addrs = [
            SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), DEFAULT_LISTEN_PORT),
            SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), DEFAULT_LISTEN_PORT),
        ];
        assert_eq!(network.listen_addrs().collect::<Vec<_>>(), addrs);
    }
}
