use std::sync::Arc;

use nvml_wrapper as nvml;

use nvml::Nvml;
use warp::Filter;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), nvml::error::NvmlError> {
    let matches = clap::command!().get_matches();

    let nvml = Nvml::init().map(Arc::new)?;

    // End-point exposing the number of devices on this machine
    let device_count = warp::get()
        .and(warp::path("device_count"))
        .and(warp::path::end())
        .map({
            let nvml = nvml.clone();
            move || nvml.device_count().replyify()
        });

    let v1_api = device_count;
    let v1_api = warp::any().and(warp::path("v1")).and(v1_api);

    warp::serve(v1_api).run(([0, 0, 0, 0], 8080)).await;
    unreachable!()
}

/// Convenience trait for transforming stuff into a [warp::Reply]
trait Replyify {
    /// Transform this value into a [warp::Reply]
    fn replyify(self) -> impl warp::Reply;
}

impl<T: serde::Serialize, E: ToString> Replyify for Result<T, E> {
    fn replyify(self) -> impl warp::Reply {
        use warp::http::StatusCode;

        self.as_ref()
            .map(warp::reply::json)
            .map_err(|e| warp::reply::with_status(e.to_string(), StatusCode::INTERNAL_SERVER_ERROR))
    }
}
