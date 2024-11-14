//! Database pool.

use sqlx::{postgres::PgPoolOptions, Pool, Postgres};

pub(crate) async fn get_pool() -> Result<Pool<Postgres>, sqlx::Error> {
    let pool: Pool<Postgres> = PgPoolOptions::new()
        .max_connections(5)
        .connect("postgres://metabeak:metabeak@localhost/metabeak")
        .await?;

    Ok(pool)
}

pub(crate) async fn close_pool(pool: &Pool<Postgres>) {
    pool.close().await
}
