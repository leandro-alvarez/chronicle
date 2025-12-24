use serde_json::json;

use chronicle::event::Event;
use chronicle::storage::{append_event, load_aggregate, rebuild_index};

const SHOULD_WRITE: bool = false;
const PATH: &str = "accounts::Person.log";

fn main() -> std::io::Result<()> {
    let schema_event = Event {
        event_type: "SchemaDefined".into(),
        namespace: "accounts".into(),
        schema_id: "Person".into(),
        schema_version: 1,
        aggregate_id: None,
        payload: json!({
            "name": "string",
            "email": "string"
        }),
    };

    let event = Event {
        event_type: "Created".into(),
        namespace: "accounts".into(),
        schema_id: "Person".into(),
        schema_version: 1,
        aggregate_id: Some(1),
        payload: json!({
            "name": "Leandro",
        }),
    };

    let update_event = Event {
        event_type: "Updated".into(),
        namespace: "accounts".into(),
        schema_id: "Person".into(),
        schema_version: 1,
        aggregate_id: Some(1),
        payload: json!({
            "name": "Juan",
            "email": "l.alvarezindaburu@gmail.com"
        }),
    };

    if SHOULD_WRITE {
        let offset = append_event(PATH, &schema_event)?;
        println!("Event written at offset {}", offset);
        let offset = append_event(PATH, &event)?;
        println!("Event written at offset {}", offset);
        let offset = append_event(PATH, &update_event)?;
        println!("Event written at offset {}", offset);
    }

    let index = rebuild_index(PATH)?;
    println!("Index: {:?}", index);

    let whole = load_aggregate(PATH, 1, &index);
    println!("Whole agg with id 1: {:?}", whole?);
    Ok(())
}
