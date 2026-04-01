# File Watcher

A simple recursive directory watcher that reports file and directory changes.

## Solution Design
### Problem Breakdown
1. Initial parsing & periodic reconciliation
The entire directory tree must be parsed to build an initial snapshot.
Additionally, periodic full scans are required because filesystem watchers (e.g. inotify via notify) may miss events in edge cases.

2. Change detection (diffing)
Changes can be detected by comparing snapshots or by applying incremental updates.
- Cheap checks: path, size, mtime
- Expensive check: file hash (used only when metadata indicates possible change)

3. Hashing
Hashing large files is expensive and blocking.
- Must be executed in spawn_blocking
- Offloaded to a worker pool
- Triggered only for “suspicious” files (metadata changed)

4. Event delivery
Changes should be reported to a generic sink.
- Goal: at-most-once delivery
- Controller should not block on sink processing
- Events are sent through a queue to a dedicated sink worker

## Design Options
### Design 1 — Periodic Scan Only
- Periodically parse full directory tree
- Diff snapshots
- Emit delta to sink
Pros:
- Simple
Cons:
- High latency
- Inefficient for large trees

### Design 2 — Controller + Workers
- Controller owns state (HashMap<Path, Metadata>)
- Parser and hash workers communicate via queues
- Controller computes diffs and emits events
Pros:
- Centralized state management
- Clear ownership
Cons:
- Still relies heavily on full scans
### Design 3 — Hybrid (Final)
Components
1. Controller (core)
- Owns authoritative state
- Processes all updates
- Emits events to sink
2. File watcher
- Uses notify with `RecursiveMode::Recursive`
- Sends filesystem events to controller via `mpsc`
- Events treated as hints, not source of truth
3. Hash worker pool
- Receives hash jobs from controller
- Computes hashes asynchronously (`spawn_blocking`)
- Sends results back to controller
4. Periodic parser
- Runs in background (e.g. every 60s)
- Builds full snapshot
- Sends snapshot to controller (via oneshot)
- Controller performs diff with current state

---

#### Controller Responsibilities
- Handle watcher events:
    - mark paths as dirty
    - detect deletes
    - schedule hashing if needed
- Handle hash results:
    - verify version (avoid stale updates)
    - update state
    - emit Updated if hash changed
- Handle periodic snapshots:
    - diff old vs new snapshot
    - emit batch of `Created`, `Updated`, `Deleted`
    - replace state
- Dispatch events to sink:
    - via bounded `mpsc`
    - non-blocking to ensure at-most-once

## Key Design Decisions
- Single state owner (controller) → no shared mutable state, no mutex needed
- Watcher + periodic scan → balance between performance and correctness
- Metadata-first diff + conditional hashing → efficient change detection
- Bounded queues → backpressure control
- At-most-once delivery → simplicity over guarantees

## Summary

The final design is a hybrid event-driven + reconciliation system:

- Watcher provides fast incremental updates
- Periodic scans ensure correctness
- Controller maintains consistent state and emits changes
- Hashing is offloaded and performed only when necessary
