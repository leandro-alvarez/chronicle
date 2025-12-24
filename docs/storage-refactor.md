# Storage Module Refactoring

Immediate code quality fixes for `src/storage.rs`. For feature work, see [roadmap.md](./roadmap.md).

---

## 1. Remove `println!` from Library Code

**Severity:** Medium | **Effort:** Low | **Status:** TODO

**Locations:** lines 20, 55, 125, 129

Library code should not print to stdout. Callers should decide how to handle errors.

The `read_events` function (line 115) only prints events and doesn't return them, making it unusable as a library function.

**Fix:** Remove all `println!` calls. Return errors to the caller. Consider removing `read_events` entirely or making it return `Vec<EventEnvelope>`.

---

## ~~2. Fix Visibility Issues~~

**Status:** DONE

- Made `AggregateIndex` public
- Removed `IndexedEvent` struct entirely
- Simplified to `pub type AggregateIndex = HashMap<u64, Vec<u64>>`

---

## 3. Use Idiomatic Rust Patterns

**Severity:** Low | **Effort:** Low

### ~~3a. Use `entry` API~~

**Status:** DONE (fixed during item 2)

### ~~3b. Use `if let` instead of match~~

**Status:** DONE (fixed during item 2)

### 3c. Simplify `open_file_for_read_or_fail` (lines 16-24)

**Status:** TODO

**Current:**
```rust
fn open_file_for_read_or_fail<P: AsRef<Path>>(path: P) -> Result<fs::File, io::Error> {
    match OpenOptions::new().read(true).open(path) {
        Ok(f) => return Ok(f),
        Err(err) => {
            println!("Error opening file {}", err);
            return Err(err);
        }
    };
}
```

**Proposed:**
```rust
fn open_file_for_read<P: AsRef<Path>>(path: P) -> Result<fs::File, io::Error> {
    OpenOptions::new().read(true).open(path)
}
```

---

## 4. Rename `timestamp_ms` to `write_timestamp_ms`

**Severity:** Medium | **Effort:** Low | **Status:** TODO

**File:** `src/event.rs`

The `timestamp_ms` field should be renamed to `write_timestamp_ms` and set by Chronicle on append, not by the caller.

See [roadmap.md](./roadmap.md#chronicle-controls-write_timestamp_ms) for rationale.

**Current:**
```rust
pub struct EventEnvelope {
    // ...
    pub timestamp_ms: u64,  // caller provides
    // ...
}
```

**Proposed:**
```rust
pub struct EventEnvelope {
    // ...
    pub write_timestamp_ms: u64,  // Chronicle sets on append
    // ...
}
```

This also requires updating `append_event` to set the timestamp instead of trusting the caller.

---

## Summary

| Issue | Status |
|-------|--------|
| Remove `println!` calls | TODO |
| Simplify `AggregateIndex` to offsets only | DONE |
| Idiomatic Rust patterns (entry, if let) | DONE |
| Simplify `open_file_for_read_or_fail` | TODO |
| Rename `timestamp_ms` to `write_timestamp_ms` | TODO |
