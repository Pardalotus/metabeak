//! Service functions for metadata assertion agents to use.

use scholarly_identifiers::identifiers::Identifier;
use sqlx::{Pool, Postgres, Transaction};

use crate::{
    db::{self, metadata::MetadataAssertionReason, source::MetadataSourceId},
    util::hash_data,
};

/// Assert metadata about a subject from a given source.
/// If there's a duplidate assertion  based on the source and content, ignore it.
pub(crate) async fn assert_metadata<'a>(
    subject: &Identifier,
    metadata_json: &str,
    source: MetadataSourceId,
    tx: &mut Transaction<'a, Postgres>,
    pool: &Pool<Postgres>,
) -> Result<(), sqlx::Error> {
    // Do this out of transaction so it's idempotent and compatible with other concurrent transactions.
    // We won't ever want to roll this back.
    let subject_id = db::entity::resolve_identifier(subject, pool).await?;

    let hash = hash_data(metadata_json);

    db::metadata::insert_metadata_assertion(
        metadata_json,
        source,
        subject_id,
        &hash,
        MetadataAssertionReason::Primary,
        tx,
    )
    .await?;

    Ok(())
}
