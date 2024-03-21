use std::sync::Arc;

use nvml_wrapper as nvml;

use nvml::error::NvmlError;
use nvml::Nvml;
use warp::Filter;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), nvml::error::NvmlError> {
    use std::net;

    let matches = clap::command!()
        .arg(
            clap::arg!(listen: -l --listen <ADDR> "Address to listen on for connections")
                .value_parser(clap::value_parser!(net::IpAddr)),
        )
        .arg(
            clap::arg!(port: -p --port <PORT> "Port to listen on")
                .value_parser(clap::value_parser!(u16)),
        )
        .get_matches();

    let nvml = Nvml::init().map(Arc::new)?;

    // End-point exposing the number of devices on this machine
    let device_count = warp::get()
        .and(warp::path("device_count"))
        .and(warp::path::end())
        .map({
            let nvml = nvml.clone();
            move || nvml.device_count().replyify()
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

    let device = device_name.or(device_uuid).or(device_serial).or(device_power_usage);
    let device = warp::any().and(warp::path("device")).and(device);

    let v1_api = device_count.or(device);
    let v1_api = warp::any().and(warp::path("v1")).and(v1_api);

    let addr = matches
        .get_one("listen")
        .cloned()
        .unwrap_or(net::Ipv6Addr::UNSPECIFIED.into());
    let port = matches.get_one("port").cloned().unwrap_or(80);
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
        r => Ok(r.and_then(func).replyify()),
    };
    std::future::ready(res)
}

/// Convenience trait for transforming stuff into a [warp::Reply]
trait Replyify {
    /// Transform this value into a [warp::Reply]
    fn replyify(self) -> impl warp::Reply;
}

impl<T: serde::Serialize, E: Replyify> Replyify for Result<T, E> {
    fn replyify(self) -> impl warp::Reply {
        self.map(|v| warp::reply::json(&v))
            .map_err(Replyify::replyify)
    }
}

impl Replyify for NvmlError {
    fn replyify(self) -> impl warp::Reply {
        use warp::http::StatusCode;
        let status = match self {
            NvmlError::InvalidArg => StatusCode::NOT_FOUND,
            NvmlError::NotSupported => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        warp::reply::with_status(self.to_string(), status)
    }
}
