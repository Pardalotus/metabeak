//! Model and database functions for metadata sources and analyzers.

/// Source for Metadata Assertions.
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum MetadataSourceId {
    Unknown = 0,
    Test = 1,
    Crossref = 2,
}

impl MetadataSourceId {
    pub(crate) fn from_str_value(value: &str) -> MetadataSourceId {
        match value {
            "crossref" => MetadataSourceId::Crossref,
            "test" => MetadataSourceId::Test,
            _ => MetadataSourceId::Unknown,
        }
    }

    pub(crate) fn from_int_value(value: i32) -> MetadataSourceId {
        match value {
            2 => MetadataSourceId::Crossref,
            1 => MetadataSourceId::Test,
            _ => MetadataSourceId::Unknown,
        }
    }

    pub(crate) fn to_str_value(self) -> String {
        String::from(match self {
            MetadataSourceId::Crossref => "crossref",
            MetadataSourceId::Test => "test",
            _ => "UNKNOWN",
        })
    }
}

#[cfg(test)]
mod metadata_source_tests {
    use super::*;

    #[test]
    fn roundtrip_metadatasource() {
        let inputs = ["crossref", "test"];
        for input in inputs.iter() {
            let from_str = MetadataSourceId::from_str_value(input);
            let as_str = from_str.to_str_value();
            let as_number = from_str as i32;
            let from_number = MetadataSourceId::from_int_value(as_number);

            assert_eq!(from_str, from_number);
            assert_eq!(&as_str, input);
        }
    }

    /// To cope with foreign keys shifting, or weird inputs, represent unknown values rather than fail.
    #[test]
    fn always_returns() {
        let result_str = MetadataSourceId::from_str_value("BLEURGH");
        assert_eq!(
            result_str,
            MetadataSourceId::Unknown,
            "Unknown string values return an 'unknown' value."
        );

        let result_num = MetadataSourceId::from_int_value(9999);
        assert_eq!(
            result_num,
            MetadataSourceId::Unknown,
            "Unknown string values return an 'unknown' value."
        );
    }
}

/// ID of an Event Analyzer, which is a function that produces events.
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum EventAnalyzerId {
    Unknown = 0,
    /// Test data, for internal and external testing.
    Test = 1,

    /// Lifecycle of metadata systems, such as indexing.
    Lifecycle = 2,

    /// Citations and references between artilces.
    Reference = 3,

    /// Contributions, e.g. authorship
    Contribution = 4,

    /// Links to other identifiers for a work
    Identifier = 5,
}

impl EventAnalyzerId {
    pub(crate) fn from_str_value(value: &str) -> EventAnalyzerId {
        match value {
            "lifecycle" => EventAnalyzerId::Lifecycle,
            "test" => EventAnalyzerId::Test,
            "reference" => EventAnalyzerId::Reference,
            "contribution" => EventAnalyzerId::Contribution,
            "identifier" => EventAnalyzerId::Identifier,
            _ => EventAnalyzerId::Unknown,
        }
    }

    pub(crate) fn to_str_value(self) -> String {
        String::from(match self {
            EventAnalyzerId::Lifecycle => "lifecycle",
            EventAnalyzerId::Test => "test",
            EventAnalyzerId::Reference => "reference",
            EventAnalyzerId::Contribution => "contribution",
            EventAnalyzerId::Identifier => "identifier",
            _ => "UNKNOWN",
        })
    }

    pub(crate) fn from_int_value(value: i32) -> EventAnalyzerId {
        match value {
            2 => EventAnalyzerId::Lifecycle,
            1 => EventAnalyzerId::Test,
            3 => EventAnalyzerId::Reference,
            4 => EventAnalyzerId::Contribution,
            5 => EventAnalyzerId::Identifier,
            _ => EventAnalyzerId::Unknown,
        }
    }
}

#[cfg(test)]
mod event_analyzer_id_tests {
    use super::*;

    #[test]
    fn roundtrip_event_analyzer_id() {
        let inputs = [
            "lifecycle",
            "test",
            "reference",
            "contribution",
            "identifier",
            "UNKNOWN",
        ];
        for input in inputs.iter() {
            let from_str = EventAnalyzerId::from_str_value(input);
            let as_str = from_str.to_str_value();
            let as_number = from_str as i32;
            let from_number = EventAnalyzerId::from_int_value(as_number);

            assert_eq!(from_str, from_number);
            assert_eq!(&as_str, input);
        }
    }

    /// To cope with foreign keys shifting, or weird inputs, represent unknown values rather than fail.
    #[test]
    fn always_returns() {
        let result_str = EventAnalyzerId::from_str_value("BLEURGH");
        assert_eq!(
            result_str,
            EventAnalyzerId::Unknown,
            "Unknown string values return an 'unknown' value."
        );

        let result_num = EventAnalyzerId::from_int_value(9999);
        assert_eq!(
            result_num,
            EventAnalyzerId::Unknown,
            "Unknown string values return an 'unknown' value."
        );
    }
}
