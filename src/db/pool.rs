//! Database pool.

use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use std::time::Duration;

pub(crate) async fn get_pool() -> Result<Pool<Postgres>, sqlx::Error> {
    let pool: Pool<Postgres> = PgPoolOptions::new()
        .max_connections(5)
        // Allow for long transactions for bulk ingestion
        .idle_timeout(Duration::from_secs(60 * 60))
        .connect("postgres://metabeak:metabeak@localhost/metabeak")
        .await?;

    Ok(pool)
}

pub(crate) async fn close_pool(pool: &Pool<Postgres>) {
    pool.close().await
}
