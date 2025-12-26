use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::event::{Event, StoredEvent};

/// Maps aggregate IDs to their event offsets in the log.
pub type AggregateIndex = HashMap<u64, Vec<u64>>;

struct ReadResult {
    offset: u64,
    event: StoredEvent,
}

fn open_file_for_read<P: AsRef<Path>>(path: P) -> Result<fs::File, io::Error> {
    OpenOptions::new().read(true).open(path)
}

/// Reads the event at the current file cursor if offset is None,
/// and advances the cursor to the start of the next entry.
fn read_event_at_offset(file: &mut File, offset: Option<u64>) -> Result<ReadResult, io::Error> {
    if let Some(off) = offset {
        file.seek(SeekFrom::Start(off))?;
    }

    let returned_offset = file.stream_position()?;
    let mut len_buf = [0; 4];
    file.read_exact(&mut len_buf)?;
    let length = u32::from_be_bytes(len_buf) as usize;
    let mut event_buf = vec![0u8; length];
    file.read_exact(&mut event_buf)?;

    Ok(ReadResult {
        offset: returned_offset,
        event: serde_json::from_slice(&event_buf)?,
    })
}

fn scan_log_entries<P: AsRef<Path>, F: FnMut(u64, StoredEvent)>(
    path: P,
    mut f: F,
) -> Result<(), io::Error> {
    let mut file = open_file_for_read(path)?;
    loop {
        let result = match read_event_at_offset(&mut file, None) {
            Ok(r) => r,
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                // Clean EOF at entry boundary
                break;
            }
            Err(err) => {
                return Err(err);
            }
        };
        f(result.offset, result.event);
    }
    Ok(())
}

pub fn append_event<P: AsRef<Path>>(path: P, event: &Event) -> std::io::Result<u64> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    // Record offset before writing
    let offset = file.seek(SeekFrom::End(0))?;

    // Chronicle sets the write timestamp
    let write_timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before 1970")
        .as_millis() as u64;

    let stored_event = StoredEvent {
        write_timestamp_ms,
        event: event.clone(),
    };

    let json = serde_json::to_vec(&stored_event)?;
    let len = json.len() as u32;

    file.write_all(&len.to_be_bytes())?;
    file.write_all(&json)?;
    file.flush()?;

    Ok(offset)
}

pub fn load_aggregate<P: AsRef<Path>>(
    path: P,
    aggregate_id: u64,
    index: &AggregateIndex,
) -> Result<Vec<StoredEvent>, io::Error> {
    let mut file = open_file_for_read(path)?;
    let mut results = vec![];

    let offsets = match index.get(&aggregate_id) {
        Some(list) => list,
        None => return Ok(vec![]),
    };

    for offset in offsets {
        let result = read_event_at_offset(&mut file, Some(*offset))?;
        results.push(result.event);
    }

    Ok(results)
}

pub fn rebuild_index<P: AsRef<Path>>(path: P) -> Result<AggregateIndex, io::Error> {
    let mut index = AggregateIndex::new();
    scan_log_entries(path, |offset, event| {
        if event.event.aggregate_id.is_some() {
            index
                .entry(event.event.aggregate_id.unwrap())
                .or_default()
                .push(offset);
        }
    })?;
    Ok(index)
}

pub fn read_events<P: AsRef<Path>>(path: P) -> Result<(), io::Error> {
    let mut file = open_file_for_read(path)?;
    loop {
        let result = match read_event_at_offset(&mut file, None) {
            Ok(r) => r,
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                // Clean EOF at entry boundary
                return Ok(());
            }
            Err(err) => {
                return Err(err);
            }
        };
        println!("{:?} at offset {}", result.event, result.offset);
    }
}

#[cfg(test)]
mod tests {
    use core::time;
    use std::thread::sleep;
    use tempfile::NamedTempFile;

    use serde_json::json;

    use super::*;

    fn mock_event(id: u64) -> Event {
        Event {
            aggregate_id: Some(id),
            schema_id: "Test".into(),
            schema_version: 1,
            namespace: "test".into(),
            event_type: "Test".into(),
            payload: json!({
                "name": "string",
                "email": "string"
            }),
        }
    }

    #[test]
    fn append() {
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let e1 = mock_event(1);
        let e2 = mock_event(2);

        let off1 = append_event(path, &e1).unwrap();
        let off2 = append_event(path, &e2).unwrap();

        assert!(off2 > off1);
    }

    #[test]
    fn scan_log_entries_with_empty_file_is_ok() {
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();
        let path = file.path();
        let mut seen = Vec::new();
        scan_log_entries(&path, |offset, event| {
            seen.push((offset, event));
        })
        .unwrap();
        assert!(seen.is_empty());
    }

