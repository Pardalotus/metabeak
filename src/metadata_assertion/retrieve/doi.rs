use anyhow::Result;
use backon::ConstantBuilder;
use backon::Retryable;
use scholarly_identifiers::identifiers::Identifier;
use serde_json::Value;
use sqlx::Postgres;
use sqlx::Transaction;
use std::time::Duration;
use tokio::time::sleep;

use crate::db::metadata::MetadataAssertionReason;
use crate::db::source::MetadataSourceId;
use crate::metadata_assertion::service::assert_metadata;

/// Attempt to fetch and store a metadata assertion for a DOI.
pub(crate) async fn try_collect_metadata_assertion<'a>(
    identifier: &scholarly_identifiers::identifiers::Identifier,
    pool: &sqlx::Pool<sqlx::Postgres>,
    tx: &mut Transaction<'a, Postgres>,
) -> Result<()> {
    if let Identifier::Doi {
        prefix: _,
        suffix: _,
    } = identifier
    {
        log::debug!("Try collect metadata for: {:?}", identifier);
        if let Some(url) = identifier.to_uri() {
            let request = || request_url(&url);
            match request
                .retry(
                    ConstantBuilder::default()
                        .with_max_times(2)
                        .with_delay(Duration::from_millis(500)),
                )
                .await
            {
                Ok(json) => {
                    assert_metadata(
                        identifier,
                        &json.to_string(),
                        MetadataSourceId::ContentNegotiation,
                        MetadataAssertionReason::Secondary,
                        pool,
                        tx,
                    )
                    .await?;
                    Ok(())
                }
                Err(err) => {
                    log::error!(
                        "Error retrieving content negotiation for DOI: {:?}: {:?}",
                        identifier,
                        err
                    );
                    Ok(())
                }
            }
        } else {
            // If it's not possible to build a URI for a DOI, that's an internal problem. Log and move on.
            // The metadata won't be asserted.
            log::error!("Failed to build URI for DOI {:?}", identifier);
            Ok(())
        }
    } else {
        Ok(())
    }
}

async fn request_url(url: &str) -> Result<Value> {
    log::debug!("Try {}", url);

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("Accept", "application/vnd.citationstyles.csl+json")
        .send()
        .await?;

    if response.status() != 200 {
        log::info!("Got {} from {:?}", response.status(), response.headers());
    }

    // Special case for slow down.
    if response.status() == 429 {
        log::error!("Slowing down!");
        sleep(Duration::from_secs(10)).await;
    }

    let text = response.text().await?;

    // Parse the response to ensure we got back valid JSON.
    let json = serde_json::from_str::<Value>(&text)?;

    Ok(json)
}
