use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::event::EventEnvelope;

pub struct IndexedEvent {
    timestamp: u64,
    offset: u64,
}

impl Debug for IndexedEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.timestamp, self.offset)
    }
}

struct ReadEvent {
    offset: u64,
    event: EventEnvelope,
}

type AggregateIndex = HashMap<u64, Vec<IndexedEvent>>;

fn open_file_for_read_or_fail<P: AsRef<Path>>(path: P) -> Result<fs::File, io::Error> {
    match OpenOptions::new().read(true).open(path) {
        Ok(f) => return Ok(f),
        Err(err) => {
            println!("Error opening file {}", err);
            return Err(err);
        }
    };
}

fn read_event_at_offset(file: &mut File, offset: Option<u64>) -> Result<ReadEvent, io::Error> {
    if let Some(off) = offset {
        file.seek(SeekFrom::Start(off))?;
    }

    let returned_offset = file.stream_position()?;
    let mut len_buf = [0; 4];
    file.read_exact(&mut len_buf)?;
    let length = u32::from_be_bytes(len_buf) as usize;
    let mut event_buf = vec![0u8; length];
    file.read_exact(&mut event_buf)?;

    Ok(ReadEvent {
        offset: returned_offset,
        event: serde_json::from_slice(&event_buf)?,
    })
}

fn scan_log<P: AsRef<Path>>(path: P) -> Result<Vec<(u64, u64, EventEnvelope)>, io::Error> {
    let mut file = open_file_for_read_or_fail(path)?;
    let mut offset_events_vector = vec![];
    loop {
        let read_event = match read_event_at_offset(&mut file, None) {
            Ok(re) => re,
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                // Clean EOF at entry boundary
                return Ok(offset_events_vector);
            }
            Err(err) => {
                println!("Error reading file {}", err);
                return Err(err);
            }
        };
        offset_events_vector.push((
            read_event.event.timestamp_ms,
            read_event.offset,
            read_event.event,
        ));
    }
}

pub fn append_event<P: AsRef<Path>>(path: P, event: &EventEnvelope) -> std::io::Result<u64> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    // Record offset before writing
    let offset = file.seek(SeekFrom::End(0))?;

    let json = serde_json::to_vec(event)?;
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
) -> Result<Vec<EventEnvelope>, io::Error> {
    let mut file = open_file_for_read_or_fail(path)?;
    let mut results = vec![];

    let offset_list = match index.get(&aggregate_id) {
        Some(list) => list,
        None => return Ok(vec![]),
    };

    for IndexedEvent {
        timestamp: _timestamp,
        offset,
    } in offset_list
    {
        let read_event = read_event_at_offset(&mut file, Some(*offset))?;
        results.push(read_event.event);
    }

    Ok(results)
}

pub fn rebuild_index<P: AsRef<Path>>(path: P) -> Result<AggregateIndex, io::Error> {
    let mut index = AggregateIndex::new();
    let timestamp_offset_list = scan_log(path)?;
    for (timestamp, offset, event) in timestamp_offset_list {
        match event.aggregate_id {
            Some(id) => {
                if index.contains_key(&id) {
                    let offset_list = index.get_mut(&id).unwrap();
                    offset_list.push(IndexedEvent { timestamp, offset });
                } else {
                    index.insert(id, vec![IndexedEvent { timestamp, offset }]);
                };
            }
            _ => (),
        }
    }
    Ok(index)
}

pub fn read_events<P: AsRef<Path>>(path: P) -> Result<(), io::Error> {
    let mut file = open_file_for_read_or_fail(path)?;
    loop {
        let read_event = match read_event_at_offset(&mut file, None) {
            Ok(re) => re,
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                // Clean EOF at entry boundary
                return Ok(());
            }
            Err(err) => {
                println!("Error reading file {}", err);
                return Err(err);
            }
        };
        println!("{:?} at offset {}", read_event.event, read_event.offset);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn mock_event(id: u64) -> EventEnvelope {
        let ts = chrono::Utc::now()
            .timestamp_millis()
            .try_into()
            .expect("system clock before 1970");

        EventEnvelope {
            aggregate_id: Some(id),
            schema_id: "Test".into(),
            schema_version: 1,
            namespace: "test".into(),
            timestamp_ms: ts,
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

        assert_eq!(events[0].1, off1);
        assert_eq!(events[1].1, off2);
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
        assert_eq!(events[0].2.aggregate_id, Some(1));
        assert_eq!(events[1].2.aggregate_id, Some(1));
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
    fn load_aggregate_preserves_order() {
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        // Create events with explicit timestamps
        let mut e1 = mock_event(1);
        let mut e2 = mock_event(1);
        let mut e3 = mock_event(1);

        e1.timestamp_ms = 100;
        e2.timestamp_ms = 50; // earlier timestamp, but appended later
        e3.timestamp_ms = 150;

        append_event(path, &e1).unwrap();
        append_event(path, &e2).unwrap();
        append_event(path, &e3).unwrap();

        let index = rebuild_index(path).unwrap();
        let events = load_aggregate(path, 1, &index).unwrap();

        assert_eq!(events.len(), 3);

        // Order must match append order, not timestamp order
        assert_eq!(events[0].timestamp_ms, 100);
        assert_eq!(events[1].timestamp_ms, 50);
        assert_eq!(events[2].timestamp_ms, 150);
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
