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
    let matches = clap::command!()
        .arg(
            clap::arg!(listen: -l --listen <ADDR> ... "Listen for connections on this address")
                .value_parser(clap::value_parser!(net::IpAddr)),
        )
        .arg(
            clap::arg!(port: -p --port <PORT> "Port to listen on")
                .value_parser(clap::value_parser!(u16)),
        )
        .arg(
            clap::arg!(base_uri: --"base-uri" <URI> "Base URI under which the API is hosted")
                .value_parser(clap::value_parser!(warp::http::Uri)),
        )
        .arg(
            clap::arg!(oneshot_duration: --"oneshot-duration" <MILLISECS> "Default duration for oneshot measurements")
                .value_parser(clap::value_parser!(u64)),
        )
        .arg(
            clap::arg!(gc_min_age: --"gc-min-age" <SECONDS> "Age at which a campaign might be collected")
                .value_parser(clap::value_parser!(u64)),
        )
        .arg(
            clap::arg!(gc_min_campaigns: --"gc-min-campaigns" <NUM> "Number of campaings at which collection will start")
                .value_parser(clap::value_parser!(NonZeroUsize)),
        )
        .arg(
            clap::arg!(verbosity: -v --verbose ... "Increase the verbosity level")
                .action(clap::ArgAction::Count),
        )
        .get_matches();

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

    let base_uri: Arc<warp::http::Uri> = matches.get_one("base_uri").cloned().unwrap_or_default();

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
    let oneshot_duration = matches
        .get_one("oneshot_duration")
        .cloned()
        .map(Duration::from_millis)
        .unwrap_or(config::DEFAULT_ONESHOT_DURATION);
    let energy_oneshot = warp::get().and(warp::path::end()).and(warp::query()).then({
        let nvml = nvml.clone();
        move |d: DurationParam| {
            let duration = d.as_duration().unwrap_or(oneshot_duration);
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

    let port = matches
        .get_one("port")
        .cloned()
        .unwrap_or(config::DEFAULT_LISTEN_PORT);
    let incoming = if let Some(addrs) = matches.get_many("listen") {
        incoming_from(&mut addrs.map(|p| net::SocketAddr::new(*p, port))).await
    } else {
        let mut addrs = config::DEFAULT_LISTEN_ADDRS
            .into_iter()
            .map(|p| p.socket_addr(port));
        incoming_from(&mut addrs).await
    }
    .context("Could not start up server")?;
    let serve = warp::serve(v1_api).run_incoming(incoming);

    let gc_min_age = matches
        .get_one("gc_min_age")
        .cloned()
        .map(Duration::from_secs)
        .unwrap_or(config::DEFAULT_GC_MIN_AGE);
    let gc_min_campaigns = matches
        .get_one("gc_min_campaigns")
        .cloned()
        .unwrap_or(config::DEFAULT_GC_MIN_CAMPAIGNS);
    let gc = collect_garbage(gc_notify, campaigns, gc_min_age, gc_min_campaigns);

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
    addrs: &mut dyn Iterator<Item = net::SocketAddr>,
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
    duration: Option<std::num::NonZeroU64>,
}

impl DurationParam {
    fn as_duration(self) -> Option<Duration> {
        self.duration
            .map(std::num::NonZeroU64::get)
            .map(Duration::from_millis)
    }
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
