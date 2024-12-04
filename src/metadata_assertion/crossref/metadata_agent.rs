//! Agent for retrieving metadata assertions from the Crossref API.

use std::sync::mpsc::{self, Receiver, Sender};

use scholarly_identifiers::identifiers::Identifier;
use sqlx::{Pool, Postgres};

use time::{Duration, OffsetDateTime};

use crate::db::agents::get_checkpoint;
use crate::db::agents::set_checkpoint;
use crate::db::metadata::MetadataAssertionReason;
use crate::metadata_assertion::crossref::works_api_client::harvest_with_filter_to_chan;
use crate::metadata_assertion::crossref::{
    metadata::get_index_date, works_api_client::harvest_precise_index_date,
};
use crate::metadata_assertion::service::assert_metadata;

/// Date value for checkpointing the harvest.
const CROSSREF_NB: &str = "crossref-not-before";

/// Retrieve all new Crossref data since the last run.
/// The date used for checkpointing is the latest indexed date reported by the Crossref API, not the local datetime.
pub(crate) async fn poll_newly_indexed_data(pool: &Pool<Postgres>) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    // Start from most recent run, now.
    // Add 1 hour margin for jitter. This results in duplicate fetches but they are de-duplicated in the database.
    let saturating_sub = get_checkpoint(CROSSREF_NB, &mut tx)
        .await?
        .unwrap_or(OffsetDateTime::now_utc())
        .saturating_sub(Duration::HOUR);
    let after = saturating_sub;

    // Get only assertions indexed after the date.
    let new_after = harvest_recently_indexed(&after, pool).await?;

    set_checkpoint(CROSSREF_NB, new_after, &mut tx).await?;

    tx.commit().await?;

    Ok(())
}

/// Retrieve all Crossref data matching given Crossref REST API filter.
pub(crate) async fn fetch_secondary_metadata_with_filter(
    pool: &Pool<Postgres>,
    filter: String,
) -> anyhow::Result<()> {
    let tx = pool.begin().await?;

    harvest_secondary_with_filter(filter, pool).await?;

    tx.commit().await?;

    Ok(())
}

pub(crate) fn get_identifier_and_json(
    json_value: serde_json::Value,
) -> Option<(Identifier, String)> {
    if let Some(doi) = &json_value["DOI"].as_str() {
        // Normalise and identify the type of the identifier.
        // For Crossref records, this will be the DOI type ID.
        let identifier = scholarly_identifiers::identifiers::Identifier::parse(doi);

        if let Ok(json_value) = serde_json::to_string(&json_value) {
            Some((identifier, json_value))
        } else {
            None
        }
    } else {
        None
    }
}

/// Harvest data until the given date, returning the index date of the most recent.
/// If none were retrieved, the `after` date is returned, so it can be attepmted again next time.
pub(crate) async fn harvest_recently_indexed<'a>(
    after: &OffsetDateTime,
    pool: &Pool<Postgres>,
) -> anyhow::Result<OffsetDateTime> {
    let (send_metadata_docs, receive_metadata_docs): (
        Sender<serde_json::Value>,
        Receiver<serde_json::Value>,
    ) = mpsc::channel();
    let after_a = *after;
    let c =
        tokio::task::spawn(
            async move { harvest_precise_index_date(send_metadata_docs, after_a).await },
        );

    let mut latest_date = *after;

    log::info!("Start harvest after {}", after);
    let mut count = 0;
    let mut tx = pool.begin().await?;

    for item in receive_metadata_docs {
        if let Some(indexed) = get_index_date(&item) {
            latest_date = indexed.max(latest_date);

            if let Some((identifier, json)) = get_identifier_and_json(item) {
                count += 1;
                if (count % 1000) == 0 {
                    log::info!("Harvested {} items.", count);
                }

                assert_metadata(
                    &identifier,
                    &json,
                    crate::db::source::MetadataSourceId::Crossref,
                    MetadataAssertionReason::Primary,
                    pool,
                    &mut tx,
                )
                .await?;
            }
        }
    }
    tx.commit().await?;

    log::info!("Stop harvest, retrieved {}, latest {}", count, latest_date);

    c.await?.unwrap();
    Ok(latest_date)
}

/// Harvest data until the given date, returning the index date of the most recent.
/// If none were retrieved, the `after` date is returned, so it can be attepmted again next time.
pub(crate) async fn harvest_secondary_with_filter<'a>(
    filter: String,
    pool: &Pool<Postgres>,
) -> anyhow::Result<()> {
    log::info!("Start harvest for filter {}", filter);

    let (send_metadata_docs, receive_metadata_docs): (
        Sender<serde_json::Value>,
        Receiver<serde_json::Value>,
    ) = mpsc::channel();
    let c =
        tokio::task::spawn(
            async move { harvest_with_filter_to_chan(send_metadata_docs, filter).await },
        );

    let mut count = 0;
    let mut tx = pool.begin().await?;
    for item in receive_metadata_docs {
        if let Some((identifier, json)) = get_identifier_and_json(item) {
            count += 1;
            if (count % 1000) == 0 {
                log::info!("Harvested {} items.", count);
            }

            assert_metadata(
                &identifier,
                &json,
                crate::db::source::MetadataSourceId::Crossref,
                MetadataAssertionReason::Secondary,
                pool,
                &mut tx,
            )
            .await?;
        }
    }

    tx.commit().await?;

    log::info!("Stop harvest, retrieved {}", count);

    c.await?.unwrap();

    Ok(())
}
