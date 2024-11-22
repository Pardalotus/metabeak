use scholarly_identifiers::identifiers::Identifier;
use sqlx::{prelude::FromRow, Postgres, Transaction};

use super::source::MetadataSourceId;

/// Reason for making a metadata assertion.
/// Leaving space for a 'secondary' reason, which is metadata fetched in connection with a primary assertion.
#[derive(Clone, Copy)]
pub(crate) enum MetadataAssertionReason {
    /// Metadata fetched because the source indicated there was new metadata available for an entity.
    Primary = 1,
}

/// Insert a metadata assertion.
/// If there's a hash-based duplicate, ignore it.
pub(crate) async fn insert_metadata_assertion<'a>(
    json: &str,
    source: MetadataSourceId,
    subject_entity_id: i64,
    hash: &str,
    reason: MetadataAssertionReason,
    tx: &mut Transaction<'a, Postgres>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO metadata_assertion
         (json, source_id, subject_entity_id, hash, reason)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (subject_entity_id, hash, source_id)
        DO NOTHING;",
    )
    .bind(&json)
    .bind(source as i32)
    .bind(subject_entity_id)
    .bind(hash)
    .bind(reason as i16)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Result from polling the Metadata Assertion queue.
#[derive(FromRow, Debug)]
pub(crate) struct MetadataQueueEntry {
    pub(crate) source_id: i32,
    pub(crate) json: String,
    pub(crate) subject_id_type: i32,
    pub(crate) subject_id_value: String,
    pub(crate) assertion_id: i64,
}

impl MetadataQueueEntry {
    pub(crate) fn subject_id(&self) -> Identifier {
        // Expect that identifier types in the DB are always from the vocabulary.
        Identifier::from_id_string_pair(&self.subject_id_value, self.subject_id_type as u32)
            .unwrap()
    }
}

/// Poll from metadata_assertion_queue in a transaction. Uses SKIP LOCKED to avoid
/// deadlocking with other executions. Rows are locked until the transaction is
/// committed or aborted.
pub(crate) async fn poll_assertions<'a>(
    limit: i32,
    tx: &mut Transaction<'a, Postgres>,
) -> Result<Vec<MetadataQueueEntry>, sqlx::Error> {
    let rows: Vec<MetadataQueueEntry> = sqlx::query_as(
        "WITH
            assertions AS (
                SELECT
                    metadata_assertion_queue.queue_id as queue_id,
                    metadata_assertion.source_id as source_id,
                    metadata_assertion.json as json,
                    metadata_assertion.assertion_id as assertion_id,
                    subject.identifier_type as subject_id_type,
                    subject.identifier as subject_id_value
                FROM metadata_assertion_queue
                JOIN metadata_assertion
                ON metadata_assertion_queue.assertion_id = metadata_assertion.assertion_id
                JOIN entity AS subject ON subject.entity_id = metadata_assertion.subject_entity_id
                ORDER BY metadata_assertion_queue.queue_id ASC
                FOR UPDATE SKIP LOCKED
                LIMIT $1),
            queue_ids AS (SELECT queue_id FROM assertions),
            deleted AS (
                DELETE FROM metadata_assertion_queue
                WHERE queue_id IN (SELECT queue_id FROM queue_ids))
        SELECT * FROM assertions;",
    )
    .bind(limit)
    .fetch_all(&mut **tx)
    .await? as Vec<MetadataQueueEntry>;

    Ok(rows)
}
