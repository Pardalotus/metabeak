//! Model for representing Handlers, and data going into and out of them.

use scholarly_identifiers::identifiers::Identifier;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::{
    db::source::{EventAnalyzerId, MetadataSourceId},
    util::VERSION,
};

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

    pub(crate) source: MetadataSourceId,

    // If there's a subject_id field, it's represented here.
    pub(crate) subject_id: Option<Identifier>,

    // If there's an object_id field, it's represented here.
    pub(crate) object_id: Option<Identifier>,

    // ID of the metadata assertion that generated this, or -1 if imported.
    pub(crate) assertion_id: i64,

    // Remainder of the JSON structure once the hydrated fields have been removed.
    // See DR-0012.
    pub(crate) json: String,
}

/// Map an Identifier Type to the value passed to the Handler.
fn identifier_type_string(identifier: &Identifier) -> serde_json::Value {
    serde_json::Value::String(String::from(match identifier {
        Identifier::Doi {
            prefix: _,
            suffix: _,
        } => "doi",
        Identifier::Orcid(_) => "orcid",
        Identifier::Ror(_) => "ror",
        Identifier::Uri(_) => "uri",
        Identifier::String(_) => "string",
        Identifier::Isbn(_) => "isbn",
    }))
}

/// Is this field meant to be hydrated, and therefore not stored in the database JSON.
fn is_hydrated_field(field: &str) -> bool {
    field.eq("analyzer")
        || field.eq("source")
        || field.eq("subject_id")
        || field.eq("subject_id_type")
        || field.eq("object_id")
        || field.eq("object_id_type")
}

impl Event {
    /// Serialize to a public JSON representation, hydrating some fields from database values.
    pub(crate) fn to_json_value(&self) -> Option<String> {
        let analyzer_value = serde_json::Value::String(self.analyzer.to_str_value());
        let source_value = serde_json::Value::String(self.source.to_str_value());

        match serde_json::from_str::<serde_json::Value>(&self.json) {
            Ok(data) => match data {
                serde_json::Value::Object(mut data_obj) => {
                    data_obj.insert(String::from("analyzer"), analyzer_value);
                    data_obj.insert(String::from("source"), source_value);

                    if let Some(ref identifier) = self.subject_id {
                        data_obj.insert(
                            String::from("subject_id"),
                            serde_json::Value::String(identifier.to_stable_string()),
                        );
                        data_obj.insert(
                            String::from("subject_id_type"),
                            identifier_type_string(identifier),
                        );

                        if let Some(uri) = identifier.to_uri() {
                            data_obj.insert(
                                String::from("subject_id_uri"),
                                serde_json::Value::String(uri),
                            );
                        }
                    }

                    if let Some(ref identifier) = self.object_id {
                        data_obj.insert(
                            String::from("object_id"),
                            serde_json::Value::String(identifier.to_stable_string()),
                        );
                        data_obj.insert(
                            String::from("object_id_type"),
                            identifier_type_string(identifier),
                        );

                        if let Some(uri) = identifier.to_uri() {
                            data_obj.insert(
                                String::from("object_id_uri"),
                                serde_json::Value::String(uri),
                            );
                        }
                    }

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
                    let source = MetadataSourceId::from_str_value(source_str);

                    // When ingested from an external source, we don't have the link back to the assertion id.
                    let assertion_id = -1;

                    // Defaults to -1 (i.e. unassigned), so we can load events for insertion into the database.
                    // Events may be submitted without IDs, and
                    // they're assigned by the database on insertion.
                    let event_id: i64 = match data_obj.get("event_id") {
                        Some(value) => value.as_i64().unwrap_or(-1),
                        None => -1,
                    };

                    let subject_id = if let Some(val) = data_obj.get("subject_id") {
                        val.as_str().map(|id_str| Identifier::parse(id_str))
                    } else {
                        None
                    };

                    let object_id = if let Some(val) = data_obj.get("object_id") {
                        val.as_str().map(|id_str| Identifier::parse(id_str))
                    } else {
                        None
                    };

                    let mut normalized_event = serde_json::Map::new();
                    for field in data_obj.keys() {
                        if is_hydrated_field(&field) {
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
                            subject_id,
                            object_id,
                            assertion_id,
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
#[derive(Debug, PartialEq)]
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
