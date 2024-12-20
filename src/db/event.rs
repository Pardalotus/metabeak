//! Model and database functions for Events and Event Queue.

use scholarly_identifiers::identifiers::Identifier;
use sqlx::{prelude::FromRow, Postgres, Transaction};

use crate::execution::model::Event;

use super::source::{EventAnalyzerId, MetadataSourceId};

/// State of an Event Queue item.
/// Currently only 'new', as event queue items will be deleted once handled.
#[derive(Debug, Copy, Clone)]
pub(crate) enum EventQueueState {
    New = 1,
}

/// Insert an Event.
/// Ignore the pre-existing event_id, create a new one.
pub(crate) async fn insert_event<'a>(
    event: &Event,
    subject_entity_id: Option<i64>,
    object_entity_id: Option<i64>,
    status: EventQueueState,
    tx: &mut Transaction<'a, Postgres>,
) -> Result<u64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO event
         (json, status, source_id, analyzer_id, subject_entity_id, object_entity_id, assertion_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING event_id;",
    )
    .bind(&event.json)
    .bind(status as i32)
    .bind(event.source as i32)
    .bind(event.analyzer as i32)
    .bind(subject_entity_id)
    .bind(object_entity_id)
    .bind(event.assertion_id)
    .fetch_one(&mut **tx)
    .await?;

    Ok(row.0 as u64)
}

/// Result from polling the Event Queue.
#[derive(FromRow, Debug)]
pub(crate) struct EventQueueEntry {
    pub(crate) event_id: i64,
    pub(crate) analyzer_id: i32,
    pub(crate) source_id: i32,
    pub(crate) json: String,
    pub(crate) subject_id_type: Option<i32>,
    pub(crate) subject_id_value: Option<String>,
    pub(crate) object_id_type: Option<i32>,
    pub(crate) object_id_value: Option<String>,
    pub(crate) assertion_id: i64,
}

