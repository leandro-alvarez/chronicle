# Chronicle Roadmap

This document outlines the next features to implement, in priority order.

## Next Features

### 1. Sequential Replay API

Expose an iterator-based replay mechanism starting from any offset.

```rust
fn replay(from_offset: u64) -> impl Iterator<Item = Result<EventEnvelope, StorageError>>
```

**Why:** The current `scan_log` is private and loads all events into memory. An iterator allows streaming through large logs and is the foundation for all replay-based features.

**Notes:**
- Iterator should yield `Result` to handle mid-log corruption gracefully
- Likely requires a stateful `EventLog` struct to hold the file handle

---

### 2. Bounded Replay

Replay events until a maximum offset.

```rust
fn replay_until(from_offset: u64, until_offset: u64) -> impl Iterator<Item = Result<EventEnvelope, StorageError>>
```

**Why:** Allows consumers to replay a specific range of the log. Offset-based (not timestamp-based) because offsets are unambiguous physical positions — timestamps can be duplicated or out of order.

---

### 3. Persisted Index File

Save and load the aggregate index to disk instead of rebuilding on every startup.

```rust
fn save_index(path: &Path, index: &AggregateIndex) -> Result<(), StorageError>
fn load_index(path: &Path) -> Result<AggregateIndex, StorageError>
```

**Why:** Rebuilding the index by scanning the entire log doesn't scale for large logs.

**Notes:**
- Must detect index staleness: store the last known log offset in the index file
- On load, verify the log hasn't grown past the stored offset
- If stale, rebuild incrementally from the stored offset

---

### 4. Incremental Index Rebuild

Rebuild the index starting from a specific offset.

```rust
fn rebuild_index_from(offset: u64) -> Result<AggregateIndex, StorageError>
```

**Why:** Falls out naturally from the replay API and simplifies the incremental rebuild logic needed for persisted indexes. When loading a stale index, only replay from where it left off.

**Notes:**
- Uses `replay(from_offset)` internally
- Can be combined with an existing index via merge

---

### 5. Checksum Per Entry

Add CRC32 checksum to each event for corruption detection.

**Current format:**
```
[4 bytes: length][N bytes: JSON payload]
```

**New format:**
```
[4 bytes: length][N bytes: JSON payload][4 bytes: CRC32]
```

**Why:** Detect silent corruption (bit flips, partial writes). CRC32 is fast and sufficient for integrity (not security).

**Notes:**
- Checksum covers the payload only (or payload + length)
- On read, verify checksum and return error if mismatch

---

### 6. Log Metadata Header

Add a header at the start of each log file.

```rust
struct LogHeader {
    magic: [u8; 4],      // e.g., b"CHRN"
    version: u16,        // format version
    flags: u16,          // reserved for future use
    created_at: u64,     // timestamp
}
```

**Why:**
- Detect if a file is actually a Chronicle log
- Support format evolution without breaking existing logs
- Enable future features (compression, encryption flags)

**Notes:**
- Magic bytes let you fail fast on wrong file type
- Version field enables migration logic for old formats

---

### 7. Clear Durability Contract

Expose two append variants with explicit durability guarantees.

```rust
/// Appends event and flushes to OS buffer. Fast, but data may be lost on crash.
fn append_event(event: &EventEnvelope) -> Result<u64, StorageError>

/// Appends event and fsyncs to disk. Slower, but guarantees durability.
fn append_event_sync(event: &EventEnvelope) -> Result<u64, StorageError>
```

**Why:** Let callers choose between throughput and durability. Some use cases (WAL for critical data) need `fsync`. Others (high-throughput ingestion) can tolerate some loss.

---

## Future Considerations (Not Yet Prioritized)

These features may be added later but are not blocking:

| Feature | Description |
|---------|-------------|
| Timestamp index | Secondary index for time-range queries |
| Schema/namespace index | Secondary index for filtering by event type |
| Log segmentation | Split large logs into multiple files |
| Compaction | Remove old entries (for WAL use case) |
| Custom error types | Replace `io::Error` with `StorageError` enum |
| Builder pattern | Ergonomic event construction |

---

## Design Principles

### Offsets Are the Source of Truth

Offsets are the only reliable ordering mechanism. Timestamps can be duplicated, out of order, or clock-skewed. All replay and indexing is based on offsets.

**Aggregate index stores offsets only:**
```rust
pub type AggregateIndex = HashMap<u64, Vec<u64>>;  // aggregate_id -> offsets
```

No need to store timestamps in the index — the event payload contains it when read.

### Chronicle Controls `write_timestamp_ms`

The `write_timestamp_ms` field is set by Chronicle when the event is appended, not by the caller. This is enforced through separate input/output types.

| Field | Who controls | Purpose |
|-------|--------------|---------|
| `offset` | Chronicle | Physical position in log |
| `write_timestamp_ms` | Chronicle | When Chronicle accepted the event |
| `payload` | Caller | Business data (can include own timestamps) |

**Why:**
- Storage metadata belongs to the storage layer
- Callers can't be trusted to provide accurate/consistent timestamps
- Prevents out-of-order timestamps from misleading consumers
- Clear separation: Chronicle controls storage metadata, callers control payload

If a caller needs "when the event occurred" (business time), they put it in the payload.

**Implementation (DONE):**
```rust
/// Input: provided by caller
pub struct Event {
    pub event_type: String,
    pub namespace: String,
    pub schema_id: String,
    pub schema_version: u32,
    pub aggregate_id: Option<u64>,
    pub payload: Value,
}

/// Output: returned when reading from log
pub struct StoredEvent {
    pub write_timestamp_ms: u64,  // set by Chronicle
    pub event: Event,
}
```

---

## Implementation Notes

### Stateful Struct

Most features above imply a stateful struct to hold the file handle and index:

```rust
pub struct EventLog {
    file: File,
    path: PathBuf,
    index: AggregateIndex,
}

impl EventLog {
    pub fn open(path: &Path) -> Result<Self, StorageError>;
    pub fn append(&mut self, event: &EventEnvelope) -> Result<u64, StorageError>;
    pub fn append_sync(&mut self, event: &EventEnvelope) -> Result<u64, StorageError>;
    pub fn replay(&self, from: u64) -> impl Iterator<Item = Result<EventEnvelope, StorageError>>;
    pub fn load_aggregate(&self, id: u64) -> Result<Vec<EventEnvelope>, StorageError>;
}
```

### Index Staleness Detection

When persisting the index, store metadata:

```rust
struct PersistedIndex {
    log_end_offset: u64,       // last known log position
    log_checksum: u32,         // optional: checksum of last entry
    index: AggregateIndex,
}
```

On load:
1. Check if current log length > `log_end_offset`
2. If so, replay from `log_end_offset` to rebuild missing entries
3. Save updated index
