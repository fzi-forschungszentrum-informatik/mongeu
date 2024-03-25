use std::net;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use nvml_wrapper as nvml;

use anyhow::Context;
use nvml::error::NvmlError;
use nvml::Nvml;
use tokio::sync;
use warp::reply::json;
use warp::Filter;

mod energy;
mod health;
mod replyify;

use energy::BaseMeasurements;
use replyify::Replyify;

const DEFAULT_LISTEN_ADDR: net::IpAddr = net::IpAddr::V6(net::Ipv6Addr::UNSPECIFIED);
const DEFAULT_LISTEN_PORT: u16 = 80;

const DEFAULT_ONESHOT_DURATION: Duration = Duration::from_millis(500);

const DEFAULT_GC_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const DEFAULT_GC_MIN_CAMPAIGNS: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(1 << 16) };
const MIN_GC_TICK: Duration = Duration::from_secs(60);

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let matches = clap::command!()
        .arg(
            clap::arg!(listen: -l --listen <ADDR> "Address to listen on for connections")
                .value_parser(clap::value_parser!(net::IpAddr)),
        )
        .arg(
            clap::arg!(port: -p --port <PORT> "Port to listen on")
                .value_parser(clap::value_parser!(u16)),
        )
        .arg(
            clap::arg!(base_uri: --"base-uri" <URI> "Base URI under which the API is hosted")
                .value_parser(clap::value_parser!(u16)),
        )
        .arg(
            clap::arg!(oneshot_duration: --"oneshot-duration" <MILLISECS> "Default duration for oneshot measurements")
                .value_parser(clap::value_parser!(u16)),
        )
        .arg(
            clap::arg!(gc_min_age: --"gc-min-age" <SECONDS> "Age at which a campaign might be collected")
                .value_parser(clap::value_parser!(u64)),
        )
        .arg(
            clap::arg!(gc_min_campaigns: --"gc-min-campaigns" <NUM> "Number of campaings at which collection will start")
                .value_parser(clap::value_parser!(NonZeroUsize)),
        )
        .get_matches();

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

    let base_uri: Arc<str> = matches.get_one("base_uri").cloned().unwrap_or("".into());

    // End-point exposing the number of devices on this machine
    let device_count = warp::get()
        .and(warp::path("device_count"))
        .and(warp::path::end())
        .map({
            let nvml = nvml.clone();
            move || nvml.device_count().map(|v| json(&v)).replyify()
        });

    let device_name = warp::get()
        .and(warp::path::param::<u32>())
        .and(warp::path("name"))
        .and(warp::path::end())
        .and_then({
            let nvml = nvml.clone();
            move |i| with_device(nvml.as_ref(), i, |d| d.name())
        });

    let device_uuid = warp::get()
        .and(warp::path::param::<u32>())
        .and(warp::path("uuid"))
        .and(warp::path::end())
        .and_then({
            let nvml = nvml.clone();
            move |i| with_device(nvml.as_ref(), i, |d| d.uuid())
        });

    let device_serial = warp::get()
        .and(warp::path::param::<u32>())
        .and(warp::path("serial"))
        .and(warp::path::end())
        .and_then({
            let nvml = nvml.clone();
            move |i| with_device(nvml.as_ref(), i, |d| d.serial())
        });

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

    let oneshot_duration = matches
        .get_one("oneshot_duration")
        .cloned()
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_ONESHOT_DURATION);
    let energy_oneshot = warp::get().and(warp::path::end()).and(warp::query()).then({
        let nvml = nvml.clone();
        move |d: DurationParam| {
            let duration = d.as_duration().unwrap_or(oneshot_duration);
            energy_oneshot(nvml.clone(), duration)
        }
    });

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

    let ping = warp::get()
        .and(warp::path("ping"))
        .and(warp::path::end())
        .map(|| warp::http::StatusCode::OK);

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
    let v1_api = warp::path("v1").and(v1_api);

    let addr = matches
        .get_one("listen")
        .cloned()
        .unwrap_or(DEFAULT_LISTEN_ADDR);
    let port = matches
        .get_one("port")
        .cloned()
        .unwrap_or(DEFAULT_LISTEN_PORT);
    let serve = warp::serve(v1_api).run(net::SocketAddr::new(addr, port));

    let gc_min_age = matches
        .get_one("gc_min_age")
        .cloned()
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_GC_MIN_AGE);
    let gc_min_campaigns = matches
        .get_one("gc_min_campaigns")
        .cloned()
        .unwrap_or(DEFAULT_GC_MIN_CAMPAIGNS);
    let gc = collect_garbage(gc_notify, campaigns, gc_min_age, gc_min_campaigns);

    tokio::join!(serve, gc);
    unreachable!()
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

        // We definitely only want to hold this lock for a short time.
        let mut campaigns = campaigns.write().await;

        // It's not woth doing anything until we reach a certain number of
        // campaigns.
        if campaigns.len() >= min_campaigns.get() {
            campaigns.delete_older_than(now - min_age);
        }
    }
}
