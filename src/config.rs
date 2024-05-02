//! Configuration related types and utilities
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroUsize;
use std::time::Duration;

use clap::Args;
use serde::Deserialize;
use warp::http::Uri;

use crate::util;

const DEFAULT_LISTEN_ADDRS: [ListenAddr; 2] = [
    ListenAddr::new(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)),
    ListenAddr::new(IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED)),
];
const DEFAULT_LISTEN_PORT: u16 = 80;

const DEFAULT_ONESHOT_ENABLE: bool = false;
const DEFAULT_ONESHOT_DURATION: Duration = Duration::from_millis(500);

const DEFAULT_GC_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const DEFAULT_GC_MIN_CAMPAIGNS: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(1 << 16) };

const DEFAULT_CACHE_MAX_AGE: Duration = Duration::from_secs(15 * 60);

/// General configuration
#[derive(Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub network: Network,
    pub oneshot: Oneshot,
    pub gc: GC,
    pub misc: Misc,
}

impl Config {
    /// Retrieve a [Config] from a TOML file
    pub fn from_toml_file(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        use anyhow::Context;

        let toml = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("Could not read file {}", path.as_ref().display()))?;
        Self::from_toml(toml)
    }

    /// Retrieve a [Config] from a TOML, provided as [str]
    pub fn from_toml(toml: impl AsRef<str>) -> anyhow::Result<Self> {
        use anyhow::Context;

        toml::from_str(toml.as_ref()).context("Could not parse TOML")
    }
}

impl Args for Config {
    fn augment_args(cmd: clap::Command) -> clap::Command {
        Self::augment_args_for_update(cmd)
    }

    fn augment_args_for_update(cmd: clap::Command) -> clap::Command {
        let cmd = Network::augment_args_for_update(cmd);
        let cmd = Oneshot::augment_args_for_update(cmd);
        let cmd = GC::augment_args_for_update(cmd);
        let cmd = Misc::augment_args_for_update(cmd);
        cmd
    }
}

impl clap::FromArgMatches for Config {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::error::Error> {
        let mut res = Self::default();
        res.update_from_arg_matches(matches)?;
        Ok(res)
    }

    fn update_from_arg_matches(
        &mut self,
        matches: &clap::ArgMatches,
    ) -> Result<(), clap::error::Error> {
        self.network.update_from_arg_matches(matches)?;
        self.oneshot.update_from_arg_matches(matches)?;
        self.gc.update_from_arg_matches(matches)?;
        self.misc.update_from_arg_matches(matches)?;
        Ok(())
    }
}

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

/// Oneshot measurement configuration
#[derive(Copy, Clone, Args, Deserialize)]
#[serde(default)]
pub struct Oneshot {
    /// Enable potentially blocking oneshot end-points
    #[arg(long = "enable-oneshot")]
    pub enable: bool,

    /// Default duration for oneshot measurements
    #[arg(long = "oneshot-duration", value_name("MILLIS"), value_parser = util::parse_millis)]
    #[serde(deserialize_with = "util::deserialize_millis")]
    pub duration: Duration,
}

impl Default for Oneshot {
    fn default() -> Self {
        Self {
            enable: DEFAULT_ONESHOT_ENABLE,
            duration: DEFAULT_ONESHOT_DURATION,
        }
    }
}

/// Garbage collection configuration
#[derive(Copy, Clone, Args, Deserialize)]
#[serde(default)]
pub struct GC {
    /// Age at which a campaign might be collected
    #[arg(long = "gc-min-age", value_name("SECONDS"), value_parser = util::parse_secs)]
    #[serde(deserialize_with = "util::deserialize_secs")]
    pub min_age: Duration,

    /// Number of campaings at which collection will start
    #[arg(long = "gc-min-campaigns", value_name("NUM"))]
    pub min_campaigns: NonZeroUsize,
}

impl Default for GC {
    fn default() -> Self {
        Self {
            min_age: DEFAULT_GC_MIN_AGE,
            min_campaigns: DEFAULT_GC_MIN_CAMPAIGNS,
        }
    }
}

/// Miscelleneous configuration
#[derive(Clone, Args, Deserialize)]
#[serde(default)]
pub struct Misc {
    /// Base URI under which the API is hosted
    #[arg(long = "base-uri", value_name("URI"), value_parser = util::parse_base_uri)]
    #[serde(deserialize_with = "util::deserialize_base_uri")]
    pub base_uri: Uri,

    /// Max-age to communicate for non-ephemeral values in Cache-control
    #[arg(long = "cache-max-age", value_name("SECONDS"), value_parser = util::parse_secs)]
    #[serde(deserialize_with = "util::deserialize_secs")]
    pub cache_max_age: Duration,
}

impl Default for Misc {
    fn default() -> Self {
        Self {
            base_uri: Default::default(),
            cache_max_age: DEFAULT_CACHE_MAX_AGE,
        }
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

    #[cfg(unix)]
    #[test]
    fn example_config() {
        let Config {
            network,
            oneshot,
            gc,
            misc,
        } = toml::from_str(include_str!("../example_config.toml")).expect("Could not parse TOML");

        assert_eq!(network.port, 80);
        let addrs = [SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8080)];
        assert_eq!(network.listen_addrs().collect::<Vec<_>>(), addrs);

        assert_eq!(oneshot.duration, Duration::from_millis(200));

        assert_eq!(gc.min_age, Duration::from_secs(12 * 60 * 60));
        assert_eq!(gc.min_campaigns.get(), 100);

        assert_eq!(misc.base_uri, "/gms/");
    }

    #[test]
    fn sane_default_uri() {
        let misc: Misc = Default::default();
        assert_eq!(misc.base_uri.query(), None);
        assert_eq!(misc.base_uri.path(), "/");
    }
}
