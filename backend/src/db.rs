use sqlx::{mysql::MySqlPoolOptions, MySqlPool};

pub(crate) async fn connect(database_url: &str) -> MySqlPool {
    let pool = MySqlPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
        .expect("Failed to connect to MySQL");

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Failed to run database migrations");

    pool
}
