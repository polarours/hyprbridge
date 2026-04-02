use axum::{Router, response::IntoResponse, routing::get};
use hyprbridge_relay_server::{RelayState, handle_ws};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::info;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let state = Arc::new(RelayState::new());
    let app = Router::new()
        .route("/health", get(health))
        .route("/ws", get(handle_ws))
        .with_state(state)
        .layer(CorsLayer::permissive());
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    info!("Relay server listening on 0.0.0.0:8080");
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> impl IntoResponse { "ok" }