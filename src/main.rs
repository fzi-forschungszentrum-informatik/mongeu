use std::net;
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
            clap::arg!(oneshot_duration: --"oneshot-duration" <MILLISECS> "Default duration for oneshot measurements")
                .value_parser(clap::value_parser!(u16)),
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
    let campaigns_write = warp::any().then(move || campaigns.clone().write_owned());

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
            move |mut c: CampaignsWriteLock| {
                c.create(nvml.as_ref())
                    .and_then(|i| {
                        format!("/v1/energy/{i}")
                            .try_into()
                            .context("Could not create URI for new measurement campaign {i}")
                    })
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
                health::check(nvml.as_ref(), &*c)
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
    warp::serve(v1_api)
        .run(net::SocketAddr::new(addr, port))
        .await;
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
