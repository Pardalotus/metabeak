//! Client for Crossref API
use anyhow::Result;
use backon::Retryable;
use serde::Deserialize;
use std::sync::mpsc::Sender;
use std::time::Duration as SD;
use time::format_description;
use time::{Duration, OffsetDateTime};
use tokio::time::sleep;

use backon::ExponentialBuilder;

use crate::metadata_assertion::crossref::metadata::get_index_date;

const BASE: &str = "https://api.crossref.org/v1/works";

#[derive(Deserialize, Debug)]
struct CrossrefResponse {
    message: CrossrefResponseMessage,
}

#[derive(Deserialize, Debug)]
struct CrossrefResponseMessage {
    #[serde(alias = "total-results")]
    total_results: usize,

    #[serde(alias = "next-cursor")]
    next_cursor: String,

    // Leave the work model as an opaque structure, we're not concerned with the detailed internal schema.
    items: Vec<serde_json::Value>,
}

async fn request_url(url: &str) -> Result<CrossrefResponse> {
    log::debug!("Try {}", url);

    let response = reqwest::get(url).await?;

    if response.status() != 200 {
        log::info!(
            "Got {} from {}: {:?}",
            response.status(),
            url,
            response.headers()
        );
    }

    // Special case for slow down.
    if response.status() == 429 {
        log::error!("Slowing down!");
        sleep(SD::from_secs(10)).await;
    }

    let text = response.text().await?;

    // Parse the response to ensure we got back valid JSON.
    let deserialised = serde_json::from_str::<CrossrefResponse>(&text)?;

    Ok(deserialised)
}

/// Fetch historical data until the given [`not_before`] date.
/// Request sorted results, so we can stop paging when we hit the date.
/// Due to lack of secondary sort beyond date, it's sensible to add extra padding.
pub(crate) async fn fetch_from_indexed(
    rows: u32,
    cursor: &str,
    from_date: &str,
) -> Result<(Vec<serde_json::Value>, String)> {
    let url = format!(
        "{}?filter=from-index-date:{}&sort=indexed&order=desc&rows={}&cursor={}",
        BASE, from_date, rows, cursor
    );

    let request = || request_url(&url);
    let response = request.retry(ExponentialBuilder::default()).await?;

    // On first page log how many results might be present.
    if cursor == "*" {
        log::info!(
            "Fetching results since index date, total possible {} ",
            response.message.total_results
        );
    }

    Ok((response.message.items, response.message.next_cursor))
}

/// Fetch documents matching Crossref filter.
pub(crate) async fn fetch_with_filter(
    rows: u32,
    cursor: &str,
    filter: &str,
) -> Result<(Vec<serde_json::Value>, String)> {
    let url = format!("{}?filter={}&rows={}&cursor={}", BASE, filter, rows, cursor);

    let request = || request_url(&url);
    let response = request.retry(ExponentialBuilder::default()).await?;

    // On first page log how many results might be present.
    if cursor == "*" {
        log::info!(
            "Fetching results with filter {}, total possible {} ",
            filter,
            response.message.total_results
        );
    }

    Ok((response.message.items, response.message.next_cursor))
}

/// Harvest metadata indexed with Crossref since date-time to channel.
/// Stop at the precise date-time, plus some padding.
///
/// This is designed for doing continual live queries to the API. It doesn't
/// consume the entire result set, only those works that were indexed since the
/// given date-time.
pub(crate) async fn harvest_precise_index_date(
    chan: Sender<serde_json::Value>,
    after: OffsetDateTime,
) -> Result<()> {
    log::debug!("Harvest to channel");

    let rows = 1000;
    let mut cursor = String::from("*");
    let mut again = true;

    let ymd_format = format_description::parse("[year]-[month]-[day]").unwrap();

    // The API only deals in time intervals of one day, so we can't request the
    // specific cut-off time. Instead we need to truncate it to the day
    // boundary. Choose the start of the day before the requested cut-off. This
    // means we're not asking the API to sort the entire data set. It should be
    // sufficient to use the start of the current day, but adding an extra day
    // avoids a potential boundary condition. We won't retrieve that much data,
    // as we finish pagination when we hit the not_before date.
    let from_index_date = after
        .saturating_sub(Duration::DAY)
        .format(&ymd_format)
        .unwrap();

    while again {
        let result = fetch_from_indexed(rows, &cursor, &from_index_date).await;

        match result {
            Ok((items, new_cursor)) => {
                let num_items = items.len();

                // Stop when there are zero results, means we reached the end of the result set.
                if num_items == 0 {
                    again = false;
                }

                // Find those items indexed after the not_before date.
                let wanted_items: Vec<serde_json::Value> = items
                    .into_iter()
                    .filter(|item| {
                        if let Some(item_indexed) = get_index_date(item) {
                            item_indexed.gt(&after)
                        } else {
                            false
                        }
                    })
                    .collect();

                // Stop when there are no results after the not_before date. Results are ordered by the index date, so it's safe to stop here.
                // Only stop at the precise date when flag is set. Otherwise rely on the API to send whatever fits the query.
                if wanted_items.is_empty() {
                    again = false;
                }

                log::debug!(
                    "Page of {}, of which {} wanted",
                    num_items,
                    wanted_items.len(),
                );

                for item in wanted_items {
                    chan.send(item).unwrap();
                }
                cursor = new_cursor;
            }
            Err(e) => {
                log::error!("Error! {:?}", e);
                again = false;
            }
        }
    }

    Ok(())
}

/// Harvest metadata matching filter to channel.
pub(crate) async fn harvest_with_filter_to_chan(
    chan: Sender<serde_json::Value>,
    filter: String,
) -> Result<()> {
    log::debug!("Harvest to channel");

    let rows = 1000;
    let mut cursor = String::from("*");
    let mut again = true;

    while again {
        let result = fetch_with_filter(rows, &cursor, &filter).await;

        match result {
            Ok((items, new_cursor)) => {
                let num_items = items.len();

                // Stop when there are zero results, means we reached the end of the result set.
                if num_items == 0 {
                    again = false;
                }

                log::debug!("Page of {}.", num_items,);

                for item in items {
                    chan.send(item).unwrap();
                }
                cursor = new_cursor;
            }
            Err(e) => {
                log::error!("Error! {:?}", e);
                again = false;
            }
        }
    }

    Ok(())
}
