mod aggregation;
mod analytics;
mod audit_handlers;
mod audit_routes;
mod benchmark_engine;
mod benchmark_handlers;
mod benchmark_routes;
mod checklist;
mod config_handlers;
mod config_routes;
mod contract_history_handlers;
mod contract_history_routes;
mod detector;
mod error;
mod handlers;
mod metrics;
mod observability;
mod rate_limit;
mod routes;
mod scoring;
mod state;
mod template_handlers;
mod template_routes;
mod scanner_service;
mod scan_handlers;
mod scan_routes;

use anyhow::Result;
use axum::http::{header, HeaderValue, Method};
use axum::{middleware, routing::get, Router};
use dotenv::dotenv;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;

use crate::rate_limit::RateLimitState;
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let otlp_endpoint = std::env::var("OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://jaeger:4317".to_string());
    observability::init(&otlp_endpoint);
    metrics::init_metrics();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("../../database/migrations").run(&pool).await?;
    tracing::info!("database connected and migrations applied");

    aggregation::spawn_aggregation_task(pool.clone());

    let state = AppState::new(pool);
    let rate_limit_state = RateLimitState::from_env();

    let cors = CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("http://localhost:3000"),
            HeaderValue::from_static("https://soroban-registry.vercel.app"),
        ])
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let app = Router::new()
        .merge(routes::contract_routes())
        .merge(routes::publisher_routes())
        .merge(routes::health_routes())
        .merge(routes::migration_routes())
        .merge(audit_routes::security_audit_routes())
        .merge(benchmark_routes::benchmark_routes())
        .merge(config_routes::config_routes())
        .merge(contract_history_routes::contract_history_routes())
        .merge(template_routes::template_routes())
        .merge(scan_routes::scan_routes())
        .route("/metrics", get(observability::metrics_handler))
        .fallback(handlers::route_not_found)
        .layer(middleware::from_fn(metrics_middleware))
        .layer(middleware::from_fn_with_state(
            rate_limit_state,
            rate_limit::rate_limit_middleware,
        ))
        .layer(CorsLayer::permissive())
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));
    tracing::info!(addr = %addr, "API server listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

async fn metrics_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> axum::response::Response {
    let method = req.method().to_string();
    let path = req
        .uri()
        .path()
        .to_string()
        .replace(|c: char| c.is_ascii_alphanumeric() || c == '/' || c == '-' || c == '_', |c: char| c)
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .to_string();
    let timer = std::time::Instant::now();

    let response = next.run(req).await;

    let status = response.status().as_u16().to_string();
    let elapsed = timer.elapsed().as_secs_f64();

    metrics::HTTP_REQUESTS_TOTAL
        .with_label_values(&[&method, &path, &status])
        .inc();
    metrics::HTTP_REQUEST_DURATION
        .with_label_values(&[&method, &path])
        .observe(elapsed);

    tracing::info!(method = %method, path = %path, status = %status, latency_ms = %(elapsed * 1000.0) as u64);

    response
}
