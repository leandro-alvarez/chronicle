use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// Event provided by the caller for storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event_type: String,
    pub namespace: String,
    pub schema_id: String,
    pub schema_version: u32,
    pub aggregate_id: Option<u64>,
    pub payload: Value,
}

/// Event with Chronicle-controlled metadata, returned when reading from the log.
#[derive(Debug, Serialize, Deserialize)]
pub struct StoredEvent {
    /// Timestamp set by Chronicle when the event was written.
    pub write_timestamp_ms: u64,
    /// The original event data.
    #[serde(flatten)]
    pub event: Event,
}

impl fmt::Display for StoredEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "({}, {}, {}{}, v{}, {:?}, {})",
            self.event.event_type,
            self.write_timestamp_ms,
            self.event.namespace,
            self.event.schema_id,
            self.event.schema_version,
            self.event.aggregate_id,
            self.event.payload
        )
    }
}
