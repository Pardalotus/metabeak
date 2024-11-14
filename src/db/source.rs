//! Model and database functions for metadata sources and analyzers.

#[derive(Debug, Copy, Clone)]
pub(crate) enum MetadataSource {
    Unknown = 0,
    Test = 1,
    Crossref = 2,
}

impl MetadataSource {
    pub(crate) fn from_str_value(value: &str) -> MetadataSource {
        match value {
            "crossref" => MetadataSource::Crossref,
            "test" => MetadataSource::Test,
            _ => MetadataSource::Unknown,
        }
    }

    pub(crate) fn from_int_value(value: i32) -> MetadataSource {
        match value {
            2 => MetadataSource::Crossref,
            1 => MetadataSource::Test,
            _ => MetadataSource::Unknown,
        }
    }

    pub(crate) fn to_str_value(&self) -> String {
        String::from(match self {
            MetadataSource::Crossref => "crossref",
            MetadataSource::Test => "test",
            _ => "UNKNOWN",
        })
    }

    pub(crate) fn to_int_value(&self) -> i32 {
        match self {
            MetadataSource::Crossref => 2,
            MetadataSource::Test => 1,
            _ => 0,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum EventAnalyzerId {
    Unknown = 0,
    Test = 1,
    Lifecycle = 2,
}

impl EventAnalyzerId {
    pub(crate) fn from_str_value(value: &str) -> EventAnalyzerId {
        match value {
            "lifecycle" => EventAnalyzerId::Lifecycle,
            "test" => EventAnalyzerId::Test,
            _ => EventAnalyzerId::Unknown,
        }
    }

    pub(crate) fn to_str_value(&self) -> String {
        String::from(match self {
            EventAnalyzerId::Lifecycle => "lifecycle",
            EventAnalyzerId::Test => "test",
            _ => "UNKNOWN",
        })
    }

    pub(crate) fn from_int_value(value: i32) -> EventAnalyzerId {
        match value {
            2 => EventAnalyzerId::Lifecycle,
            1 => EventAnalyzerId::Test,
            _ => EventAnalyzerId::Unknown,
        }
    }

    pub(crate) fn to_int_value(self) -> i32 {
        match self {
            EventAnalyzerId::Lifecycle => 2,
            EventAnalyzerId::Test => 1,
            _ => 0,
        }
    }
}
