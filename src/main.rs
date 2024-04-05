use std::net;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use nvml_wrapper as nvml;

use anyhow::Context;
use log::LevelFilter;
use nvml::error::NvmlError;
use nvml::Nvml;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync;
use warp::reply::json;
use warp::Filter;

mod config;
mod energy;
mod health;
mod replyify;
mod util;

use energy::BaseMeasurements;
use replyify::Replyify;

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
        base_uri,
    } = config;
    let base_uri = Arc::new(base_uri);

    init_logger(LevelFilter::Warn, matches.get_count("verbosity").into())
        .context("Could not initialize logger")?;
    let nvml = Arc::new(Nvml::init().context("Could not initialize NVML handle")?);

    let campaigns = Campaigns::default();
    let campaign_param = {
        let campaigns = campaigns.clone();
        warp::path::param().and_then(move |i| get_campaign(campaigns.clone(), i))
    };
    let campaigns_read = {
        let campaigns = campaigns.clone();
        warp::any().then(move || campaigns.clone().read_owned())
    };
    let campaigns_write = {
        let campaigns = campaigns.clone();
        warp::any().then(move || campaigns.clone().write_owned())
    };

    let gc_notify: Arc<tokio::sync::Notify> = Default::default();

    // End-point exposing the number of devices on this machine
    let device_count = warp::get()
        .and(warp::path("device_count"))
        .and(warp::path::end())
        .map({
            let nvml = nvml.clone();
            move || nvml.device_count().map(|v| json(&v)).replyify()
        });

    // End-point exposing the name of a specific device
    let device_name = warp::get()
        .and(warp::path::param::<u32>())
        .and(warp::path("name"))
        .and(warp::path::end())
        .and_then({
            let nvml = nvml.clone();
            move |i| with_device(nvml.as_ref(), i, |d| d.name())
        });

    // End-point exposing the UUID of a specific device
    let device_uuid = warp::get()
        .and(warp::path::param::<u32>())
        .and(warp::path("uuid"))
        .and(warp::path::end())
        .and_then({
            let nvml = nvml.clone();
            move |i| with_device(nvml.as_ref(), i, |d| d.uuid())
        });

    // End-point exposing the serial number of a specific device
    let device_serial = warp::get()
        .and(warp::path::param::<u32>())
        .and(warp::path("serial"))
        .and(warp::path::end())
        .and_then({
            let nvml = nvml.clone();
            move |i| with_device(nvml.as_ref(), i, |d| d.serial())
        });

    // End-point exposing the current power usage of a specific device
    let device_power_usage = warp::get()
        .and(warp::path::param::<u32>())
        .and(warp::path("power_usage"))
        .and(warp::path::end())
        .and_then({
            let nvml = nvml.clone();
            move |i| with_device(nvml.as_ref(), i, |d| d.power_usage())
        });

    let device = device_name
        .or(device_uuid)
        .or(device_serial)
        .or(device_power_usage);
    let device = warp::path("device").and(device);

    // End-point for performing a one-shot measurement of energy consumption
    let energy_oneshot = warp::get().and(warp::path::end()).and(warp::query()).then({
        let nvml = nvml.clone();
        let default_duration = oneshot.duration;
        move |d: DurationParam| {
            let duration = d.duration.unwrap_or(default_duration);
            energy_oneshot(nvml.clone(), duration)
        }
    });

    // End-point for creating a new measurement campaign
    let energy_create = warp::post()
        .and(campaigns_write.clone())
        .and(warp::path::end())
        .map({
            let nvml = nvml.clone();
            let gc_notify = gc_notify.clone();
            let base_uri = base_uri.clone();
            move |mut c: CampaignsWriteLock| {
                let id = c.create(nvml.as_ref()).map_err(Replyify::replyify)?;
                gc_notify.notify_one();

                format!("{base_uri}/v1/energy/{id}")
                    .try_into()
                    .context("Could not create URI for new measurement campaign {i}")
                    .map(|t: warp::http::Uri| warp::redirect::see_other(t))
                    .map_err(Replyify::replyify)
            }
        });

    // End-point for deleting/ending a measurement campaign
    let energy_delete = warp::delete()
        .and(campaigns_write.clone())
        .and(warp::path::param())
        .and(warp::path::end())
        .map(|mut c: CampaignsWriteLock, i| {
            use warp::http::StatusCode;

            if c.delete(i).is_some() {
                StatusCode::OK
            } else {
                StatusCode::NOT_FOUND
            }
        });

    // End-point for getting a (new) measurement in a campaign
    let energy_measure = warp::get()
        .and(campaign_param.clone())
        .and(warp::path::end())
        .map({
            let nvml = nvml.clone();
            move |b: CampaignReadLock| b.measurement(nvml.as_ref()).map(|v| json(&v)).replyify()
        });

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
        .and(campaigns_read.clone())
        .map({
            let nvml = nvml.clone();
            move |c: CampaignsReadLock| {
                health::check(nvml.as_ref(), &c)
                    .map(|v| json(&v))
                    .replyify()
            }
        });

    let v1_api = device_count.or(device).or(energy).or(ping).or(health);
    let v1_api = warp::path("v1").and(v1_api).with(warp::log("traffic"));

    let incoming = incoming_from(network.listen_addrs())
        .await
        .context("Could not start up server")?;
    let serve = warp::serve(v1_api).run_incoming(incoming);

    let gc = collect_garbage(gc_notify, campaigns, gc.min_age, gc.min_campaigns);

    tokio::join!(serve, gc);
    unreachable!()
}

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

