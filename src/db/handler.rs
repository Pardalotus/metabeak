//! Model and database functions for Handler Functions and execution results.

use crate::execution::model::{ExecutionResult, HandlerSpec};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres, Transaction};

/// State of a handler function.
/// Currently they are always enabled.
#[derive(Clone, Debug, PartialEq, PartialOrd, sqlx::Type, Deserialize, Serialize)]
pub enum HandlerState {
    Enabled = 1,
    Disabled = 2,
    Unknown = 3,
}

/// Insert a handler function.
/// Returning the Handler ID, and boolean flag to indicate if it was newly created or already existed.
pub(crate) async fn insert_handler(
    task: &HandlerSpec,
    hash: &str,
    owner_id: i32,
    status: HandlerState,
    pool: &Pool<Postgres>,
) -> Result<(i64, bool), sqlx::Error> {
    let row: (Option<i64>, Option<i64>) = sqlx::query_as(
        "WITH new_id AS (
                    INSERT INTO handler
                    (owner_id, hash, code, status)
                    VALUES ($1, $2, $3, $4)
                    ON CONFLICT (hash) DO NOTHING
                    RETURNING handler_id),
        old_id AS (SELECT handler_id
                    FROM handler
                    WHERE hash = $2 LIMIT 1)
        SELECT (SELECT * from new_id) AS new, (SELECT * FROM old_id) AS old;",
    )
    .bind(owner_id)
    .bind(hash)
    .bind(&task.code)
    .bind(status as i32)
    .fetch_one(pool)
    .await?;

    match row {
        (Some(new), _) => Ok((new, true)),
        (None, Some(old)) => Ok((old, false)),
        _ => Err(sqlx::Error::RowNotFound),
    }
}

/// Retrieve all Handler functions that are enabled.
/// Assumes that there is a small enough number that they will fit in heap.
pub(crate) async fn get_all_enabled_handlers<'a>(
    tx: &mut Transaction<'a, Postgres>,
) -> Result<Vec<HandlerSpec>, sqlx::Error> {
    let rows: Vec<HandlerSpec> = sqlx::query_as(
        "SELECT *
         FROM handler
         WHERE status = $1",
    )
    .bind(HandlerState::Enabled as i32)
    .fetch_all(&mut **tx)
    .await? as Vec<HandlerSpec>;

    Ok(rows)
}

/// Save a set of [RunResult]s.
pub(crate) async fn save_results<'a>(
    results: &[ExecutionResult],
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
        .bind(&result.result)
        .bind(&result.error)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

pub(crate) async fn get_by_id(
    pool: &Pool<Postgres>,
    handler_id: i64,
) -> Result<HandlerSpec, sqlx::Error> {
    sqlx::query_as(
        "SELECT
            handler_id,
            code,
            status
         FROM handler
         WHERE handler_id = $1
         LIMIT 1;",
    )
    .bind(handler_id)
    .fetch_one(pool)
    .await
}

/// Get successful results for handler after cursor.
pub(crate) async fn get_success_results(
    pool: &Pool<Postgres>,
    handler_id: i64,
    after: i64,
    limit: i32,
) -> Result<Vec<ExecutionResult>, sqlx::Error> {
    // Use success_execution_idx
    let rows: Vec<ExecutionResult> = sqlx::query_as(
        "SELECT * FROM execution_result
         WHERE
            handler_id = $1
         AND
            result_id > $2
         AND
           result IS NOT NULL
         ORDER BY result_id ASC
         LIMIT $3
         ",
    )
    .bind(handler_id)
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await? as Vec<ExecutionResult>;

    Ok(rows)
}

/// Get all results for handler after cursor.
pub(crate) async fn get_all_results(
    pool: &Pool<Postgres>,
    handler_id: i64,
    after: i64,
    limit: i32,
) -> Result<Vec<ExecutionResult>, sqlx::Error> {
    // Use all_execution_idx
    let rows: Vec<ExecutionResult> = sqlx::query_as(
        "SELECT * FROM execution_result
         WHERE
            handler_id = $1
         AND
            result_id > $2
         ORDER BY result_id ASC
         LIMIT $3
         ",
    )
    .bind(handler_id)
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await? as Vec<ExecutionResult>;

    Ok(rows)
}
