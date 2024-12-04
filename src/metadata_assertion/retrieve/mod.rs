use scholarly_identifiers::identifiers::Identifier;
use sqlx::{Pool, Postgres, Transaction};

use crate::db;

pub(crate) mod doi;
pub(crate) mod ror;

/// Attempt to ensure an entity has a metadata assertion.
pub(crate) async fn ensure_metadata_assertion<'a>(
    identifier: &Identifier,
    entity_id: i64,
    pool: &Pool<Postgres>,
    tx: &mut Transaction<'a, Postgres>,
) {
    // Check for presence of any metadata assertion on that entity.
    // The source and date don't matter, as we'll take the latest.
    if !db::metadata::has_metadata_assertion(entity_id, pool).await {
        // Each function will only assert metadata if it recognises the type. No need for a switch here, as it would be extraneous.
        if let Err(err) = doi::try_collect_metadata_assertion(identifier, pool, tx).await {
            log::error!("Failed to collect metadata for {:?}, {:?}", identifier, err);
        }
    } else {
        log::debug!("Already got metadata for {:?}, {}", identifier, entity_id);
    }
}
