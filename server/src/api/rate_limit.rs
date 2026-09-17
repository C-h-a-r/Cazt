use std::net::SocketAddr;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use axum::extract::ConnectInfo;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use dashmap::DashMap;

use crate::error::{AppError, ErrorCode};

static BUCKETS: LazyLock<DashMap<String, Window>> = LazyLock::new(DashMap::new);

struct Window {
    count: u32,
    started: Instant,
}

const LIMIT: u32 = 120;
const WINDOW: Duration = Duration::from_secs(60);

pub async fn limit(request: Request<axum::body::Body>, next: Next) -> Result<Response, AppError> {
    let key = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip().to_string())
        .unwrap_or_else(|| "unknown".into());

    let limited = {
        let mut entry = BUCKETS.entry(key).or_insert_with(|| Window {
            count: 0,
            started: Instant::now(),
        });
        if entry.started.elapsed() > WINDOW {
            entry.count = 0;
            entry.started = Instant::now();
        }
        entry.count += 1;
        entry.count > LIMIT
    };

    if limited {
        return Err(AppError::new(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::RateLimited,
            "Slow down a little — too many requests from this address.",
        ));
    }

    Ok(next.run(request).await)
}