/// Perform an operation with a device
fn with_device<T: serde::Serialize>(
    nvml: &nvml::Nvml,
    index: u32,
    func: impl Fn(nvml::Device) -> Result<T, NvmlError>,
) -> impl std::future::Future<Output = Result<impl warp::Reply, warp::Rejection>> {
    let res = match nvml.device_by_index(index) {
        Err(NvmlError::InvalidArg) => Err(warp::reject::not_found()),
        r => Ok(r.and_then(func).map(|v| json(&v)).replyify()),
    };
    std::future::ready(res)
}

/// Perform a "blocking" oneshot measurement over a given duration
async fn energy_oneshot(
    nvml: Arc<nvml::Nvml>,
    duration: Duration,
) -> Result<impl warp::Reply, impl warp::Reply> {
    let base = energy::BaseMeasurement::new(nvml.as_ref()).map_err(Replyify::replyify)?;

    tokio::time::sleep(duration).await;

    base.measurement(nvml.as_ref()).map(|v| json(&v)).replyify()
}

/// Helper type for representing a duration in `ms` in a paramater
#[derive(Copy, Clone, Debug, serde::Deserialize)]
struct DurationParam {
    #[serde(deserialize_with = "util::deserialize_opt_millis")]
    #[serde(default)]
    duration: Option<Duration>,
}

type Campaigns = Arc<sync::RwLock<BaseMeasurements>>;

type CampaignsReadLock = sync::OwnedRwLockReadGuard<BaseMeasurements>;

type CampaignsWriteLock = sync::OwnedRwLockWriteGuard<BaseMeasurements>;

type CampaignReadLock = sync::OwnedRwLockReadGuard<BaseMeasurements, energy::BaseMeasurement>;

/// Extract a single campaign under a [sync::OwnedRwLockReadGuard]
async fn get_campaign(
    campaigns: Campaigns,
    id: energy::BMId,
) -> Result<CampaignReadLock, warp::Rejection> {
    sync::OwnedRwLockReadGuard::try_map(campaigns.read_owned().await, |c| c.get(id))
        .map_err(|_| warp::reject::not_found())
}

/// Runs cyclic garbage collection after being notified
async fn collect_garbage(
    notifier: Arc<tokio::sync::Notify>,
    campaigns: Campaigns,
    min_age: Duration,
    min_campaigns: NonZeroUsize,
) {
    let tick_duration = std::cmp::max(min_age / 4, MIN_GC_TICK);

    let mut timer = tokio::time::interval(tick_duration);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let now = tokio::select! {
            t = timer.tick() => t.into(),
            _ = notifier.notified() => std::time::Instant::now(),
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
