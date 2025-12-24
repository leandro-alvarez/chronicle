# What is Chronicle?

Chronicle is a lightweight, append-only event log written in Rust, designed to provide durable, replayable storage for event streams and rebuildable indexes as a low-level system primitive.

It persists events sequentially to disk in a simple, readable format and allows indexes to be rebuilt deterministically by replaying the log.

Chronicle is intended to be embedded inside systems that need durable event storage without relying on an external database, message broker, or background services.

# Core Ideas

## Append-only log

- Events are written sequentially.
- No in-place mutation of historical data.
- Optimized for durability and crash safety.

## Deterministic replay

- The log is the source of truth.
- All indexes can be rebuilt by replaying the log.
- System state can be reconstructed exactly.

## Explicit indexing

- Secondary indexes (by aggregate, timestamp, schema, etc.) map logical queries to byte offsets.

- Indexes are advisory and rebuildable, not authoritative.

## Minimal abstraction

- Chronicle does not manage consumers, offsets, replication, or schemas.
- It stores opaque event envelopes.
- Interpretation is delegated to higher layers.

## Embeddable by design

- Runs in-process.
- No network layer.
- Suitable for CLIs, agents, embedded systems, or internal infrastructure.

# What Chronicle Is Not

- Not a database
- Not a message broker (Kafka, Pulsar)
- Not a CDC platform (Debezium)
- Not an ORM or application framework
- Not opinionated about business schemas

Chronicle is a storage primitive, not an application component.

# Typical Use Cases

- Durable local event logs
- Write-ahead logs (WAL) for custom systems
- Embedded event sourcing
- Change capture before forwarding elsewhere
- Deterministic replay for testing and recovery
