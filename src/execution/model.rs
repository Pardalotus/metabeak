//! Model for representing Handlers, and data going into and out of them.

use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::db::source::{EventAnalyzerId, MetadataSource};

// This is provided by Cargo at build time, so complied as a static string.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Environment passed into each function execution.
#[derive(Serialize, Deserialize)]
pub(crate) struct Global {
    environment: String,
    version: String,
}

impl Global {
    pub(crate) fn build() -> Global {
        Global {
            environment: String::from("Pardalotus Metabeak"),
            version: String::from(VERSION),
        }
    }

    pub(crate) fn json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

/// A handler function to be run.
#[derive(Debug, FromRow)]
pub(crate) struct HandlerSpec {
    /// ID of the handler, to allow collation of results.
    /// -1 for undefined (e.g. for testing)
    pub(crate) handler_id: i64,

    /// JavaScript code that must contain a function named 'f'.
    pub(crate) code: String,
}

/// Input data for a handler function run.
/// The analyzer and source fields are not stored in the `json` field.
#[derive(Debug)]
pub(crate) struct Event {
    pub(crate) event_id: i64,

    pub(crate) analyzer: EventAnalyzerId,

    pub(crate) source: MetadataSource,

    // Remainder of the JSON structure.
    // See DR-0012.
    pub(crate) json: String,
}

impl Event {
    // Serialize to a public JSON representation, with all fields present.
    pub(crate) fn to_json_value(&self) -> Option<String> {
        let analyzer_value = serde_json::Value::String(self.analyzer.to_str_value());
        let source_value = serde_json::Value::String(self.source.to_str_value());

        match serde_json::from_str::<serde_json::Value>(&self.json) {
            Ok(data) => match data {
                serde_json::Value::Object(mut data_obj) => {
                    data_obj["analyzer"] = analyzer_value;
                    data_obj["source"] = source_value;

                    if let Ok(json) = serde_json::to_string(&serde_json::Value::Object(data_obj)) {
                        Some(json)
                    } else {
                        // Highly unlikely.
                        log::error!("Failed to serialize JSON.");
                        None
                    }
                }
                _ => {
                    log::error!("Got unexpected type for JSON object: {}", &self.json);
                    None
                }
            },
            Err(e) => {
                log::error!(
                    "Failed to parse Event. Error: {:?}. Input: {}",
                    e,
                    &self.json
                );
                None
            }
        }
    }

    /// Load a JSON event from the public JSON representation.
    /// None if there was a problem parsing it.
    /// This clones subfields of the JSON Value, and is on a hot path. Candidate for optimisation if needed.
    pub(crate) fn from_json_value(input: &str) -> Option<Event> {
        match serde_json::from_str::<serde_json::Value>(input) {
            Ok(data) => match data {
                serde_json::Value::Object(data_obj) => {
                    let analyzer_str = data_obj.get("analyzer")?.as_str().unwrap_or("UNKNOWN");
                    let source_str = data_obj.get("source")?.as_str().unwrap_or("UNKNOWN");
                    let analyzer = EventAnalyzerId::from_str_value(analyzer_str);
                    let source = MetadataSource::from_str_value(source_str);

                    // Defaults to -1 (i.e. unassigned), so we can load events for insertion into the database.
                    // Events may be submitted without IDs, and
                    // they're assigned by the database on insertion.
                    let event_id: i64 = match data_obj.get("event_id") {
                        Some(value) => value.as_i64().unwrap_or(-1),
                        None => -1,
                    };

                    let mut normalized_event = serde_json::Map::new();
                    for field in data_obj.keys() {
                        if !(field.eq("analyzer") || field.eq("source")) {
                            if let Some(obj) = data_obj.get(field) {
                                normalized_event.insert(field.clone(), obj.clone());
                            }
                        }
                    }
                    if let Ok(json) =
                        serde_json::to_string(&serde_json::Value::Object(normalized_event))
                    {
                        Some(Event {
                            event_id,
                            analyzer,
                            source,
                            json,
                        })
                    } else {
                        // Highly unlikely.
                        log::error!("Failed to serialize JSON.");
                        None
                    }
                }
                _ => {
                    log::error!("Got unexpected type for JSON object: {}", input);
                    None
                }
            },
            Err(e) => {
                log::error!("Failed to parse Event. Error: {:?}. Input: {}", e, input);
                None
            }
        }
    }
}

/// Result from a handler function run.
/// A handler function returns an array of results. There will be one of these objects per entry.
#[derive(Debug)]
pub(crate) struct RunResult {
    /// ID of the handler function used.
    pub(crate) handler_id: i64,

    /// ID of the event it was triggered from.
    pub(crate) event_id: i64,

    /// Single JSON object.
    pub(crate) output: Option<String>,

    /// Error string, if execution failed.
    pub(crate) error: Option<String>,
}
