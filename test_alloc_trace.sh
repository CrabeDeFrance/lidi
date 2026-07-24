#!/bin/bash

# Example script demonstrating allocation tracing

set -e

TRACE_FILE="/tmp/lidi_alloc_trace_$(date +%s).log"

echo "=== Allocation Tracing Test ==="
echo ""
echo "This script demonstrates the allocation tracing feature."
echo "Output will be written to: $TRACE_FILE"
echo ""

# Build with allocation tracing if not already built
if [ ! -f "target/release/lidi-receive" ]; then
    echo "Building lidi-receive with alloc-trace feature..."
    cargo build --release --features alloc-trace -p lidi-receive
fi

echo ""
echo "=== Starting lidi-receive with allocation tracing ==="
echo "Trace file: $TRACE_FILE"
echo ""
echo "To use this in your own test:"
echo ""
echo "  LIDI_ALLOC_TRACE=$TRACE_FILE ./target/release/lidi-receive -c your_config.toml"
echo ""
echo "This will trace all memory allocations during packet processing."
echo ""

echo "=== Analyzing a trace file (example) ==="
echo ""
echo "Once you have a trace file, analyze it with:"
echo ""
echo "  # Count allocations"
echo "  grep -c '^\[alloc\]' $TRACE_FILE"
echo ""
echo "  # Find largest allocations"
echo "  grep '^\[alloc\]' $TRACE_FILE | sed 's/.*size=//' | sort -rn | head -10"
echo ""
echo "  # Get total bytes allocated"
echo "  grep '^\[alloc\]' $TRACE_FILE | awk -F'size=' '{sum += \$2} END {print sum \" bytes\"}'"
echo ""
