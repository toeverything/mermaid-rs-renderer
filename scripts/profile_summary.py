#!/usr/bin/env python3
"""Summarize mmdr timing breakdowns across fixtures."""

from __future__ import annotations

import argparse
import json
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def run_timing(bin_path: Path, fixture: Path) -> dict:
    with tempfile.NamedTemporaryFile(suffix=".svg", delete=False) as tmp:
        out_path = tmp.name
    cmd = [
        str(bin_path),
        "-i",
        str(fixture),
        "-o",
        out_path,
        "-e",
        "svg",
        "--timing",
    ]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or f"mmdr failed for {fixture}")
    text = result.stderr.strip().splitlines()
    if not text:
        raise RuntimeError(f"No timing output for {fixture}")
    payload = text[-1]
    return json.loads(payload)


def summarize(values: list[int]) -> float:
    if not values:
        return 0.0
    return sum(values) / len(values)


def main() -> None:
    parser = argparse.ArgumentParser(description="Summarize mmdr timing breakdowns.")
    parser.add_argument(
        "--bin",
        default=str(ROOT / "target" / "release" / "mmdr"),
        help="Path to mmdr binary",
    )
    parser.add_argument(
        "--fixtures",
        action="append",
        default=[],
        help="Fixture files or directories (repeatable). Defaults to benches/fixtures.",
    )
    parser.add_argument("--runs", type=int, default=5, help="Runs per fixture")
    parser.add_argument("--warmup", type=int, default=1, help="Warmup runs per fixture")
    parser.add_argument(
        "--pattern",
        action="append",
        default=[],
        help="Filter fixtures by substring (repeatable)",
    )
    args = parser.parse_args()

    bin_path = Path(args.bin)
    if not bin_path.exists():
        subprocess.run(["cargo", "build", "--release"], check=True, cwd=ROOT)

    fixtures: list[Path] = []
    roots = [Path(p) for p in args.fixtures if p] or [ROOT / "benches" / "fixtures"]
    for root in roots:
        if root.is_file() and root.suffix == ".mmd":
            fixtures.append(root)
        elif root.exists():
            fixtures.extend(sorted(root.glob("**/*.mmd")))

    if args.pattern:
        patterns = args.pattern
        fixtures = [f for f in fixtures if any(p in str(f) for p in patterns)]

    if not fixtures:
        raise SystemExit("No fixtures found")

    print("Profiling mmdr timing breakdowns:")
    for fixture in fixtures:
        parse_vals: list[int] = []
        layout_vals: list[int] = []
        render_vals: list[int] = []
        total_vals: list[int] = []
        for _ in range(args.warmup):
            _ = run_timing(bin_path, fixture)
        for _ in range(args.runs):
            timing = run_timing(bin_path, fixture)
            parse_vals.append(int(timing["parse_us"]))
            layout_vals.append(int(timing["layout_us"]))
            render_vals.append(int(timing["render_us"]))
            total_vals.append(int(timing["total_us"]))
        parse_avg = summarize(parse_vals)
        layout_avg = summarize(layout_vals)
        render_avg = summarize(render_vals)
        total_avg = summarize(total_vals)
        if total_avg <= 0:
            continue
        parse_pct = (parse_avg / total_avg) * 100.0
        layout_pct = (layout_avg / total_avg) * 100.0
        render_pct = (render_avg / total_avg) * 100.0
        print(
            f"- {fixture.name}: total {total_avg/1000:.2f} ms "
            f"(parse {parse_avg/1000:.2f} ms {parse_pct:.0f}%, "
            f"layout {layout_avg/1000:.2f} ms {layout_pct:.0f}%, "
            f"render {render_avg/1000:.2f} ms {render_pct:.0f}%)"
        )


if __name__ == "__main__":
    main()
