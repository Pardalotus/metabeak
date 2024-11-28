use scholarly_identifiers::identifiers::Identifier;

use crate::db::metadata::MetadataQueueEntry;
use crate::db::source::{EventAnalyzerId, MetadataSourceId};
use crate::execution::model::Event;

pub(crate) fn extract_events(
    assertion: &MetadataQueueEntry,
    maybe_json: Option<serde_json::Value>,
) -> Vec<Event> {
    let mut results = vec![];

    if assertion.source_id == MetadataSourceId::Crossref as i32 {
        if let Some(json) = maybe_json {
            lifecycle(&mut results, assertion);
            orcid(&json, &mut results, assertion);
            isbn(&json, &mut results, assertion);
            references(&json, &mut results, assertion);
        }
    }
    results
}

fn lifecycle(results: &mut Vec<Event>, assertion: &MetadataQueueEntry) {
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

fn orcid(json: &serde_json::Value, results: &mut Vec<Event>, assertion: &MetadataQueueEntry) {
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

fn isbn(json: &serde_json::Value, results: &mut Vec<Event>, assertion: &MetadataQueueEntry) {
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

fn references(json: &serde_json::Value, results: &mut Vec<Event>, assertion: &MetadataQueueEntry) {
    if let Some(references) = json.get("reference") {
        if let Some(references) = references.as_array() {
            for reference in references {
                // If there's no DOI it's unlinked, and should be skipped.
                if let Some(doi) = reference.get("DOI") {
                    if let Some(doi) = doi.as_str() {
                        let id = Identifier::parse(doi);
                        results.push(Event {
                            event_id: -1,
                            analyzer: EventAnalyzerId::Reference,
                            subject_id: Some(assertion.subject_id()),
                            object_id: Some(id),
                            source: MetadataSourceId::from_int_value(assertion.source_id),
                            assertion_id: assertion.assertion_id,
                            json: serde_json::json!({"type":"references"}).to_string(),
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

    fn assert_contains_events(expected_events: Vec<(&str, Event)>, events: Vec<Event>) {
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
    fn test_contribution() {
        let entry = read_entry(
            "testing/unit/crossref-article.json",
            MetadataSourceId::Crossref,
        );
        let events = extract_events(&entry, Some(serde_json::from_str(&entry.json).unwrap()));

        // List of events and labels for debugging.
        let expected_events = vec![
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

        assert_contains_events(expected_events, events);
    }

    #[test]
    fn test_lifecycle() {
        let article = read_entry(
            "testing/unit/crossref-article.json",
            MetadataSourceId::Crossref,
        );
        let article_events =
            extract_events(&article, Some(serde_json::from_str(&article.json).unwrap()));

        let expected_article_events = vec![(
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
        )];

        assert_contains_events(expected_article_events, article_events);

        let book = read_entry(
            "testing/unit/crossref-book.json",
            MetadataSourceId::Crossref,
        );
        let book_events = extract_events(&book, Some(serde_json::from_str(&book.json).unwrap()));

        // List of events and labels for debugging.
        let expected_book_events = vec![(
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
        )];

        assert_contains_events(expected_book_events, book_events);
    }

    #[test]
    fn test_isbn() {
        let entry = read_entry(
            "testing/unit/crossref-book.json",
            MetadataSourceId::Crossref,
        );
        let events = extract_events(&entry, Some(serde_json::from_str(&entry.json).unwrap()));

        // List of events and labels for debugging.
        let expected_events = vec![
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

        assert_contains_events(expected_events, events);
    }

    /// All linked references. No unlinked ones.
    #[test]
    fn test_references() {
        let entry = read_entry(
            "testing/unit/crossref-article.json",
            MetadataSourceId::Crossref,
        );
        let events = extract_events(&entry, Some(serde_json::from_str(&entry.json).unwrap()));

        let expected_events = vec![
            (
                "ref-1",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Reference,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.35381"),
                        suffix: String::from("r.k.v5i5.1052"),
                    }),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"references"}"##),
                },
            ),
            (
                "ref-2",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Reference,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.15517"),
                        suffix: String::from("revedu.v45i1.41009"),
                    }),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"references"}"##),
                },
            ),
            (
                "ref-3",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Reference,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.3390"),
                        suffix: String::from("educsci12030191"),
                    }),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"references"}"##),
                },
            ),
            (
                "ref-4",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Reference,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.37811"),
                        suffix: String::from("cl_rcm.v7i4.7011"),
                    }),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"references"}"##),
                },
            ),
            (
                "ref-5",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Reference,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i3.3178"),
                    }),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"references"}"##),
                },
            ),
            (
                "ref-6",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Reference,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.48082"),
                        suffix: String::from("espacios-a21v42n08p04"),
                    }),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"references"}"##),
                },
            ),
            (
                "ref-7",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Reference,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.2307"),
                        suffix: String::from("j.ctv2wk71sb"),
                    }),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"references"}"##),
                },
            ),
            (
                "ref-8",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Reference,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.47422"),
                        suffix: String::from("fepol.3"),
                    }),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"references"}"##),
                },
            ),
            (
                "ref-9",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Reference,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.1007"),
                        suffix: String::from("s10639-023-11723-7"),
                    }),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"references"}"##),
                },
            ),
            (
                "ref-10",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Reference,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.3390"),
                        suffix: String::from("educsci14040367"),
                    }),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"references"}"##),
                },
            ),
            (
                "ref-11",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Reference,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.3390"),
                        suffix: String::from("educsci12030179"),
                    }),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"references"}"##),
                },
            ),
            (
                "ref-12",
                Event {
                    event_id: -1,
                    analyzer: EventAnalyzerId::Reference,
                    source: MetadataSourceId::Crossref,
                    subject_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("exploradordigital.v8i4.3221"),
                    }),
                    object_id: Some(scholarly_identifiers::identifiers::Identifier::Doi {
                        prefix: String::from("10.33262"),
                        suffix: String::from("ap.v6i1.1.463"),
                    }),
                    assertion_id: 2,
                    json: String::from(r##"{"type":"references"}"##),
                },
            ),
        ];

        assert_contains_events(expected_events, events);
    }
}
