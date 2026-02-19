use axum::{
    routing::{get, post, put},
    Router,
};

use crate::{handlers, state::AppState};

/// Contract-related routes
pub fn contract_routes() -> Router<AppState> {
    Router::new()
        .route("/api/contracts", get(handlers::list_contracts))
        .route("/api/contracts", post(handlers::publish_contract))
        .route("/api/contracts/:id", get(handlers::get_contract))
        .route("/api/contracts/:id/versions", get(handlers::get_contract_versions))
        .route("/api/contracts/verify", post(handlers::verify_contract))
}

/// Publisher-related routes
pub fn publisher_routes() -> Router<AppState> {
    Router::new()
        .route("/api/publishers", post(handlers::create_publisher))
        .route("/api/publishers/:id", get(handlers::get_publisher))
        .route("/api/publishers/:id/contracts", get(handlers::get_publisher_contracts))
}

/// Health check routes
pub fn health_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::health_check))
        .route("/api/stats", get(handlers::get_stats))
}

/// Migration-related routes
pub fn migration_routes() -> Router<AppState> {
    Router::new()
        .route("/api/migrations", post(handlers::migrations::create_migration).get(handlers::migrations::get_migrations))
        .route("/api/migrations/:id", put(handlers::migrations::update_migration).get(handlers::migrations::get_migration))
}
