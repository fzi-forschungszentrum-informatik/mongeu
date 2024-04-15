//! Utilities for making things a [Reply]
use nvml_wrapper::error::NvmlError;
use warp::http::header::{self, HeaderName, HeaderValue};
use warp::http::StatusCode;
use warp::reply::{self, Json, WithHeader};
use warp::Reply;

/// `Cache-control` `no-cache` directive
pub const NO_CACHE: HeaderValue = HeaderValue::from_static("no-cache");

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
        use log::Level;

        let (status, level) = match self {
            NvmlError::InvalidArg => (StatusCode::NOT_FOUND, Level::Trace),
            NvmlError::NotSupported => (StatusCode::NOT_FOUND, Level::Trace),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, Level::Warn),
        };

        log::log!(level, "Encountered error: {self:#}");
        warp::reply::with_status(self.to_string(), status)
    }
}

impl Replyify for anyhow::Error {
    type Reply = warp::reply::WithStatus<String>;

    fn replyify(self) -> Self::Reply {
        log::warn!("Encountered error: {self:#}");
        warp::reply::with_status(self.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
    }
}

/// Convenience trait for [Replyify]ing a [Result] in specific ways
pub trait ResultExt: Sized {
    /// Type encapsulated in [Result::Ok]
    type Value;

    /// Type encapsulated in [Result::Err]
    type Error;

    /// Replyify this by transforming [Result::Ok] into JSON
    fn json_reply(self) -> Result<Json, <Self::Error as Replyify>::Reply>
    where
        Self::Value: serde::Serialize,
        Self::Error: Replyify;

    /// Attach a `Cache-control` no-cache
    fn no_cache(self) -> Result<WithHeader<Self::Value>, Self::Error>
    where
        Self::Value: Reply,
    {
        self.cache_control(NO_CACHE)
    }

    /// Attach a `Cache-control` directive
    fn cache_control(
        self,
        directive: impl Into<HeaderValue>,
    ) -> Result<WithHeader<Self::Value>, Self::Error>
    where
        Self::Value: Reply,
    {
        self.with_header(header::CACHE_CONTROL, directive)
    }

    /// Attach a header
    fn with_header<V>(
        self,
        name: HeaderName,
        value: V,
    ) -> Result<WithHeader<Self::Value>, Self::Error>
    where
        Self::Value: Reply,
        V: Into<HeaderValue>;
}

impl<T, E> ResultExt for Result<T, E> {
    type Value = T;
    type Error = E;

    fn json_reply(self) -> Result<Json, <Self::Error as Replyify>::Reply>
    where
        Self::Value: serde::Serialize,
        Self::Error: Replyify,
    {
        self.map(|v| reply::json(&v)).replyify()
    }

    fn with_header<V>(
        self,
        name: HeaderName,
        value: V,
    ) -> Result<WithHeader<Self::Value>, Self::Error>
    where
        Self::Value: Reply,
        V: Into<HeaderValue>,
    {
        self.map(|r| reply::with_header(r, name, value))
    }
}
