use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::NaiveDateTime;
use redis::{aio::ConnectionManager, AsyncCommands};
use serde::{Deserialize, Serialize};
use sqlx::{mysql::MySqlPoolOptions, FromRow, MySqlPool};
use std::{env, sync::Arc};
use thiserror::Error;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

const SESSION_COOKIE: &str = "task_session";
const SESSION_TTL_SECONDS: u64 = 60 * 60 * 24;

#[derive(Clone)]
struct AppState {
    db: MySqlPool,
    redis: ConnectionManager,
    fixed_password: Arc<String>,
}

#[derive(Debug, Error)]
enum AppError {
    #[error("ログインが必要です")]
    Unauthorized,
    #[error("タスクが見つかりません")]
    NotFound,
    #[error("{0}")]
    BadRequest(String),
    #[error("内部エラーが発生しました")]
    Internal,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(serde_json::json!({ "message": self.to_string() }))).into_response()
    }
}

#[derive(Debug, Serialize, FromRow)]
struct Task {
    id: i64,
    title: String,
    completed: bool,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

#[derive(Deserialize)]
struct LoginRequest {
    password: String,
}

#[derive(Deserialize)]
struct CreateTaskRequest {
    title: String,
}

#[derive(Deserialize)]
struct UpdateTaskRequest {
    title: Option<String>,
    completed: Option<bool>,
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

    let db = MySqlPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .expect("MySQLへの接続に失敗しました");
    sqlx::migrate!().run(&db).await.expect("DBマイグレーションに失敗しました");

    let redis_client = redis::Client::open(redis_url).expect("Redis URLが不正です");
    let redis = ConnectionManager::new(redis_client)
        .await
        .expect("Redisへの接続に失敗しました");

    let state = AppState { db, redis, fixed_password: Arc::new(fixed_password) };
    let origin: HeaderValue = cors_origin.parse().expect("CORS_ORIGINが不正です");

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/session", get(session))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route("/api/tasks/{id}", axum::routing::patch(update_task).delete(delete_task))
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
        .expect("ポート8080を使用できません");
    tracing::info!("Taskflow API listening on :8080");
    axum::serve(listener, app).await.expect("APIサーバーが停止しました");
}

fn required_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("環境変数{name}が必要です"))
}

async fn health() -> &'static str {
    "ok"
}

async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    if payload.password != *state.fixed_password {
        return Err(AppError::Unauthorized);
    }

    let token = Uuid::new_v4().to_string();
    let mut redis = state.redis.clone();
    redis
        .set_ex::<_, _, ()>(format!("session:{token}"), "single-user", SESSION_TTL_SECONDS)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to create session");
            AppError::Internal
        })?;

    let cookie = format!(
        "{SESSION_COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={SESSION_TTL_SECONDS}"
    );
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie.parse().map_err(|_| AppError::Internal)?);
    Ok((headers, Json(serde_json::json!({ "authenticated": true }))))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<impl IntoResponse, AppError> {
    if let Some(token) = session_token(&headers) {
        let mut redis = state.redis.clone();
        let _: Result<(), _> = redis.del(format!("session:{token}")).await;
    }
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::SET_COOKIE,
        format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0")
            .parse()
            .map_err(|_| AppError::Internal)?,
    );
    Ok((response_headers, StatusCode::NO_CONTENT))
}

async fn session(State(state): State<AppState>, headers: HeaderMap) -> Json<serde_json::Value> {
    let authenticated = is_authenticated(&state, &headers).await.unwrap_or(false);
    Json(serde_json::json!({ "authenticated": authenticated }))
}

async fn list_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Task>>, AppError> {
    require_auth(&state, &headers).await?;
    let tasks = sqlx::query_as::<_, Task>(
        "SELECT id, title, completed, created_at, updated_at FROM tasks ORDER BY created_at DESC, id DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(db_error)?;
    Ok(Json(tasks))
}

async fn create_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<Task>), AppError> {
    require_auth(&state, &headers).await?;
    let title = validate_title(&payload.title)?;
    let result = sqlx::query("INSERT INTO tasks (title) VALUES (?)")
        .bind(title)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    let task = find_task(&state.db, result.last_insert_id() as i64).await?;
    Ok((StatusCode::CREATED, Json(task)))
}

async fn update_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateTaskRequest>,
) -> Result<Json<Task>, AppError> {
    require_auth(&state, &headers).await?;
    let current = find_task(&state.db, id).await?;
    let title = match payload.title {
        Some(title) => validate_title(&title)?,
        None => current.title,
    };
    let completed = payload.completed.unwrap_or(current.completed);
    sqlx::query("UPDATE tasks SET title = ?, completed = ? WHERE id = ?")
        .bind(title)
        .bind(completed)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    Ok(Json(find_task(&state.db, id).await?))
}

async fn delete_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    require_auth(&state, &headers).await?;
    let result = sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn find_task(db: &MySqlPool, id: i64) -> Result<Task, AppError> {
    sqlx::query_as::<_, Task>(
        "SELECT id, title, completed, created_at, updated_at FROM tasks WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await
    .map_err(db_error)?
    .ok_or(AppError::NotFound)
}

fn validate_title(title: &str) -> Result<String, AppError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("タスク名を入力してください".into()));
    }
    if title.chars().count() > 255 {
        return Err(AppError::BadRequest("タスク名は255文字以内にしてください".into()));
    }
    Ok(title.to_owned())
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|cookie| cookie.strip_prefix(&format!("{SESSION_COOKIE}=")).map(str::to_owned))
}

async fn is_authenticated(state: &AppState, headers: &HeaderMap) -> Result<bool, AppError> {
    let Some(token) = session_token(headers) else { return Ok(false) };
    let mut redis = state.redis.clone();
    redis
        .exists(format!("session:{token}"))
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to check session");
            AppError::Internal
        })
}

async fn require_auth(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    if is_authenticated(state, headers).await? {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(?error, "database error");
    AppError::Internal
}
