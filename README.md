# What is Chronicle?

Chronicle is a lightweight, append-only event log and indexing engine written in Rust.

It is designed to persist domain events to disk in a simple, durable format while maintaining secondary indexes (such as timestamp and schema indexes) that allow efficient replay, querying, and recovery without relying on an external database or message broker.

Chronicle focuses on deterministic storage, fast sequential writes, and predictable reads, making it suitable as a foundational building block for event-driven systems, local change capture, or embedded event sourcing.

# Core Ideas

Append-only event log

Events are written sequentially to disk.

No in-place mutation of historical data.

Optimized for durability and crash safety.

Explicit on-disk indexing

Separate index files map logical queries (e.g. timestamps, schemas) to byte offsets in the log.

Indexes can be rebuilt by replaying the log.

Corruption can be detected and recovered from.

Minimal abstraction

Chronicle is not a queue, database, or streaming platform.

It does not manage consumers, offsets, or replication.

It provides primitives for writing, indexing, and reading events.

Embeddable by design

Runs in-process.

No network layer.

Suitable for CLIs, services, agents, or edge systems.

# What Chronicle Is Not

Not Kafka, Pulsar, or a message broker

Not Debezium or a CDC platform

Not a relational or document database

Not opinionated about schemas or serialization

Chronicle stores opaque event envelopes and lets higher layers decide how to interpret and consume them.

# Typical Use Cases

Event sourcing for small–to–medium systems

Durable audit logs

Local change capture before forwarding elsewhere

Reproducible system state via event replay

Testing and simulation of event streams

# Design Goals

Correctness over cleverness

Crash-safe by default

Readable on-disk format

Rebuildable indexes

No hidden background processes
