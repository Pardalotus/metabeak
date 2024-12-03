//! Database pool.

use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use std::time::Duration;

pub(crate) async fn get_pool(uri: String) -> Result<Pool<Postgres>, sqlx::Error> {
    let pool: Pool<Postgres> = PgPoolOptions::new()
        .max_connections(50)
        // Allow for long transactions for bulk ingestion
        .idle_timeout(Duration::from_secs(60 * 60))
        .connect(&uri)
        .await?;

    Ok(pool)
}

pub(crate) async fn close_pool(pool: &Pool<Postgres>) {
    pool.close().await
}

/// Run a query against the database to check connectivity.
pub(crate) async fn heartbeat(pool: &sqlx::Pool<sqlx::Postgres>) -> Result<bool, sqlx::Error> {
    let result: i32 = sqlx::query_scalar("SELECT 1;").fetch_one(pool).await?;
    Ok(result == 1)
}
