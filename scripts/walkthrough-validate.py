#!/usr/bin/env python3
"""Objective checks on the walkthrough screenshots.

Beyond "the app reached the final marker", this catches a silently broken UI:
a frame that renders as a solid colour (compositor never drew the window), a
frame at the wrong size, or a shot that never got written at all.

Reads the PNGs through netpbm's `pngtopnm` so there is no Pillow dependency --
the same netpbm the capture script already needs. Exits non-zero listing every
bad shot.

Usage: scripts/walkthrough-validate.py <dir> [--size WxH] [--expect name ...]
"""

import subprocess
import sys
from collections import Counter
from pathlib import Path

# A frame smaller than this is a stub, not a screenshot.
MIN_BYTES = 5_000
# The virtual screen the driver runs at.
EXPECTED_SIZE = (1400, 950)
# Below this many distinct colours the frame is effectively blank: an empty
# Adwaita window still has a header bar, text and shadows, so real frames land
# in the thousands.
MIN_DISTINCT_COLOURS = 200
# ... and the most common colour must not swallow the whole frame.
MAX_DOMINANT_FRACTION = 0.97


# Only every Nth pixel is sampled: colour statistics do not need every one, and
# a full 1400x950 frame is 1.3M tuples per shot.
SAMPLE_STRIDE = 7


def pixels(path: Path):
    """Decode a PNG to (width, height, sampled list of RGB tuples)."""
    data = subprocess.run(
        ["pngtopnm", str(path)], check=True, capture_output=True
    ).stdout
    # P6 is RGB; P5 is greyscale, which netpbm emits when the frame has no
    # colour at all -- a strong hint on its own that the app never drew.
    magic = data[:2]
    if magic not in (b"P6", b"P5"):
        raise ValueError(f"unexpected PNM magic {magic!r}")
    channels = 3 if magic == b"P6" else 1

    # Header: <magic> <width> <height> <maxval>, whitespace separated,
    # `#` comments.
    fields, offset = [], 2
    while len(fields) < 3:
        while data[offset : offset + 1].isspace():
            offset += 1
        if data[offset : offset + 1] == b"#":
            offset = data.index(b"\n", offset) + 1
            continue
        end = offset
        while not data[end : end + 1].isspace():
            end += 1
        fields.append(int(data[offset:end]))
        offset = end
    offset += 1

    width, height = fields[0], fields[1]
    body = memoryview(data)[offset:]
    step = channels * SAMPLE_STRIDE
    rgb = [
        tuple(body[i : i + channels])
        for i in range(0, width * height * channels, step)
    ]
    return width, height, rgb


def check(path: Path) -> list[str]:
    problems = []
    size = path.stat().st_size
    if size < MIN_BYTES:
        problems.append(f"only {size} bytes")

    try:
        width, height, rgb = pixels(path)
    except Exception as error:  # noqa: BLE001 - report, do not crash the run
        return problems + [f"could not decode: {error}"]

    if (width, height) != EXPECTED_SIZE:
        problems.append(f"is {width}x{height}, expected {EXPECTED_SIZE[0]}x{EXPECTED_SIZE[1]}")

    counts = Counter(rgb)
    distinct = len(counts)
    dominant = counts.most_common(1)[0][1] / len(rgb)
    if distinct < MIN_DISTINCT_COLOURS:
        problems.append(f"looks blank: only {distinct} distinct colours")
    if dominant > MAX_DOMINANT_FRACTION:
        problems.append(f"looks blank: {dominant:.1%} of pixels are one colour")

    return problems


def main() -> int:
    args = sys.argv[1:]
    if not args:
        print(__doc__, file=sys.stderr)
        return 2

    directory = Path(args[0])
    expected = args[args.index("--expect") + 1 :] if "--expect" in args else []
    if "--size" in args:
        global EXPECTED_SIZE  # noqa: PLW0603 - a CLI override, set once
        width, height = args[args.index("--size") + 1].split("x")
        EXPECTED_SIZE = (int(width), int(height))

    shots = sorted(directory.glob("*.png"))
    if not shots:
        print(f"no screenshots in {directory}", file=sys.stderr)
        return 1

    failures = []
    for shot in shots:
        problems = check(shot)
        status = "ok" if not problems else "; ".join(problems)
        print(f"{shot.name}: {status}")
        if problems:
            failures.append(shot.name)

    for name in expected:
        if not (directory / f"{name}.png").exists():
            print(f"{name}.png: missing")
            failures.append(f"{name}.png")

    if failures:
        print(f"\n{len(failures)} bad screenshot(s): {', '.join(failures)}", file=sys.stderr)
        return 1

    print(f"\nall {len(shots)} screenshots pass validation")
    return 0


if __name__ == "__main__":
    sys.exit(main())
