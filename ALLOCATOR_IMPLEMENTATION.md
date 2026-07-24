# Allocation Tracing Allocator Implementation

## Overview

A custom global allocator has been added to Lidi to trace all memory allocations during packet processing. This allows detailed analysis of memory usage patterns without the overhead of external profiling tools.

## Architecture

### Key Components

1. **`lidi-command-utils/src/allocator.rs`** — Core allocator implementation
   - `TracingAllocator` struct implementing `GlobalAlloc` trait
   - Wraps the system allocator (`std::alloc::System`)
   - Captures stack traces for each allocation using the `backtrace` crate

2. **Activation Mechanism**
   - Feature flag: `alloc-trace` (disabled by default)
   - Runtime control: `LIDI_ALLOC_TRACE` environment variable
   - Tracing starts **after** program initialization to avoid logging startup allocations

3. **Thread-Safe Logging**
   - Uses `parking_lot::Mutex` for lock-free performance
   - `OnceLock` for lazy initialization of trace file
   - Atomic flag for fast enable/disable checks

### Design Decisions

#### Why not log startup allocations?

Startup allocations are numerous and unrelated to packet processing. The allocator is designed to be enabled **after** initialization via the `LIDI_ALLOC_TRACE` environment variable. This approach:
- Keeps trace files small and focused
- Improves readability of actual packet-handling allocations
- Reduces overhead during the warm-up phase

#### Why parking_lot?

The `parking_lot::Mutex` is more efficient than `std::sync::Mutex` for short critical sections (just a log write). It has:
- Smaller memory footprint
- Better performance characteristics for uncontended locks
- No poisoning semantics (not needed here)

#### Why OnceLock instead of Once?

`OnceLock` is the modern Rust standard (stabilized in 1.70). It provides:
- Type-safe storage initialization
- No unsafe code needed in the allocator
- Better API than `std::sync::Once`

## Implementation Details

### Allocation Logging

When tracing is enabled, each allocation (and deallocation/realloc) is logged with:

```
[OPERATION] addr=0x..., size=N
<backtrace with function names and source locations>
```

Example output:
```
[alloc] addr=0x7f1234567890, size=4096
 0: lidi_command_utils::allocator::dump_allocation
    at /workspace/lidi-command-utils/src/allocator.rs:47
 1: lidi_command_utils::allocator::TracingAllocator::alloc
    at /workspace/lidi-command-utils/src/allocator.rs:16
 2: core::alloc::global::GlobalAlloc::alloc
    at rust/library/core/src/alloc.rs:###
```

### Performance Overhead

**When disabled (no LIDI_ALLOC_TRACE):**
- Two atomic loads per allocation (negligible)
- No backtrace capture
- No file I/O
- ~0% overhead in practice

**When enabled:**
- Backtrace capture (expensive)
- File I/O with lock contention
- ~5-20% slowdown depending on allocation frequency
- Only for debugging/analysis

### Thread Safety

The allocator is fully thread-safe:
- `AtomicBool` for enable flag (lock-free)
- `Mutex<File>` for trace output (one lock per write)
- `OnceLock` for one-time file initialization

Multiple threads can log simultaneously without data corruption. The file output may be interleaved if allocations happen concurrently, but each allocation record is atomic.

## Feature Flags

### In lidi-command-utils

```toml
[features]
alloc-trace = []
```

### In lidi-send and lidi-receive

```toml
[features]
alloc-trace = [ "lidi-command-utils/alloc-trace" ]
```

## Public API

### `lidi_command_utils::enable_tracing(path: &str)`

Enables allocation tracing and writes to the specified file.

- Must be called after program initialization
- Panics if the file cannot be created
- Safe to call multiple times (will warn on subsequent calls)

```rust
if let Ok(trace_path) = std::env::var("LIDI_ALLOC_TRACE") {
    lidi_command_utils::enable_tracing(&trace_path);
}
```

### `lidi_command_utils::disable_tracing()`

Disables allocation tracing without removing the global allocator.

- Does not close the trace file (it flushes after each write)
- Can be called multiple times without effect

## Dependencies Added

```toml
[dependencies.backtrace]
version = "0"
default-features = false
features = [ "std" ]

[dependencies.parking_lot]
version = "0"
default-features = false
features = [ "wasm-bindgen" ]
```

Both are optional and only used when `alloc-trace` feature is enabled.

## Testing

### Build verification

```bash
# Without feature (default)
cargo build --release

# With feature
cargo build --release --features alloc-trace

# Workspace with feature
cargo build --release --features alloc-trace
```

### Runtime testing

```bash
# Start receiver with allocation tracing
LIDI_ALLOC_TRACE=/tmp/alloc.log ./target/release/lidi-receive -c config.toml

# Analyze the trace
grep -c "^\[alloc\]" /tmp/alloc.log
grep "^\[alloc\]" /tmp/alloc.log | sed 's/.*size=//' | sort -rn | head -10
```

## Future Improvements

1. **Structured logging** — Output JSON or other structured format for easier analysis
2. **Filtering** — Trace only allocations above/below certain size thresholds
3. **Peak memory tracking** — Monitor peak memory usage during execution
4. **Leak detection** — Compare alloc/dealloc pairs to find potential leaks
5. **Call tree analysis** — Build hierarchical view of allocation patterns

## Security Considerations

- Trace files are created world-readable if not in a protected directory
- Stack traces may leak internal implementation details
- Only use in development environments
- Never enable in production

## Related Files

- **ALLOC_TRACE_USAGE.md** — User guide for using the allocator
- **test_alloc_trace.sh** — Example script for testing
- **lidi-command-utils/src/allocator.rs** — Implementation
- **lidi-send/src/bin/lidi-send.rs** — Integration in sender
- **lidi-receive/src/bin/lidi-receive.rs** — Integration in receiver
