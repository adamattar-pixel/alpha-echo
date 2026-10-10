use std::time::Duration;

use axum::{Router, http::HeaderValue, routing::get};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{AppState, briefings, intelligence, market_data, portfolio};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .merge(portfolio::router())
        .merge(briefings::router())
        .merge(intelligence::router())
        .merge(market_data::router())
        .layer(
            CorsLayer::new()
                .allow_origin([
                    HeaderValue::from_static("http://127.0.0.1:3000"),
                    HeaderValue::from_static("http://localhost:3000"),
                ])
                .allow_headers(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .max_age(Duration::from_secs(3600)),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}
