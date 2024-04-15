use std::net;
use std::num::NonZeroUsize;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use nvml_wrapper as nvml;

use anyhow::Context;
use log::LevelFilter;
use nvml::error::NvmlError;
use nvml::Nvml;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync;
use warp::Filter;

mod config;
mod energy;
mod health;
mod param;
mod replyify;
mod util;

use energy::BaseMeasurements;
use replyify::{Replyify, ResultExt};

const MIN_GC_TICK: Duration = Duration::from_secs(60);

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    use clap::{Args, FromArgMatches};

    let matches = config::Config::augment_args_for_update(clap::command!())
        .arg(
            clap::arg!(config: -c --config <FILE> "Read configuration from a TOML file")
                .value_parser(clap::value_parser!(std::path::PathBuf)),
        )
        .arg(
            clap::arg!(verbosity: -v --verbose ... "Increase the verbosity level")
                .action(clap::ArgAction::Count),
        )
        .get_matches();

    let mut config = matches
        .get_one::<std::path::PathBuf>("config")
        .map(config::Config::from_toml_file)
        .transpose()
        .context("Could not read config file")?
        .unwrap_or_default();
    config
        .update_from_arg_matches(&matches)
        .context("Could not extract configuration from CLI")?;
    let config::Config {
        network,
        oneshot,
        gc,
        misc,
    } = config;

    let base_uri = Arc::new(misc.base_uri);
    let max_age: warp::http::header::HeaderValue =
        format!("max-age={}", misc.cache_max_age.as_secs())
            .try_into()
            .context("Could not prepare a max-age directive")?;

    init_logger(LevelFilter::Warn, matches.get_count("verbosity").into())
        .context("Could not initialize logger")?;

    let nvml = Nvml::init().context("Could not initialize NVML handle")?;
    let nvml = NVML.get_or_init(move || nvml);
    let device = warp::path::param().and_then(|i| {
        let res = match nvml.device_by_index(i) {
            Ok(d) => Ok(d),
            Err(NvmlError::InvalidArg) => Err(warp::reject::not_found()),
            Err(e) => {
                log::warn!("Could not retrieve device {i}: {e}");
                Err(warp::reject::custom(util::DeviceRetrievalError(i)))
            }
        };
        std::future::ready(res)
    });

    let campaigns = CAMPAIGNS.get_or_init(Default::default);
    let campaign_param = warp::path::param().and_then(|i| get_campaign(campaigns, i));
    let campaigns_read = warp::any().then(|| campaigns.read());
    let campaigns_write = warp::any().then(|| campaigns.write());

    let oneshot_enabled = {
        let enabled = oneshot.enable;
        move || std::future::ready(enabled.then_some(()).ok_or_else(warp::reject::not_found))
    };

    // End-point exposing the number of devices on this machine
    let device_count = warp::get()
        .and(warp::path("device_count"))
        .and(warp::path::end())
        .map(|| nvml.device_count().json_reply());

    // End-points exposing various device info
    let device_info = warp::get()
        .and(device)
        .and(warp::path::param())
        .and(warp::path::end())
        .map(|d: nvml::Device, p: param::DeviceProperty| {
            use param::DeviceProperty as DP;
            match p {
                DP::Name => d.name().json_reply(),
                DP::Uuid => d.uuid().json_reply(),
                DP::Serial => d.serial().json_reply(),
                DP::PowerUsage => d.power_usage().json_reply(),
            }
        });

    let device = warp::path("device").and(device_info);

    // End-point for performing a one-shot measurement of energy consumption
    let energy_oneshot = warp::get()
        .and_then(oneshot_enabled)
        .untuple_one()
        .and(warp::path::end())
        .and(warp::query())
        .then({
            let default_duration = oneshot.duration;
            move |d: param::Duration| {
                let duration = d.duration.unwrap_or(default_duration);
                energy_oneshot(nvml, duration)
            }
        });

    // End-point for creating a new measurement campaign
    let energy_create = warp::post()
        .and(warp::path::end())
        .and(campaigns_write)
        .map({
            let base_uri = base_uri.clone();
            move |mut c: CampaignsWriteLock| {
                let id = c.create(nvml).map_err(Replyify::replyify)?;
                GC_NOTIFIER.notify_one();

                format!("{base_uri}v1/energy/{id}")
                    .try_into()
                    .context("Could not create URI for new measurement campaign {i}")
                    .map(|t: warp::http::Uri| warp::redirect::see_other(t))
                    .replyify()
            }
        });

    // End-point for deleting/ending a measurement campaign
    let energy_delete = warp::delete()
        .and(warp::path::param())
        .and(warp::path::end())
        .and(campaigns_write)
        .map(|i, mut c: CampaignsWriteLock| {
            use warp::http::StatusCode;

            if c.delete(i).is_some() {
                StatusCode::OK
            } else {
                StatusCode::NOT_FOUND
            }
        });

    // End-point for getting a (new) measurement in a campaign
    let energy_measure = warp::get()
        .and(campaign_param)
        .and(warp::path::end())
        .map(|b: CampaignReadLock| b.measurement().json_reply());

    let energy = energy_oneshot
        .or(energy_create)
        .or(energy_delete)
        .or(energy_measure);
    let energy = warp::path("energy").and(energy);

    // Ping end-point
    let ping = warp::get()
        .and(warp::path("ping"))
        .and(warp::path::end())
        .map(|| warp::http::StatusCode::OK);

    // End-point for performing a healtch check
    let health = warp::get()
        .and(warp::path("health"))
        .and(warp::path::end())
        .and(campaigns_read)
        .map(|c: CampaignsReadLock| health::check(nvml, &c).json_reply());

    let v1_api = device_count.or(device).or(energy).or(ping).or(health);
    let v1_api = warp::path("v1").and(v1_api).with(warp::log("traffic"));

    let incoming = incoming_from(network.listen_addrs())
        .await
        .context("Could not start up server")?;
    let serve = warp::serve(v1_api).run_incoming(incoming);

    let gc = collect_garbage(campaigns, gc.min_age, gc.min_campaigns);

    tokio::join!(serve, gc);
    unreachable!()
}

