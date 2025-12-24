# Storage Module Refactoring

Immediate code quality fixes for `src/storage.rs`. For feature work, see [roadmap.md](./roadmap.md).

---

## 1. Remove `println!` from Library Code

**Severity:** Medium | **Effort:** Low | **Status:** TODO

**Locations:** lines 49, 115, 119

Library code should not print to stdout. Callers should decide how to handle errors.

The `read_events` function (line 105) only prints events and doesn't return them, making it unusable as a library function.

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

### ~~3c. Simplify `open_file_for_read_or_fail`~~

**Status:** DONE

Renamed to `open_file_for_read` and simplified to one-liner.

---

## ~~4. Rename `timestamp_ms` to `write_timestamp_ms`~~

**Status:** DONE

Split into two structs:
- `Event` — input struct provided by caller (no timestamp)
- `StoredEvent` — output struct with `write_timestamp_ms` set by Chronicle

Chronicle now sets the timestamp on append, not the caller.

---

## Summary

| Issue | Status |
|-------|--------|
| Remove `println!` calls | TODO |
| Simplify `AggregateIndex` to offsets only | DONE |
| Idiomatic Rust patterns (entry, if let) | DONE |
| Simplify `open_file_for_read` | DONE |
| Split Event/StoredEvent, Chronicle sets timestamp | DONE |
