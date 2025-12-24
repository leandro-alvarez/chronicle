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

fn scan_log<P: AsRef<Path>>(path: P) -> Result<Vec<(u64, StoredEvent)>, io::Error> {
    let mut file = open_file_for_read(path)?;
    let mut entries = vec![];
    loop {
        let result = match read_event_at_offset(&mut file, None) {
            Ok(r) => r,
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                // Clean EOF at entry boundary
                return Ok(entries);
            }
            Err(err) => {
                println!("Error reading file {}", err);
                return Err(err);
            }
        };
        entries.push((result.offset, result.event));
    }
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
    let entries = scan_log(path)?;
    for (offset, stored_event) in entries {
        if let Some(id) = stored_event.event.aggregate_id {
            index.entry(id).or_default().push(offset);
        }
    }
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
                println!("Error reading file {}", err);
                return Err(err);
            }
        };
        println!("{:?} at offset {}", result.event, result.offset);
    }
}

#[cfg(test)]
mod tests {
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
    fn append_and_scan_log() {
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let e1 = mock_event(1);
        let e2 = mock_event(2);

        let off1 = append_event(path, &e1).unwrap();
        let off2 = append_event(path, &e2).unwrap();

        assert!(off2 > off1);

        let events = scan_log(path).unwrap();
        assert_eq!(events.len(), 2);

        assert_eq!(events[0].0, off1);
        assert_eq!(events[1].0, off2);
    }

    #[test]
    fn scan_empty_file_is_ok() {
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let events = scan_log(path).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn scan_log_ignores_trailing_partial_event() {
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

        // scan_log should return ONLY the valid events
        let events = scan_log(path).unwrap();

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].1.event.aggregate_id, Some(1));
        assert_eq!(events[1].1.event.aggregate_id, Some(1));
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
