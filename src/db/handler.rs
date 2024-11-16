//! Model and database functions for Handler Functions and execution results.

use crate::execution::model::{HandlerSpec, RunResult};
use sqlx::{Pool, Postgres, Transaction};

/// State of a handler function.
/// Currently they are always enabled.
#[derive(Debug, Copy, Clone)]
pub(crate) enum HandlerState {
    Enabled = 1,
}

pub(crate) async fn insert_handler(
    task: &HandlerSpec,
    hash: &str,
    owner_id: i32,
    status: HandlerState,
    pool: &Pool<Postgres>,
) -> Result<u64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO handler
         (owner_id, hash, code, status)
        VALUES ($1,$2, $3, $4)
        ON CONFLICT (hash) DO NOTHING
        RETURNING handler_id;",
    )
    .bind(owner_id)
    .bind(hash)
    .bind(&task.code)
    .bind(status as i32)
    .fetch_one(pool)
    .await?;

    Ok(row.0 as u64)
}

/// Retrieve all Handler functions that are enabled.
/// Assumes that there is a small enough number that they will fit in heap.
pub(crate) async fn all_enabled_handlers<'a>(
    tx: &mut Transaction<'a, Postgres>,
) -> Result<Vec<HandlerSpec>, sqlx::Error> {
    let rows: Vec<HandlerSpec> = sqlx::query_as(
        "SELECT handler_id, code FROM handler
         WHERE status = $1",
    )
    .bind(HandlerState::Enabled as i32)
    .fetch_all(&mut **tx)
    .await? as Vec<HandlerSpec>;

    Ok(rows)
}

pub(crate) async fn save_results<'a>(
    results: &[RunResult],
    tx: &mut Transaction<'a, Postgres>,
) -> Result<(), sqlx::Error> {
    for result in results.iter() {
        sqlx::query(
            "INSERT INTO execution_result
             (handler_id, event_id, result, error)
            VALUES ($1, $2, $3, $4);",
        )
        .bind(result.handler_id)
        .bind(result.event_id)
        .bind(&result.output)
        .bind(&result.error)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}
