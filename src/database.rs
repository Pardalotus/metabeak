use crate::execution::model::{Event, HandlerSpec, RunResult};
use sqlx::{postgres::PgPoolOptions, prelude::FromRow, Pool, Postgres, Transaction};

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

#[derive(Debug, Copy, Clone)]
pub(crate) enum HandlerState {
    Enabled = 1,
    Disabled = 2,
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum EventQueueState {
    New = 1,
    Processed = 2,
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum MetadataSource {
    Unknown = 0,
    Test = 1,
    Crossref = 2,
}

impl MetadataSource {
    pub(crate) fn from_str_value(value: &str) -> MetadataSource {
        match value {
            "crossref" => MetadataSource::Crossref,
            "test" => MetadataSource::Test,
            _ => MetadataSource::Unknown,
        }
    }

    pub(crate) fn from_int_value(value: i32) -> MetadataSource {
        match value {
            2 => MetadataSource::Crossref,
            1 => MetadataSource::Test,
            _ => MetadataSource::Unknown,
        }
    }

    pub(crate) fn to_str_value(&self) -> String {
        String::from(match self {
            MetadataSource::Crossref => "crossref",
            MetadataSource::Test => "test",
            _ => "UNKNOWN",
        })
    }

    pub(crate) fn to_int_value(&self) -> i32 {
        match self {
            MetadataSource::Crossref => 2,
            MetadataSource::Test => 1,
            _ => 0,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum EventAnalyzerId {
    Unknown = 0,
    Test = 1,
    Lifecycle = 2,
}

impl EventAnalyzerId {
    pub(crate) fn from_str_value(value: &str) -> EventAnalyzerId {
        match value {
            "lifecycle" => EventAnalyzerId::Lifecycle,
            "test" => EventAnalyzerId::Test,
            _ => EventAnalyzerId::Unknown,
        }
    }

    pub(crate) fn to_str_value(&self) -> String {
        String::from(match self {
            EventAnalyzerId::Lifecycle => "lifecycle",
            EventAnalyzerId::Test => "test",
            _ => "UNKNOWN",
        })
    }

    pub(crate) fn from_int_value(value: i32) -> EventAnalyzerId {
        match value {
            2 => EventAnalyzerId::Lifecycle,
            1 => EventAnalyzerId::Test,
            _ => EventAnalyzerId::Unknown,
        }
    }

    pub(crate) fn to_int_value(self) -> i32 {
        match self {
            EventAnalyzerId::Lifecycle => 2,
            EventAnalyzerId::Test => 1,
            _ => 0,
        }
    }
}

pub(crate) async fn insert_task(
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
