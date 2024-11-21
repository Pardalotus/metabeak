//! Service functions for event extraction.

use sqlx::Pool;
use sqlx::Postgres;

use crate::db::entity::resolve_identifier;
use crate::db::event::insert_event;
use crate::db::event::EventQueueState;
use crate::db::metadata::poll_assertions;
use crate::db::metadata::MetadataQueueEntry;
use crate::event_extraction::crossref;
use crate::execution::model::Event;

const BATCH_SIZE: i32 = 100;

/// Poll the metadata queue and extract events.
/// Return number of metadata assertions read, and number of Events prodced.
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

fn metadata_assertions_to_events(assertions: Vec<MetadataQueueEntry>) -> Vec<Event> {
    let mut results = vec![];

    for assertion in assertions {
        let mut events = crossref::extract_events(&assertion);
        results.append(&mut events);
    }

    results
}

/// Poll the metadata queue and extract events.
pub(crate) async fn drain(pool: &Pool<Postgres>) -> anyhow::Result<()> {
    let mut count = BATCH_SIZE;

    // Stop as soon as the page of events is not full, as it's the last page.
    while count >= BATCH_SIZE {
        let (count_assertions_read, count_events_produced) = pump_n(&pool, BATCH_SIZE).await?;
        count = count_assertions_read as i32;

        log::info!(
            "Polled {} metadata assertions to make {} events",
            count_assertions_read,
            count_events_produced,
        );
    }

    Ok(())
}