    #[test]
    fn scan_log_entries_reads_all_events() {
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();
        let path = file.path();
        let ten_millis = time::Duration::from_millis(10);

        let _ = append_event(path, &mock_event(1));
        sleep(ten_millis); // We sleep so we can be sure timestamps are different
        let _ = append_event(path, &mock_event(1));
        sleep(ten_millis);
        let _ = append_event(path, &mock_event(2));

        let mut seen = Vec::new();

        scan_log_entries(&path, |offset, event| {
            seen.push((offset, event));
        })
        .unwrap();

        assert_eq!(seen.len(), 3);
        assert_eq!(
            seen[0].1.write_timestamp_ms < seen[1].1.write_timestamp_ms,
            true
        );
        assert_eq!(
            seen[1].1.write_timestamp_ms < seen[2].1.write_timestamp_ms,
            true
        );
        assert_eq!(seen[2].1.event.aggregate_id, Some(2));
    }

    #[test]
    fn scan_log_entries_rebuilds_aggregate_index() {
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let _ = append_event(path, &mock_event(1));
        let _ = append_event(path, &mock_event(1));
        let _ = append_event(path, &mock_event(2));

        let mut index: HashMap<u64, Vec<u64>> = HashMap::new();

        scan_log_entries(&path, |offset, event| {
            index
                .entry(event.event.aggregate_id.unwrap())
                .or_default()
                .push(offset);
        })
        .unwrap();

        assert_eq!(index[&1].len(), 2);
        assert_eq!(index[&2].len(), 1);
    }

    #[test]
    fn scan_log_entries_ignores_trailing_partial_event() {
        use std::fs::OpenOptions;
        use std::io::Write;
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        // Write two valid events
        let e1 = mock_event(1);
        let e2 = mock_event(1);

        append_event(path, &e1).unwrap();
        append_event(path, &e2).unwrap();

        // Manually corrupt the log:
        // write a length prefix but NOT the payload
        let mut f = OpenOptions::new().append(true).open(path).unwrap();
        let bogus_len: u32 = 9999;
        f.write_all(&bogus_len.to_be_bytes()).unwrap();
        f.flush().unwrap();

        // seen should contain ONLY the valid events
        let mut seen = Vec::new();

        scan_log_entries(&path, |offset, event| {
            seen.push((offset, event));
        })
        .unwrap();

        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].1.event.aggregate_id, Some(1));
        assert_eq!(seen[1].1.event.aggregate_id, Some(1));
    }

    #[test]
    fn scan_stops_cleanly_on_truncated_entry() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let _ = append_event(path, &mock_event(1));

        // Truncate mid-entry
        let file = OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(file.metadata().unwrap().len() - 3).unwrap();

        let mut count = 0;

        scan_log_entries(&path, |_, _| {
            count += 1;
        })
        .unwrap();

        assert_eq!(count, 0);
    }

    #[test]
    fn rebuilds_index_by_aggregate() {
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        append_event(path, &mock_event(1)).unwrap();
        append_event(path, &mock_event(1)).unwrap();
        append_event(path, &mock_event(2)).unwrap();

        let index = rebuild_index(path).unwrap();

        assert_eq!(index.get(&1).unwrap().len(), 2);
        assert_eq!(index.get(&2).unwrap().len(), 1);
    }

    #[test]
    fn loads_events_for_single_aggregate() {
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let e1 = mock_event(1);
        let e2 = mock_event(1);
        let e3 = mock_event(2);

        append_event(path, &e1).unwrap();
        append_event(path, &e2).unwrap();
        append_event(path, &e3).unwrap();

        let index = rebuild_index(path).unwrap();
        let events = load_aggregate(path, 1, &index).unwrap();

        assert_eq!(events.len(), 2);
    }

    #[test]
    fn load_aggregate_preserves_append_order() {
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let e1 = mock_event(1);
        let e2 = mock_event(1);
        let e3 = mock_event(1);

        append_event(path, &e1).unwrap();
        append_event(path, &e2).unwrap();
        append_event(path, &e3).unwrap();

        let index = rebuild_index(path).unwrap();
        let events = load_aggregate(path, 1, &index).unwrap();

        assert_eq!(events.len(), 3);

        // Timestamps should be in ascending order (Chronicle sets them on append)
        assert!(events[0].write_timestamp_ms <= events[1].write_timestamp_ms);
        assert!(events[1].write_timestamp_ms <= events[2].write_timestamp_ms);
    }

    #[test]
    fn rebuild_index_ignores_trailing_partial_event() {
        use std::fs::OpenOptions;
        use std::io::Write;
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        append_event(path, &mock_event(1)).unwrap();
        append_event(path, &mock_event(2)).unwrap();

        let mut f = OpenOptions::new().append(true).open(path).unwrap();
        f.write_all(&1234u32.to_be_bytes()).unwrap();

        let index = rebuild_index(path).unwrap();

        assert_eq!(index.get(&1).unwrap().len(), 1);
        assert_eq!(index.get(&2).unwrap().len(), 1);
    }
}
