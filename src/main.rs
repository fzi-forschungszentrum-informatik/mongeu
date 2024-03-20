#[tokio::main(flavor = "current_thread")]
async fn main() {
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
