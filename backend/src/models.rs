use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, FromRow)]
pub(crate) struct Task {
    pub(crate) id: i64,
    pub(crate) title: String,
    pub(crate) completed: bool,
    pub(crate) created_at: NaiveDateTime,
    pub(crate) updated_at: NaiveDateTime,
}

#[derive(Deserialize)]
pub(crate) struct LoginRequest {
    pub(crate) password: String,
}

#[derive(Deserialize)]
pub(crate) struct CreateTaskRequest {
    pub(crate) title: String,
}

#[derive(Deserialize)]
pub(crate) struct UpdateTaskRequest {
    pub(crate) title: Option<String>,
    pub(crate) completed: Option<bool>,
}
