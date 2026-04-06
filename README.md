# File Watcher

A simple recursive directory watcher that reports file and directory changes.

## Project Description

### Requirements

FileWatcher requires the following environment variables:

- **`WATCH_DIR`** — directory to watch for changes.  
  **Important:** must be an absolute path for the system watcher to work correctly.

- **`INTERVAL_SEC`** — interval (in seconds) for periodic full scans.  
  Most changes are captured by the filesystem watcher, but in edge cases events may be missed.  
  Periodic scanning ensures correctness and can be configured to run less frequently (e.g. every 300 seconds).

### Project Trade-offs

#### No rename handling
Rename detection is intentionally not implemented.

Handling renames correctly introduces significant complexity (e.g. move detection, inode tracking, race conditions). Given time constraints, this was excluded.

#### No robust temp file filtering
Filesystem watchers do not provide a reliable way to distinguish temporary files (e.g. editor artifacts).

A simple debounce mechanism is used to reduce noise from short-lived files, but:
- there is no reliable system-level signal to identify temporary files
- some noise may still appear in events

#### Error handling

Two categories of errors are used:

##### 1. Non-breaking errors
- Logged to stderr via a simple sink
- Do not stop the system
- Could be improved with structured error handling and retry logic

##### 2. Breaking errors
- Trigger global shutdown using `CancellationToken`
- Examples:
  - core channels closed
  - filesystem becomes unreadable

In a production system, these cases would likely trigger retries instead of immediate shutdown.

#### At-most-once delivery

To avoid blocking the controller, `try_send` is used for channel communication.

This introduces **at-most-once semantics**, meaning events may be dropped:

1. **Watcher events dropped**  
   → recovered by periodic full scan

2. **Hasher events dropped**  
   → hash may become stale; updates with same size/mtime may be missed

3. **Sink queue full**  
   → events are dropped and not retried

Retry mechanisms were not implemented due to time constraints.

#### Controller design (`loop + select`)

The controller uses a `loop + tokio::select!` pattern.

Important considerations:

- All futures inside `select!` must be **cancellation-safe**
- Awaiting long operations directly inside `select!` can lead to partial progress being cancelled
- Therefore:
  - blocking work is offloaded (`spawn_blocking`)
  - async work is delegated to separate tasks

#### `select!` complexity

Due to time constraints, `select!` branches in the controller are relatively large.

In a production system, these would be:
- extracted into smaller functions
- better structured for readability and maintainability

#### Channel sizing

Channel capacities were chosen arbitrarily for simplicity.

In a real system, they should be tuned based on:
- workload characteristics
- filesystem size
- hardware (CPU, IO throughput)
- latency requirements

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

## AI Usage

### Problem Discovery & Design

**ChatGPT:**
- Used to validate the overall architecture (`loop + select + channels`).  
  The discussion was exploratory, but confirmed this is a reasonable design.
- Discussed using `try_send` in the controller to avoid blocking and confirmed this trade-off is acceptable.
- Suggested grouping controller parameters into structs to improve code clarity and reduce argument count.
- Helped identify the `notify` library as a cross-platform solution (after recalling platform-specific APIs like `inotify`).
- Provided additional context on filesystem watchers (`inotify`) and their limitations, which led to the decision to include periodic full scans.
- Assisted with `tokio::select!` patterns, including handling one-shot channels for full snapshot processing.
- Redact this README from my notes

### Code Review

**GitHub Copilot:**
- Used for PR reviews.
- Often provided generic feedback, but occasionally helped identify real issues in:
  - control flow
  - small logic mistakes
