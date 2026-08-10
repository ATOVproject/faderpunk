#!/usr/bin/env python3
"""Decode the Core-1 panic beacon out of a MIDI capture.

The firmware cannot print a panic without a debug probe, so
`tasks/panic_beacon.rs` re-sends the panic site as CCs on MIDI channel 16.
This maps the 21-bit file hash back to a source path and prints file:line.

Usage: decode-panic-beacon.py <capture.log> [repo-root]
"""

import re
import sys
from pathlib import Path

CC_MARKER, CC_LINE_LO, CC_LINE_HI = 110, 111, 112
CC_FILE_0, CC_FILE_1, CC_FILE_2 = 113, 114, 115
BEACON_CHANNEL = 16  # receivemidi prints channels 1-based


def file_hash(path: str) -> int:
    """FNV-1a truncated to 21 bits — mirror of panic_beacon::file_hash."""
    h = 0x811C9DC5
    for b in path.encode():
        h = ((h ^ b) * 0x01000193) & 0xFFFFFFFF
    return h & 0x001FFFFF


def parse(capture: Path):
    line_re = re.compile(
        r"channel\s+(\d+)\s+control-change\s+(\d+)\s+(\d+)"
    )
    seen = {}
    for raw in capture.read_text(errors="replace").splitlines():
        m = line_re.search(raw)
        if not m:
            continue
        ch, cc, val = int(m.group(1)), int(m.group(2)), int(m.group(3))
        if ch != BEACON_CHANNEL:
            continue
        if cc in (CC_MARKER, CC_LINE_LO, CC_LINE_HI, CC_FILE_0, CC_FILE_1, CC_FILE_2):
            seen[cc] = val
    return seen


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    capture = Path(sys.argv[1])
    root = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(__file__).resolve().parent.parent

    seen = parse(capture)
    if CC_MARKER not in seen:
        print("No panic beacon in capture — Core 1 did not panic (or died before the handler ran).")
        return 1

    line = seen.get(CC_LINE_LO, 0) | (seen.get(CC_LINE_HI, 0) << 7)
    want = (
        seen.get(CC_FILE_0, 0)
        | (seen.get(CC_FILE_1, 0) << 7)
        | (seen.get(CC_FILE_2, 0) << 14)
    )

    matches = []
    for path in root.rglob("*.rs"):
        if "/target/" in str(path):
            continue
        rel = str(path.relative_to(root))
        # rustc records the path as given to the compiler; try both forms.
        for candidate in (rel, str(path)):
            if file_hash(candidate) == want:
                matches.append(candidate)

    print(f"PANIC at line {line} (file hash 0x{want:06x})")
    if matches:
        for m in sorted(set(matches)):
            print(f"  → {m}:{line}")
            src = (root / m) if (root / m).exists() else Path(m)
            try:
                text = src.read_text(errors="replace").splitlines()
                lo, hi = max(0, line - 4), min(len(text), line + 3)
                for i in range(lo, hi):
                    mark = ">>" if i + 1 == line else "  "
                    print(f"    {mark} {i + 1:5d}| {text[i]}")
            except OSError:
                pass
    else:
        print("  (no source file matched the hash — check the repo root argument)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
