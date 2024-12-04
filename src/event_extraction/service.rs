//! Service functions for event extraction.

use sqlx::{Pool, Postgres};

use crate::db::entity::resolve_identifier;
use crate::db::event::insert_event;
use crate::db::event::EventQueueState;
use crate::db::metadata::poll_assertions;
use crate::db::metadata::MetadataQueueEntry;
use crate::event_extraction::crossref;
use crate::execution::model::Event;
use crate::metadata_assertion;

const BATCH_SIZE: i32 = 1;

/// Poll the metadata queue and extract events. Return number of metadata
/// assertions read, and number of Events prodced.
///
/// Synchronously retrieve metadata for connected works.
///
/// This is transactional with respect to the queue polled and Events inserted.
/// Writes to entities table do not occur in the same transaction, allowing the
/// creation (and deduplicatoin) of identifiers to be effectively idempotent.
pub(crate) async fn pump_n(
    pool: &Pool<Postgres>,
    batch_size: i32,
) -> anyhow::Result<(usize, usize)> {
    let mut tx = pool.begin().await?;

    let assertions = poll_assertions(batch_size, &mut tx).await?;

    let count_processed = assertions.len();

    let events = metadata_assertions_to_events(assertions);
    let count_events = events.len();

    for event in events {
        log::debug!("Extract Event: {:?}", event);

        // Subject and Object are optional.
        let subject_entity_id = if let Some(ref id) = event.subject_id {
            Some(resolve_identifier(id, pool).await?)
        } else {
            None
        };

        let object_entity_id = if let Some(ref id) = event.object_id {
            Some(resolve_identifier(id, pool).await?)
        } else {
            None
        };

        log::debug!("Get assertions...");
        // Subject entity should have a metadata assertion by now, as it was used to generate events.
        // Ensure it here for consistency.
        if let (Some(ref identifier), Some(entity_id)) = (&event.subject_id, subject_entity_id) {
            metadata_assertion::retrieve::ensure_metadata_assertion(
                identifier, entity_id, &pool, &mut tx,
            )
            .await;
        }

        // Object entity usually won't have metadata assertion yet.
        if let (Some(ref identifier), Some(entity_id)) = (&event.object_id, object_entity_id) {
            metadata_assertion::retrieve::ensure_metadata_assertion(
                identifier, entity_id, &pool, &mut tx,
            )
            .await;
        }

        log::debug!("Insert...");
        insert_event(
            &event,
            subject_entity_id,
            object_entity_id,
            EventQueueState::New,
            &mut tx,
        )
        .await?;
    }

    tx.commit().await?;

    Ok((count_processed, count_events))
}

/// Extract Events from the given Metadata Assertions.
fn metadata_assertions_to_events(assertions: Vec<MetadataQueueEntry>) -> Vec<Event> {
    let mut results = vec![];

    for assertion in assertions {
        // There's no guarantee that the input will be JSON, depending on where it came from.
        // But parse this outside the handlers, else it forces each one to repeatedly deserialize.
        let json = match serde_json::from_str(&assertion.json) {
            Ok(json) => Some(json),
            Err(_) => None,
        };

        let mut events = crossref::extract_events(&assertion, json);
        log::info!(
            "Got {} events from assertion id  {} for {:?}",
            events.len(),
            assertion.assertion_id,
            assertion.subject_id()
        );
        results.append(&mut events);
    }

    results
}

/// Poll the metadata queue and extract events.
pub(crate) async fn drain(pool: &Pool<Postgres>) -> anyhow::Result<()> {
    let mut count = BATCH_SIZE;

    // Stop as soon as the page of events is not full, as it's the last page.
    while count >= BATCH_SIZE {
        let (count_assertions_read, count_events_produced) = pump_n(pool, BATCH_SIZE).await?;
        count = count_assertions_read as i32;

        log::debug!(
            "Polled {} metadata assertions to make {} events",
            count_assertions_read,
            count_events_produced,
        );
    }

    Ok(())
}
