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

### 4. Checksum Per Entry

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

### 5. Log Metadata Header

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

### 6. Clear Durability Contract

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
