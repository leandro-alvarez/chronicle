# Storage Module Refactoring

This document outlines refactoring opportunities for `src/storage.rs`.

## Issues

### 1. No Stateful Struct

**Severity:** High | **Effort:** Medium

All functions are standalone and require passing the path on every call. This leads to:
- Reopening the file on every operation
- No way to hold state (like a cached index)

**Current:**
```rust
append_event("events.log", &event)?;
let index = rebuild_index("events.log")?;
let events = load_aggregate("events.log", 1, &index)?;
```

**Proposed:**
```rust
let log = EventLog::open("events.log")?;
log.append(&event)?;
let events = log.load_aggregate(1)?;
```

---

### 2. `println!` in Library Code

**Severity:** Medium | **Effort:** Low

**Locations:** lines 31, 66, 148, 152

Library code should not print to stdout. Callers should decide how to handle errors.

The `read_events` function (line 138) only prints events and doesn't return them, making it unusable as a library function.

**Fix:** Remove all `println!` calls. Either return errors to the caller or use a logging crate like `tracing` or `log`.

---

### 3. Visibility Issues

**Severity:** Medium | **Effort:** Low

- `AggregateIndex` (line 25) is private, but `rebuild_index` returns it and `load_aggregate` takes it as a parameter. Users cannot properly type their code.
- `IndexedEvent` is public but its fields (`timestamp`, `offset`) are private, making it unusable to consumers.

**Fix:** Either make `AggregateIndex` public or encapsulate it within a struct. Make `IndexedEvent` fields public or provide accessor methods.

---

### 4. Non-Idiomatic Rust Patterns

**Severity:** Low | **Effort:** Low

#### 4a. Use `entry` API (lines 125-130)

**Current:**
```rust
if index.contains_key(&id) {
    let offset_list = index.get_mut(&id).unwrap();
    offset_list.push(IndexedEvent { timestamp, offset });
} else {
    index.insert(id, vec![IndexedEvent { timestamp, offset }]);
}
```

**Proposed:**
```rust
index.entry(id).or_default().push(IndexedEvent { timestamp, offset });
```

#### 4b. Use `if let` instead of match with unit arm (line 123)

**Current:**
```rust
match event.aggregate_id {
    Some(id) => { ... }
    _ => ()
}
```

**Proposed:**
```rust
if let Some(id) = event.aggregate_id {
    ...
}
```

---

### 5. Durability: `flush` vs `sync_all`

**Severity:** Medium | **Effort:** Low

**Location:** line 89

Currently uses `flush()` which only flushes to the OS buffer. For true crash safety, `sync_all()` ensures data reaches the physical disk.

**Fix:** Replace `file.flush()` with `file.sync_all()` for durability guarantees.

---

### 6. Generic Error Type

**Severity:** Low | **Effort:** Medium

Using `io::Error` for everything (including JSON parse errors) loses error context.

**Proposed:**
```rust
#[derive(Debug)]
pub enum StorageError {
    Io(std::io::Error),
    Serialization(serde_json::Error),
}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        StorageError::Io(err)
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(err: serde_json::Error) -> Self {
        StorageError::Serialization(err)
    }
}
```

Alternatively, use the `thiserror` crate for less boilerplate.

---

### 7. No Iterator API

**Severity:** Low | **Effort:** Medium

`scan_log` loads all events into memory. For large logs, an iterator-based approach would be more memory-efficient.

**Proposed:**
```rust
pub struct EventIterator<'a> {
    file: &'a mut File,
}

impl Iterator for EventIterator<'_> {
    type Item = Result<EventEnvelope, StorageError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Read next event or return None at EOF
    }
}
```

---

## Summary

| Issue | Severity | Effort |
|-------|----------|--------|
| No stateful struct | High | Medium |
| `println!` in library | Medium | Low |
| Visibility (`AggregateIndex`, `IndexedEvent`) | Medium | Low |
| Non-idiomatic Rust | Low | Low |
| `flush` vs `sync_all` | Medium | Low |
| Generic `io::Error` | Low | Medium |
| No iterator API | Low | Medium |
