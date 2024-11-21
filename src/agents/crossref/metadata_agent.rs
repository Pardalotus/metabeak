//! Agent for retrieving metadata assertions from the Crossref API.

use std::sync::mpsc::{self, Receiver, Sender};

use sqlx::{Pool, Postgres, Transaction};

use time::{Duration, OffsetDateTime};

use scholarly_identifiers;

use crate::{
    agents::crossref::{metadata::get_index_date, works_api_client::harvest_to_channel},
    db::{agents::set_checkpoint, metadata::poll_assertions, source::MetadataSourceId},
};
use crate::{agents::service::assert_metadata, db::agents::get_checkpoint};

/// Date value for checkpointing the harvest.
const CROSSREF_NB: &str = "crossref-not-before";

/// Retrieve all new Crossref data since the last run.
/// The date used for checkpointing is the latest indexed date reported by the Crossref API, not the local datetime.
pub(crate) async fn pump_metadata(pool: &Pool<Postgres>) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    // Start from most recent run, now.
    // Add 1 hour margin for jitter. This results in duplicate fetches but they are de-duplicated in the database.
    let saturating_sub = get_checkpoint(CROSSREF_NB, &mut tx)
        .await?
        .unwrap_or(OffsetDateTime::now_utc())
        .saturating_sub(Duration::HOUR);
    let after = saturating_sub;

    let new_after = harvest(&after, &mut tx, pool).await?;

    set_checkpoint(CROSSREF_NB, new_after, &mut tx).await?;

    tx.commit().await?;

    Ok(())
}

/// Harvest data until the given date, returning the index date of the most recent.
/// If none were retrieved, the `after` date is returned, so it can be attepmted again next time.
pub(crate) async fn harvest<'a>(
    after: &OffsetDateTime,
    tx: &mut Transaction<'a, Postgres>,
    pool: &Pool<Postgres>,
) -> anyhow::Result<OffsetDateTime> {
    let (send_metadata_docs, receive_metadata_docs): (
        Sender<serde_json::Value>,
        Receiver<serde_json::Value>,
    ) = mpsc::channel();
    let after_a = *after;
    let c =
        tokio::task::spawn(async move { harvest_to_channel(send_metadata_docs, after_a).await });

    let mut latest_date = *after;

    log::info!("Start harvest after {}", after);
    let mut count = 0;
    for item in receive_metadata_docs {
        if let Some(indexed) = get_index_date(&item) {
            latest_date = indexed.max(latest_date);

            if let Some(doi) = &item["DOI"].as_str() {
                // Normalise and identify the type of the identifier.
                // For Crossref records, this will be the DOI type ID.
                let identifier = scholarly_identifiers::identifiers::Identifier::parse(doi);

                if let Ok(json) = serde_json::to_string(&item) {
                    count += 1;
                    if (count % 1000) == 0 {
                        log::info!("Done {} items.", count);
                    }

                    assert_metadata(
                        &identifier,
                        &json,
                        crate::db::source::MetadataSourceId::Crossref,
                        tx,
                        pool,
                    )
                    .await?;
                }
            }
        }
    }
    log::info!("Stop harvest, retrieved {}, latest {}", count, latest_date);

    c.await?.unwrap();
    Ok(latest_date)
}

/// Poll the metadata queue and extract events.
pub(crate) async fn pump_events(pool: &Pool<Postgres>) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    let assertions = poll_assertions(10, MetadataSourceId::Crossref, &mut tx).await?;

    println!("POLL: {:?}", assertions.len());

    tx.commit().await?;

    Ok(())
}