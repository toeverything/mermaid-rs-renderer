#!/usr/bin/env python3
import os
import statistics
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INPUT = ROOT / "docs/diagrams/architecture.mmd"
OUT_RUST = Path("/tmp/bench-rust.png")
OUT_MMD = Path("/tmp/bench-mmdc.png")

RUST_BIN = Path(os.environ.get("MMDR_BIN", str(ROOT / "target" / "release" / "mmdr")))
MMD_CMD = [
    "npx",
    "-y",
    "@mermaid-js/mermaid-cli",
    "-p",
    "/home/jeremy/jcode/tmp-puppeteer.json",
    "-i",
    str(INPUT),
    "-o",
    str(OUT_MMD),
]

RUST_CMD = [str(RUST_BIN), "-i", str(INPUT), "-o", str(OUT_RUST), "-e", "png"]


def bench(cmd, runs=5):
    times = []
    for _ in range(runs):
        t0 = time.perf_counter()
        subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        times.append(time.perf_counter() - t0)
    return times


def main():
    # warm-up
    subprocess.run(RUST_CMD, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    subprocess.run(MMD_CMD, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    rust_times = bench(RUST_CMD)
    mmd_times = bench(MMD_CMD)

    print("Rust renderer (seconds):", ", ".join(f"{t:.3f}" for t in rust_times))
    print("Mermaid CLI (seconds):", ", ".join(f"{t:.3f}" for t in mmd_times))

    def stats(values):
        return {
            "mean": statistics.mean(values),
            "p50": statistics.median(values),
            "min": min(values),
            "max": max(values),
        }

    rust_stats = stats(rust_times)
    mmd_stats = stats(mmd_times)

    print("\nSummary (seconds):")
    print("Rust renderer:", rust_stats)
    print("Mermaid CLI:", mmd_stats)


if __name__ == "__main__":
    main()
