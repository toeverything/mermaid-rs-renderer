#!/usr/bin/env python3
"""Benchmark batch rendering throughput vs mermaid-cli."""

from __future__ import annotations

import argparse
import subprocess
import tempfile
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def run(cmd: list[str]) -> None:
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def main() -> None:
    parser = argparse.ArgumentParser(description="Batch throughput benchmark")
    parser.add_argument("--count", type=int, default=50, help="Number of diagrams")
    parser.add_argument(
        "--diagram",
        default=str(ROOT / "benches" / "fixtures" / "flowchart_tiny.mmd"),
        help="Path to a .mmd fixture",
    )
    parser.add_argument(
        "--mmdr",
        default=str(ROOT / "target" / "release" / "mmdr"),
        help="Path to mmdr binary",
    )
    parser.add_argument(
        "--mmdc",
        default="npx -y @mermaid-js/mermaid-cli",
        help="mermaid-cli command",
    )
    parser.add_argument("--warmup", type=int, default=1, help="Warmup runs")
    args = parser.parse_args()

    diagram = Path(args.diagram).read_text().strip()
    blocks = "\n\n".join([f"```mermaid\n{diagram}\n```" for _ in range(args.count)])
    md_path = Path(tempfile.gettempdir()) / "mmdr-batch.md"
    md_path.write_text(blocks)

    out_dir = Path(tempfile.gettempdir()) / "mmdr-batch-out"
    out_dir.mkdir(parents=True, exist_ok=True)

    mmdr_cmd = [args.mmdr, "-i", str(md_path), "-o", str(out_dir), "-e", "svg"]

    mmdc_cmd_base = args.mmdc.split() + ["-i", str(Path(args.diagram))]

    for _ in range(args.warmup):
        run(mmdr_cmd)
        run(mmdc_cmd_base + ["-o", str(out_dir / "mmdc-warmup.svg")])

    t0 = time.perf_counter()
    run(mmdr_cmd)
    mmdr_time = time.perf_counter() - t0

    t1 = time.perf_counter()
    for idx in range(args.count):
        run(mmdc_cmd_base + ["-o", str(out_dir / f"mmdc-{idx}.svg")])
    mmdc_time = time.perf_counter() - t1

    mmdr_per = (mmdr_time / args.count) * 1000.0
    mmdc_per = (mmdc_time / args.count) * 1000.0
    speedup = mmdc_time / mmdr_time if mmdr_time > 0 else 0.0

    print(f"Batch count: {args.count}")
    print(f"mmdr total: {mmdr_time:.3f}s ({mmdr_per:.2f} ms/diagram)")
    print(f"mmdc total: {mmdc_time:.3f}s ({mmdc_per:.2f} ms/diagram)")
    print(f"Speedup: {speedup:.0f}x")


if __name__ == "__main__":
    main()
