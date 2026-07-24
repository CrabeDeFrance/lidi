#!/usr/bin/env python3
"""
Allocation Log Analyzer
Parses allocation traces and generates memory statistics reports.
"""

import re
import sys
from collections import defaultdict
from pathlib import Path


def parse_allocations(log_file: str) -> list[dict]:
    """Parse alloc.log and extract allocation events with sources."""
    with open(log_file, 'r') as f:
        content = f.read()

    allocations = []
    allocations_text = content.split('[alloc] addr=')[1:]

    for alloc_text in allocations_text:
        lines = alloc_text.split('\n')

        # Parse size from first line
        match = re.search(r'size=(\d+)', lines[0])
        if not match:
            continue

        size = int(match.group(1))

        # Find first lidi- source (skip allocator.rs)
        source = None
        crate = None
        file = None

        for line in lines[1:]:
            if 'lidi-' in line and '.rs:' in line and 'allocator' not in line.lower():
                match = re.search(r'(lidi-[^/]+)/src/([^:]+\.rs)', line)
                if match:
                    crate = match.group(1)
                    file = match.group(2)
                    source = f"{crate}/{file}"
                    break

        if source:
            allocations.append({
                'size': size,
                'crate': crate,
                'file': file,
                'source': source,
            })

    return allocations


def analyze_allocations(allocations: list[dict]) -> dict:
    """Generate statistics from allocations."""
    if not allocations:
        return {}

    total_count = len(allocations)
    total_memory = sum(a['size'] for a in allocations)
    sizes = [a['size'] for a in allocations]

    # Size ranges
    buckets = defaultdict(int)
    for size in sizes:
        if size <= 16:
            buckets['1-16'] += 1
        elif size <= 64:
            buckets['17-64'] += 1
        elif size <= 256:
            buckets['65-256'] += 1
        elif size <= 1024:
            buckets['257-1K'] += 1
        elif size <= 4096:
            buckets['1K-4K'] += 1
        elif size <= 16384:
            buckets['4K-16K'] += 1
        elif size <= 65536:
            buckets['16K-64K'] += 1
        else:
            buckets['>64K'] += 1

    # Crate stats
    crate_stats = defaultdict(lambda: {'count': 0, 'total': 0})
    for alloc in allocations:
        if alloc['crate']:
            crate_stats[alloc['crate']]['count'] += 1
            crate_stats[alloc['crate']]['total'] += alloc['size']

    # Source stats
    source_stats = defaultdict(lambda: {'count': 0, 'total': 0})
    for alloc in allocations:
        if alloc['source']:
            source_stats[alloc['source']]['count'] += 1
            source_stats[alloc['source']]['total'] += alloc['size']

    return {
        'total_count': total_count,
        'total_memory': total_memory,
        'min_size': min(sizes),
        'max_size': max(sizes),
        'avg_size': total_memory / total_count,
        'buckets': dict(buckets),
        'crate_stats': dict(crate_stats),
        'source_stats': dict(source_stats),
    }


def format_bytes(b: int) -> str:
    """Format bytes with proper units."""
    for unit in ['B', 'KB', 'MB', 'GB']:
        if b < 1024:
            return f"{b:.1f} {unit}"
        b /= 1024
    return f"{b:.1f} TB"


def print_text_report(stats: dict):
    """Print human-readable text report."""
    if not stats:
        print("No allocations found.")
        return

    print("\n" + "=" * 70)
    print("ALLOCATION STATISTICS")
    print("=" * 70)

    print(f"\nTotal allocations: {stats['total_count']:,}")
    print(f"Total memory:      {format_bytes(stats['total_memory'])}")
    print(f"Min size:          {stats['min_size']} bytes")
    print(f"Max size:          {format_bytes(stats['max_size'])}")
    print(f"Average size:      {stats['avg_size']:.2f} bytes")

    # Size distribution
    print("\n" + "-" * 70)
    print("SIZE DISTRIBUTION")
    print("-" * 70)

    ranges = [
        ('1-16', '1–16 bytes'),
        ('17-64', '17–64 bytes'),
        ('65-256', '65–256 bytes'),
        ('257-1K', '257–1,024 bytes'),
        ('1K-4K', '1–4 KB'),
        ('4K-16K', '4–16 KB'),
        ('16K-64K', '16–64 KB'),
        ('>64K', '> 64 KB'),
    ]

    for key, label in ranges:
        if key in stats['buckets']:
            count = stats['buckets'][key]
            pct = (count * 100.0 / stats['total_count'])
            marker = " ← Most" if pct > 50 else ""
            print(
                f"{label:20} {count:7,} ({pct:5.1f}%){marker}"
            )

    # By crate
    print("\n" + "-" * 70)
    print("ALLOCATION SOURCES BY CRATE")
    print("-" * 70)

    sorted_crates = sorted(
        stats['crate_stats'].items(),
        key=lambda x: x[1]['total'],
        reverse=True
    )

    for crate, info in sorted_crates:
        count = info['count']
        total = info['total']
        avg = total / count if count > 0 else 0
        pct = (total * 100.0 / stats['total_memory'])
        print(
            f"{format_bytes(total):>8} ({count:6,}) {pct:5.1f}%  "
            f"avg={avg:7.0f} bytes  {crate}"
        )

    # Top sources
    print("\n" + "-" * 70)
    print("TOP 15 ALLOCATION SOURCES")
    print("-" * 70)

    sorted_sources = sorted(
        stats['source_stats'].items(),
        key=lambda x: x[1]['total'],
        reverse=True
    )[:15]

    for source, info in sorted_sources:
        count = info['count']
        total = info['total']
        avg = total / count if count > 0 else 0
        pct = (total * 100.0 / stats['total_memory'])
        print(
            f"{format_bytes(total):>8} ({count:6,}) {pct:5.1f}%  "
            f"avg={avg:7.0f}"
        )
        print(f"  └─ {source}\n")

    print("=" * 70 + "\n")