impl EventQueueEntry {
    fn to_event(self) -> Event {
        Event {
            event_id: self.event_id,
            analyzer: EventAnalyzerId::from_int_value(self.analyzer_id),
            source: MetadataSourceId::from_int_value(self.source_id),
            assertion_id: self.assertion_id,
            // Subject and Object are optional fields, but type and value occur together.
            subject_id: if let (Some(id_type), Some(id_val)) =
                (self.subject_id_type, &self.subject_id_value)
            {
                Identifier::from_id_string_pair(id_val, id_type as u32)
            } else {
                None
            },
            object_id: if let (Some(id_type), Some(id_val)) =
                (self.object_id_type, &self.object_id_value)
            {
                Identifier::from_id_string_pair(id_val, id_type as u32)
            } else {
                None
            },
            json: self.json,
        }
    }
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
            entries AS (
                SELECT
                    event_queue.event_queue_id as event_queue_id,
                    event_queue.event_id as event_id
                FROM event_queue
                ORDER BY event_queue.event_queue_id ASC
                FOR UPDATE SKIP LOCKED
                LIMIT $1
            ),
            events AS (
                SELECT
                    event.event_id as event_id,
                    event.analyzer_id as analyzer_id,
                    event.source_id as source_id,
                    event.assertion_id as assertion_id,
                    subject.identifier_type as subject_id_type,
                    subject.identifier as subject_id_value,
                    object.identifier_type as object_id_type,
                    object.identifier as object_id_value,
                    event.json as json
                FROM
                    entries
                    INNER JOIN event ON entries.event_id = event.event_id
                    LEFT JOIN entity AS subject ON subject.entity_id = event.subject_entity_id
                    LEFT JOIN entity AS object ON object.entity_id = event.object_entity_id
            ),
            deleted AS (
                DELETE FROM event_queue
                WHERE event_queue_id IN (SELECT event_queue_id FROM entries))
        SELECT * from events;",
    )
    .bind(limit)
    .fetch_all(&mut **tx)
    .await? as Vec<EventQueueEntry>;

    Ok(rows.into_iter().map(|r| r.to_event()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subj_obj_present() {
        let result = EventQueueEntry {
            event_id: 1,
            analyzer_id: 2,
            source_id: 1,
            json: String::from("{\"hello\": \"world\", \"foo\": \"bar\"}"),
            subject_id_type: Some(1), // Type of DOI from `scholarly_identifiers` crate.
            subject_id_value: Some(String::from("10.5555/12345678")),
            object_id_type: Some(1), // Type of DOI from `scholarly_identifiers` crate.
            object_id_value: Some(String::from("10.5555/87654321")),
            assertion_id: -1,
        };

        let event = result.to_event();

        assert_eq!(
            event.analyzer,
            EventAnalyzerId::Lifecycle,
            "Event type should be mapped."
        );
        assert_eq!(
            event.source,
            MetadataSourceId::Test,
            "MetadataSource should be mapped"
        );
        assert_eq!(
            event.json,
            String::from("{\"hello\": \"world\", \"foo\": \"bar\"}"),
            "JSON should be copied un-changed"
        );
        assert_eq!(
            event.subject_id,
            Some(Identifier::parse("https://doi.org/10.5555/12345678")),
            "Subject DOI should be constructed from inputs"
        );
        assert_eq!(
            event.object_id,
            Some(Identifier::parse("https://doi.org/10.5555/87654321")),
            "Object DOI should be constructed from inputs"
        );

        assert_eq!(event.event_id, 1, "Same Event ID should be copied");
    }

    #[test]
    fn subj_obj_absent() {
        let result = EventQueueEntry {
            event_id: 1,
            analyzer_id: 2,
            source_id: 1,
            json: String::from("{\"hello\": \"world\", \"foo\": \"bar\"}"),
            subject_id_type: None,
            subject_id_value: None,
            object_id_type: None,
            object_id_value: None,
            assertion_id: -1,
        };

        let event = result.to_event();

        assert_eq!(
            event.analyzer,
            EventAnalyzerId::Lifecycle,
            "Event type should be mapped."
        );
        assert_eq!(
            event.source,
            MetadataSourceId::Test,
            "MetadataSource should be mapped"
        );
        assert_eq!(
            event.json,
            String::from("{\"hello\": \"world\", \"foo\": \"bar\"}"),
            "JSON should be copied un-changed"
        );
        assert_eq!(
            event.subject_id, None,
            "Subject ID maps to None if missing."
        );
        assert_eq!(event.object_id, None, "Object ID maps to None if missing.");

        assert_eq!(event.event_id, 1, "Same Event ID should be copied");
    }

    /// For both Subject and Object the type and value must be present together as a pair.
    /// Test that getting them separately doesn't cause issues.
    #[test]
    fn subj_obj_partial() {
        let result = EventQueueEntry {
            event_id: 1,
            analyzer_id: 2,
            source_id: 1,
            json: String::from("{\"hello\": \"world\", \"foo\": \"bar\"}"),
            subject_id_type: None,
            subject_id_value: Some(String::from("10.5555/12345678")),
            object_id_type: None,
            object_id_value: Some(String::from("10.5555/87654321")),
            assertion_id: -1,
        };

        let event = result.to_event();

        assert_eq!(
            event.subject_id, None,
            "Subject should be None unless both type and value are present"
        );
        assert_eq!(
            event.object_id, None,
            "Object should be None unless both type and value are present"
        );

        let result = EventQueueEntry {
            event_id: 1,
            analyzer_id: 2,
            source_id: 1,
            json: String::from("{\"hello\": \"world\", \"foo\": \"bar\"}"),
            subject_id_type: Some(1),
            subject_id_value: None,
            object_id_type: Some(1),
            object_id_value: None,
            assertion_id: -1,
        };

        let event = result.to_event();

        assert_eq!(
            event.subject_id, None,
            "Subject should be None unless both type and value are present"
        );
        assert_eq!(
            event.object_id, None,
            "Object should be None unless both type and value are present"
        );
    }
}
