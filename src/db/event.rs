//! Model and database functions for Events and Event Queue.

use sqlx::{prelude::FromRow, Postgres, Transaction};

use crate::execution::model::Event;

use super::source::{EventAnalyzerId, MetadataSource};

#[derive(Debug, Copy, Clone)]
pub(crate) enum EventQueueState {
    New = 1,
    Processed = 2,
}

/// Insert an Event.
/// Ignore the pre-existing event_id, create a new one.
pub(crate) async fn insert_event<'a>(
    event: &Event,
    status: EventQueueState,
    tx: &mut Transaction<'a, Postgres>,
) -> Result<u64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO event
         (json, status, source, analyzer)
        VALUES ($1, $2, $3, $4)
        RETURNING event_id;",
    )
    .bind(&event.json)
    .bind(status as i32)
    .bind(event.source as i32)
    .bind(event.analyzer as i32)
    .fetch_one(&mut **tx)
    .await?;

    Ok(row.0 as u64)
}

/// Result from polling the Event Queue.
#[derive(FromRow, Debug)]
pub(crate) struct EventQueueEntry {
    pub(crate) event_id: i64,
    pub(crate) analyzer: i32,
    pub(crate) source: i32,
    pub(crate) json: String,
}

/// Poll from execution_events queue in a transaction. Uses SKIP LOCKED to avoid
/// deadlocking with other executions. Rows are locked until the transaction is
/// committed or aborted.
pub(crate) async fn poll<'a>(
    limit: i32,
    tx: &mut Transaction<'a, Postgres>,
) -> Result<Vec<Event>, sqlx::Error> {
    let rows: Vec<EventQueueEntry> = sqlx::query_as(
        "WITH
            events AS (
                SELECT * FROM event_queue
                JOIN event
                ON event_queue.event_id = event.event_id
                FOR UPDATE SKIP LOCKED
                LIMIT $1),
            ids AS (SELECT execution_id from events),
            deleted AS (
                DELETE FROM event_queue
                WHERE execution_id IN (SELECT execution_id FROM ids))
        SELECT * from events;",
    )
    .bind(limit)
    .fetch_all(&mut **tx)
    .await? as Vec<EventQueueEntry>;

    Ok(rows
        .iter()
        .map(|r| Event {
            event_id: r.event_id,
            analyzer: EventAnalyzerId::from_int_value(r.analyzer),
            source: MetadataSource::from_int_value(r.source),
            // OPTIMIZE
            json: r.json.clone(),
        })
        .collect())
}