def print_markdown_report(stats: dict, output_file: str = None):
    """Generate Markdown report."""
    if not stats:
        print("No allocations found.")
        return

    lines = [
        "# Allocation Statistics Report\n",
        "## Summary\n",
        "| Metric | Value |",
        "|--------|-------|",
        f"| **Total allocations** | {stats['total_count']:,} |",
        f"| **Total memory** | {format_bytes(stats['total_memory'])} |",
        f"| **Min size** | {stats['min_size']} byte |",
        f"| **Max size** | {format_bytes(stats['max_size'])} |",
        f"| **Average size** | {stats['avg_size']:.2f} bytes |",
        "\n## Size Distribution\n",
        "| Range | Count | Percentage |",
        "|-------|-------|-----------|",
    ]

    ranges = [
        ('1-16', '1–16 bytes'),
        ('17-64', '17–64 bytes'),
        ('65-256', '65–256 bytes'),
        ('257-1K', '257–1,024 bytes'),
        ('1K-4K', '1–4 KB'),
        ('4K-16K', '4–16 KB'),
        ('16K-64K', '16–64 KB'),
        ('>64K', '> 64 KB'),
    ]

    for key, label in ranges:
        if key in stats['buckets']:
            count = stats['buckets'][key]
            pct = (count * 100.0 / stats['total_count'])
            lines.append(f"| {label} | {count:,} | {pct:.1f}% |")

    lines.extend([
        "\n## Sources by Crate\n",
        "| Crate | Memory | Count | Avg Size |",
        "|-------|--------|-------|----------|",
    ])

    sorted_crates = sorted(
        stats['crate_stats'].items(),
        key=lambda x: x[1]['total'],
        reverse=True
    )

    for crate, info in sorted_crates:
        count = info['count']
        total = info['total']
        avg = total / count if count > 0 else 0
        lines.append(f"| {crate} | {format_bytes(total)} | {count:,} | {avg:.0f} bytes |")

    lines.extend([
        "\n## Top 20 Sources\n",
        "| Source | Memory | Count | Avg Size |",
        "|--------|--------|-------|----------|",
    ])

    sorted_sources = sorted(
        stats['source_stats'].items(),
        key=lambda x: x[1]['total'],
        reverse=True
    )[:20]

    for source, info in sorted_sources:
        count = info['count']
        total = info['total']
        avg = total / count if count > 0 else 0
        lines.append(
            f"| `{source}` | {format_bytes(total)} | {count:,} | {avg:.0f} bytes |"
        )

    report = '\n'.join(lines) + '\n'

    if output_file:
        with open(output_file, 'w') as f:
            f.write(report)
        print(f"Report written to: {output_file}")
    else:
        print(report)


def main():
    if len(sys.argv) < 2:
        print("Usage: analyze_allocations.py <alloc.log> [--markdown] [--output FILE]")
        print("\nOptions:")
        print("  --markdown          Output Markdown format (default: text)")
        print("  --output FILE       Write to file instead of stdout")
        sys.exit(1)

    log_file = sys.argv[1]
    markdown = '--markdown' in sys.argv
    output_file = None

    if '--output' in sys.argv:
        idx = sys.argv.index('--output')
        if idx + 1 < len(sys.argv):
            output_file = sys.argv[idx + 1]

    if not Path(log_file).exists():
        print(f"Error: {log_file} not found")
        sys.exit(1)

    print(f"Parsing {log_file}...", file=sys.stderr)
    allocations = parse_allocations(log_file)
    print(f"Found {len(allocations):,} allocations", file=sys.stderr)

    print(f"Analyzing...", file=sys.stderr)
    stats = analyze_allocations(allocations)

    if markdown:
        print_markdown_report(stats, output_file)
    else:
        print_text_report(stats)


if __name__ == '__main__':
    main()
