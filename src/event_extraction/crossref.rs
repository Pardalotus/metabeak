use scholarly_identifiers::identifiers::{self, Identifier};

use crate::db::metadata::MetadataQueueEntry;
use crate::db::source::{EventAnalyzerId, MetadataSourceId};
use crate::execution::model::Event;

pub(crate) fn extract_events(
    assertion: &MetadataQueueEntry,
    maybe_json: Option<serde_json::Value>,
) -> Vec<Event> {
    let mut results = vec![];

    if assertion.source_id == MetadataSourceId::Crossref as i32 {
        results.push(Event {
            event_id: -1,
            analyzer: EventAnalyzerId::Lifecycle,
            subject_id: Some(assertion.subject_id()),
            object_id: None,
            source: MetadataSourceId::from_int_value(assertion.source_id),
            assertion_id: assertion.assertion_id,
            json: serde_json::json!({"type": "indexed"}).to_string(),
        });
    }

    orcid(&maybe_json, &mut results, assertion);
    isbn(&maybe_json, &mut results, assertion);

    results
}

fn orcid(
    maybe_json: &Option<serde_json::Value>,
    results: &mut Vec<Event>,
    assertion: &MetadataQueueEntry,
) {
    if let Some(json) = maybe_json {
        if let Some(authors) = json.get("author") {
            if let Some(authors) = authors.as_array() {
                for author in authors {
                    if let Some(orcid) = author.get("ORCID") {
                        if let Some(orcid) = orcid.as_str() {
                            let id = Identifier::parse(orcid);
                            results.push(Event {
                                event_id: -1,
                                analyzer: EventAnalyzerId::Contribution,
                                subject_id: Some(assertion.subject_id()),
                                object_id: Some(id),
                                source: MetadataSourceId::from_int_value(assertion.source_id),
                                assertion_id: assertion.assertion_id,
                                json: serde_json::json!({"type":"author"}).to_string(),
                            });
                        }
                    }
                }
            }
        }
    }
}

fn isbn(
    maybe_json: &Option<serde_json::Value>,
    results: &mut Vec<Event>,
    assertion: &MetadataQueueEntry,
) {
    if let Some(json) = maybe_json {
        if let Some(Some(isbn_types)) = json.get("isbn-type").map(serde_json::Value::as_array) {
            for isbn_type_entry in isbn_types {
                if let Some(isbn_type) = isbn_type_entry
                    .get("type")
                    .map(serde_json::Value::as_str)
                    .flatten()
                {
                    if let Some(isbn) = isbn_type_entry
                        .get(&"value")
                        .map(serde_json::Value::as_str)
                        .flatten()
                    {
                        let isbn_identifier = Identifier::parse(isbn);

                        results.push(Event {
                            event_id: -1,
                            analyzer: EventAnalyzerId::Identifier,
                            subject_id: Some(assertion.subject_id()),
                            object_id: Some(isbn_identifier),
                            source: MetadataSourceId::from_int_value(assertion.source_id),
                            assertion_id: assertion.assertion_id,
                            json: serde_json::json!({"type":"has-isbn", "isbn-type": isbn_type})
                                .to_string(),
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use crate::metadata_assertion::crossref::metadata_agent;

    use super::*;

    const ASSERTION_ID: i64 = 2;

    /// Simulate a MetadataQueueEntry coming off the queue, reading JSON from a local file.
    fn read_entry(path: &str, source_id: MetadataSourceId) -> MetadataQueueEntry {
        // In normal execution this is round-tripping through the database so it's reasonable to convert to string and back.
        let s = fs::read_to_string(&PathBuf::from(path)).unwrap();
        let json_val = serde_json::from_str(&s).unwrap();
        let (identifier, json) = metadata_agent::get_identifier_and_json(json_val).unwrap();
        let (subject_id_value, subject_id_type) = identifier.to_id_string_pair();

        MetadataQueueEntry {
            source_id: source_id as i32,
            assertion_id: ASSERTION_ID,
            json,
            subject_id_type: subject_id_type as i32,
            subject_id_value,
        }
    }

    #[test]
    fn test_article() {
        let entry = read_entry(
            "testing/unit/crossref-article.json",
            MetadataSourceId::Crossref,
        );
        let events = extract_events(&entry, Some(serde_json::from_str(&entry.json).unwrap()));

        // List of events and labels for debugging.
        let expected_events = vec![
            (
                "lifecycle",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Lifecycle,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: None,
                    assertion_id: 2,
                    json: String::from(r##"{"type":"indexed"}"##),
                },
            ),
            (
                "orcid-1",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Contribution,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Orcid(
                        String::from("0009-0005-5061-2894"),
                    )),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"author"}"##),
                },
            ),
            (
                "orcid-2",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Contribution,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Orcid(
                        String::from("0009-0009-8606-9140"),
                    )),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"author"}"##),
                },
            ),
            // This ORCID is invalid, and the checksum digit doesn't validate.
            // Event should be recorded using the URI type, not the ORCID type.
            (
                "orcid-invalid",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Contribution,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Uri(
                        String::from("http://orcid.org/0009-0009-8606-9149"),
                    )),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"author"}"##),
                },
            ),
        ];

        for (label, expected) in expected_events.iter() {
            assert!(
                events.contains(expected),
                "Expected to find '{}' event. Looking for {:?} in {:?}",
                label,
                expected,
                events
            );
        }
    }

    #[test]
    fn test_book() {
        let entry = read_entry(
            "testing/unit/crossref-book.json",
            MetadataSourceId::Crossref,
        );
        let events = extract_events(&entry, Some(serde_json::from_str(&entry.json).unwrap()));

        // List of events and labels for debugging.
        let expected_events = vec![
            (
                "lifecycle",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Lifecycle,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.1017"),
                        suffix: String::from("cbo9780511806223"),
                    }),
                    object_id: None,
                    assertion_id: 2,
                    json: String::from(r##"{"type":"indexed"}"##),
                },
            ),
            (
                "electronic isbn",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Identifier,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.1017"),
                        suffix: String::from("cbo9780511806223"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Isbn(
                        String::from("9780511806223"),
                    )),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"has-isbn","isbn-type":"electronic"}"##),
                },
            ),
            (
                "print isbn 1",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Identifier,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.1017"),
                        suffix: String::from("cbo9780511806223"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Isbn(
                        String::from("9780521643863"),
                    )),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"has-isbn","isbn-type":"print"}"##),
                },
            ),
            (
                "print isbn 2",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Identifier,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.1017"),
                        suffix: String::from("cbo9780511806223"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Isbn(
                        String::from("9780521643658"),
                    )),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"has-isbn","isbn-type":"print"}"##),
                },
            ),
            // Invalid checksum should mean identifier isn't recognised as an ISBN.
            (
                "bad isbn - checksum wrong",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Identifier,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.1017"),
                        suffix: String::from("cbo9780511806223"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Uri(
                        String::from("9780521643869"),
                    )),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"has-isbn","isbn-type":"print"}"##),
                },
            ),
        ];

        for (label, expected) in expected_events.iter() {
            assert!(
                events.contains(expected),
                "Expected to find '{}' event. Looking for {:?} in {:?}",
                label,
                expected,
                events
            );
        }
    }
}
