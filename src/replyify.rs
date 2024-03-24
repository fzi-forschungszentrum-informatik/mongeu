//! Utilities for making things a [Reply]
use nvml_wrapper::error::NvmlError;
use warp::http::StatusCode;
use warp::Reply;

/// Convenience trait for transforming stuff into a [Reply]
pub trait Replyify {
    /// Transform this value into a [Reply]
    fn replyify(self) -> impl Reply;
}

impl<T: serde::Serialize, E: Replyify> Replyify for Result<T, E> {
    fn replyify(self) -> impl Reply {
        self.map(|v| warp::reply::json(&v))
            .map_err(Replyify::replyify)
    }
}

impl Replyify for NvmlError {
    fn replyify(self) -> impl Reply {
        let status = match self {
            NvmlError::InvalidArg => StatusCode::NOT_FOUND,
            NvmlError::NotSupported => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        warp::reply::with_status(self.to_string(), status)
    }
}
