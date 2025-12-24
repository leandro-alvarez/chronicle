use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

#[derive(Debug, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_type: String,
    pub namespace: String,
    pub schema_id: String,
    pub schema_version: u32,
    pub aggregate_id: Option<u64>,
    pub timestamp_ms: u64,
    pub payload: Value,
}

impl fmt::Display for EventEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "({}, {}, {}, {}, {}, {})",
            self.event_type,
            self.timestamp_ms,
            format!("{}{}", self.namespace, self.schema_id),
            self.schema_version,
            self.aggregate_id.unwrap(),
            self.payload
        )
    }
}
