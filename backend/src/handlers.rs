use crate::{
    error::{self, AppError},
    models::{CreateTaskRequest, LoginRequest, Task, UpdateTaskRequest},
    AppState,
};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use redis::AsyncCommands;
use sqlx::MySqlPool;
use uuid::Uuid;

const SESSION_COOKIE: &str = "task_session";
const SESSION_TTL_SECONDS: u64 = 60 * 60 * 24;

pub(crate) async fn health() -> &'static str {
    "ok"
}

pub(crate) async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    if payload.password != *state.fixed_password {
        return Err(AppError::Unauthorized);
    }

    let token = Uuid::new_v4().to_string();
    let mut redis = state.redis.clone();
    redis
        .set_ex::<_, _, ()>(
            format!("session:{token}"),
            "single-user",
            SESSION_TTL_SECONDS,
        )
        .await
        .map_err(|error| {
            tracing::error!(?error, "Failed to create session");
            AppError::Internal
        })?;

    let cookie = format!(
        "{SESSION_COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={SESSION_TTL_SECONDS}"
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        cookie.parse().map_err(|_| AppError::Internal)?,
    );
    Ok((
        headers,
        Json(serde_json::json!({ "authenticated": true })),
    ))
}

pub(crate) async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
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

pub(crate) async fn session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    let authenticated = is_authenticated(&state, &headers).await.unwrap_or(false);
    Json(serde_json::json!({ "authenticated": authenticated }))
}

pub(crate) async fn list_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Task>>, AppError> {
    require_auth(&state, &headers).await?;
    let tasks = sqlx::query_as::<_, Task>(
        "SELECT id, title, completed, created_at, updated_at FROM tasks ORDER BY created_at DESC, id DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(error::database)?;
    Ok(Json(tasks))
}

pub(crate) async fn create_task(
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
        .map_err(error::database)?;
    let task = find_task(&state.db, result.last_insert_id() as i64).await?;
    Ok((StatusCode::CREATED, Json(task)))
}

pub(crate) async fn update_task(
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
        .map_err(error::database)?;
    Ok(Json(find_task(&state.db, id).await?))
}

pub(crate) async fn delete_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    require_auth(&state, &headers).await?;
    let result = sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(error::database)?;
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
    .map_err(error::database)?
    .ok_or(AppError::NotFound)
}

fn validate_title(title: &str) -> Result<String, AppError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("Please enter a task title".into()));
    }
    if title.chars().count() > 255 {
        return Err(AppError::BadRequest(
            "Task title must be 255 characters or fewer".into(),
        ));
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
        .find_map(|cookie| {
            cookie
                .strip_prefix(&format!("{SESSION_COOKIE}="))
                .map(str::to_owned)
        })
}

async fn is_authenticated(state: &AppState, headers: &HeaderMap) -> Result<bool, AppError> {
    let Some(token) = session_token(headers) else {
        return Ok(false);
    };
    let mut redis = state.redis.clone();
    redis
        .exists(format!("session:{token}"))
        .await
        .map_err(|error| {
            tracing::error!(?error, "Failed to check session");
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