/// NVML instance
static NVML: OnceLock<nvml::Nvml> = OnceLock::new();

/// Initialize a global logger
fn init_logger(level: LevelFilter, modifier: usize) -> Result<(), impl std::error::Error> {
    let logger = simple_logger::SimpleLogger::new()
        .with_utc_timestamps()
        .with_level(level)
        .env();

    // Sadly, there is no easy way to just increment a [Level] or [LevelFilter]
    let num_level = logger.max_level() as usize + modifier;
    let level = LevelFilter::iter()
        .find(|l| *l as usize == num_level)
        .unwrap_or(log::STATIC_MAX_LEVEL);
    logger.with_level(level).init()
}

/// Create a stream of incoming TCP connections from a addresses to bind to
async fn incoming_from(
    addrs: impl IntoIterator<Item = net::SocketAddr>,
) -> anyhow::Result<impl futures_util::TryStream<Ok = TcpStream, Error = std::io::Error>> {
    use futures_util::stream::{self, StreamExt};

    let mut incoming = stream::SelectAll::new();
    for addr in addrs {
        log::trace!("Binding to address {addr}");
        let listener = TcpListener::bind(addr)
            .await
            .context("Could not bind to address '{addr}'")?;
        let listener = Arc::new(listener);
        let tcp_streams = stream::repeat(()).then(move |_| do_accept(listener.clone()));
        incoming.push(Box::pin(tcp_streams));
    }
    Ok(incoming)
}

/// Accept a connection from a given listener
async fn do_accept(listener: Arc<TcpListener>) -> Result<TcpStream, std::io::Error> {
    listener.accept().await.map(|(s, _)| s)
}

/// Perform a "blocking" oneshot measurement over a given duration
async fn energy_oneshot(
    nvml: &'static nvml::Nvml,
    duration: Duration,
) -> Result<impl warp::Reply, impl warp::Reply> {
    let base = energy::BaseMeasurement::new(nvml).map_err(Replyify::replyify)?;

    tokio::time::sleep(duration).await;

    base.measurement().json_reply()
}

type Campaigns = sync::RwLock<BaseMeasurements>;

type CampaignsReadLock = sync::RwLockReadGuard<'static, BaseMeasurements>;

type CampaignsWriteLock = sync::RwLockWriteGuard<'static, BaseMeasurements>;

type CampaignReadLock = sync::RwLockReadGuard<'static, energy::BaseMeasurement>;

/// Extract a single campaign under a [sync::RwLockReadGuard]
async fn get_campaign(
    campaigns: &'static Campaigns,
    id: energy::BMId,
) -> Result<CampaignReadLock, warp::Rejection> {
    sync::RwLockReadGuard::try_map(campaigns.read().await, |c| c.get(id))
        .map_err(|_| warp::reject::not_found())
}

static CAMPAIGNS: OnceLock<Campaigns> = OnceLock::new();

/// Runs cyclic garbage collection after being notified
async fn collect_garbage(campaigns: &Campaigns, min_age: Duration, min_campaigns: NonZeroUsize) {
    let tick_duration = std::cmp::max(min_age / 4, MIN_GC_TICK);

    let mut timer = tokio::time::interval(tick_duration);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let now = tokio::select! {
            t = timer.tick() => t.into(),
            _ = GC_NOTIFIER.notified() => std::time::Instant::now(),
        };

        log::trace!("Triggering garbage collection");

        // We definitely only want to hold this lock for a short time.
        let mut campaigns = campaigns.write().await;

        let count = campaigns.len();
        log::trace!("Number of active campaigns is {count}");

        // It's not woth doing anything until we reach a certain number of
        // campaigns.
        if count >= min_campaigns.get() {
            log::info!("Performing garbage collection");
            campaigns.delete_older_than(now - min_age);
        }
    }
}

static GC_NOTIFIER: sync::Notify = sync::Notify::const_new();
