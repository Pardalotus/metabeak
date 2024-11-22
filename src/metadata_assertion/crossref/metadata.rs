//! Functions for working with Crossref metadata.

use time::{format_description::well_known::Iso8601, OffsetDateTime};

/// Get the indexed date for the work, if present and valid.
pub(crate) fn get_index_date(item: &serde_json::Value) -> Option<OffsetDateTime> {
    if let Some(value) = &item["indexed"]["date-time"].as_str() {
        match OffsetDateTime::parse(value, &Iso8601::DEFAULT) {
            Ok(time) => Some(time),
            _ => None,
        }
    } else {
        None
    }
}
