mod db;
mod error;
mod handlers;
mod models;

use axum::{
    http::{header, HeaderValue, Method},
    routing::{get, post},
    Router,
};
use redis::aio::ConnectionManager;
use sqlx::MySqlPool;
use std::{env, sync::Arc};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db: MySqlPool,
    pub(crate) redis: ConnectionManager,
    pub(crate) fixed_password: Arc<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let database_url = required_env("DATABASE_URL");
    let redis_url = required_env("REDIS_URL");
    let fixed_password = required_env("FIXED_PASSWORD");
    let cors_origin = env::var("CORS_ORIGIN").unwrap_or_else(|_| "http://localhost:3000".into());

    let mysql = db::connect(&database_url).await;
    let redis_client = redis::Client::open(redis_url).expect("Invalid Redis URL");
    let redis = ConnectionManager::new(redis_client)
        .await
        .expect("Failed to connect to Redis");

    let state = AppState {
        db: mysql,
        redis,
        fixed_password: Arc::new(fixed_password),
    };
    let origin: HeaderValue = cors_origin.parse().expect("CORS_ORIGIN is invalid");

    let app = Router::new()
        .route("/health", get(handlers::health))
        .route("/api/session", get(handlers::session))
        .route("/api/login", post(handlers::login))
        .route("/api/logout", post(handlers::logout))
        .route(
            "/api/tasks",
            get(handlers::list_tasks).post(handlers::create_task),
        )
        .route(
            "/api/tasks/{id}",
            axum::routing::patch(handlers::update_task).delete(handlers::delete_task),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(origin)
                .allow_credentials(true)
                .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
                .allow_headers([header::CONTENT_TYPE]),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("Port 8080 is unavailable");
    tracing::info!("Taskflow API listening on :8080");
    axum::serve(listener, app)
        .await
        .expect("API server stopped");
}

fn required_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("Environment variable {name} is required"))
}
