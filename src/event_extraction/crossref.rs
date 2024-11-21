use crate::db::metadata::MetadataQueueEntry;
use crate::db::source::{EventAnalyzerId, MetadataSourceId};
use crate::execution::model::Event;

pub(crate) fn extract_events(assertion: &MetadataQueueEntry) -> Vec<Event> {
    let mut results = vec![];

    if assertion.source_id == MetadataSourceId::Crossref as i32 {
        results.push(Event {
            event_id: -1,
            analyzer: EventAnalyzerId::Lifecycle,
            subject_id: Some(assertion.subject_id()),
            object_id: None,
            source: MetadataSourceId::from_int_value(assertion.source_id),
            json: serde_json::json!({"type": "indexed", "bytes": assertion.json.len()}).to_string(),
        });
    }

    results
}
