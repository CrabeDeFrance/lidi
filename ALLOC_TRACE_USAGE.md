# Allocation Tracing Guide

This document describes how to trace all memory allocations in lidi-send and lidi-receive during packet processing.

## Compiling with allocation tracing

To compile with allocation tracing support, use the `alloc-trace` feature:

```bash
# Build lidi-receive with allocation tracing
cargo build --release --features alloc-trace -p lidi-receive

# Build lidi-send with allocation tracing
cargo build --release --features alloc-trace -p lidi-send

# Build entire workspace with allocation tracing
cargo build --release --features alloc-trace
```

## Enabling tracing at runtime

The tracing is **disabled by default** to avoid logging startup allocations. Enable it via the `LIDI_ALLOC_TRACE` environment variable:

```bash
# Run lidi-receive with allocation tracing to /tmp/alloc.log
LIDI_ALLOC_TRACE=/tmp/alloc.log ./target/release/lidi-receive -c config.toml

# Run lidi-send with allocation tracing to /tmp/alloc.log
LIDI_ALLOC_TRACE=/tmp/alloc.log ./target/release/lidi-send -c config.toml
```

## Output format

Each allocation is logged with the following information:

- **Operation**: `alloc`, `dealloc`, or `realloc`
- **Address**: Memory address (hex format `0x...`)
- **Size**: Number of bytes
- **Stack trace**: Full backtrace showing the call stack at the time of allocation

Example output:

```
[alloc] addr=0x7f1234567890, size=4096
 0: lidi_receive::allocator::dump_allocation
    at /workspace/lidi-command-utils/src/allocator.rs:47
 1: lidi_receive::allocator::TracingAllocator::alloc
    at /workspace/lidi-command-utils/src/allocator.rs:18
 2: core::alloc::global::GlobalAlloc::alloc
...
```

## Analyzing the traces

The trace file can be quite large during packet processing. Here are some useful commands:

```bash
# Count total allocations
grep -c "^\[alloc\]" /tmp/alloc.log

# Find largest allocations
grep "^\[alloc\]" /tmp/alloc.log | sed 's/.*size=//' | sort -rn | head -20

# Find allocations for a specific size
grep "\[alloc\].*size=1500" /tmp/alloc.log | wc -l

# Extract just the operation, address, and size (no stack traces)
grep "^\[" /tmp/alloc.log | grep -v "at " | head -100
```

## Disabling without recompiling

If you compiled with `--features alloc-trace`, you can disable it at runtime simply by not setting the `LIDI_ALLOC_TRACE` environment variable. Without this variable, tracing remains disabled.

## Performance impact

When tracing is **disabled** (no `LIDI_ALLOC_TRACE` set), there is minimal overhead—just two atomic loads per allocation.

When tracing is **enabled**, there is significant overhead due to:
- Stack trace capture (backtrace)
- File I/O for each allocation
- Lock contention on the trace file

**Do not use allocation tracing in production.** It's designed for development and debugging only.

## Notes

- Tracing must be enabled **after** program initialization to avoid logging all startup allocations
- The trace file is created (or overwritten if it exists)
- Tracing is thread-safe; all threads write to the same file with proper synchronization
- Stack traces require debug symbols; use `--release` with `debug = true` in `Cargo.toml` for better symbols (if needed)

## Example workflow

```bash
# Build with allocation tracing
cargo build --release --features alloc-trace -p lidi-receive

# Start receiver with tracing
LIDI_ALLOC_TRACE=/tmp/recv_alloc.log \
  ./target/release/lidi-receive -c receiver.toml &
RECEIVER_PID=$!

# ... run your test scenario ...

# Stop receiver
kill $RECEIVER_PID
wait $RECEIVER_PID 2>/dev/null

# Analyze
wc -l /tmp/recv_alloc.log
grep -c "\[alloc\]" /tmp/recv_alloc.log
grep "size=[0-9]\{6\}" /tmp/recv_alloc.log | head -10  # Find large allocations
```
