//! Utilities for making things a [Reply]
use nvml_wrapper::error::NvmlError;
use warp::http::StatusCode;
use warp::Reply;

/// Convenience trait for transforming stuff into a [Reply]
pub trait Replyify {
    /// Type of the reply [Self::replyify] is transforming [Self] into
    type Reply: Reply;

    /// Transform this value into a [Reply]
    fn replyify(self) -> Self::Reply;
}

impl<T: Reply, E: Replyify> Replyify for Result<T, E> {
    type Reply = Result<T, E::Reply>;

    fn replyify(self) -> Self::Reply {
        self.map_err(Replyify::replyify)
    }
}

impl Replyify for NvmlError {
    type Reply = warp::reply::WithStatus<String>;

    fn replyify(self) -> Self::Reply {
        let status = match self {
            NvmlError::InvalidArg => StatusCode::NOT_FOUND,
            NvmlError::NotSupported => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        warp::reply::with_status(self.to_string(), status)
    }
}

impl Replyify for anyhow::Error {
    type Reply = warp::reply::WithStatus<String>;

    fn replyify(self) -> Self::Reply {
        warp::reply::with_status(self.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
    }
}
