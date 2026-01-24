#!/usr/bin/env python3
import os
import statistics
import subprocess
import time
from pathlib import Path

from typing import Optional


def find_puppeteer_chrome() -> Optional[str]:
    base = Path.home() / ".cache" / "puppeteer" / "chrome"
    if not base.exists():
        return None
    candidates = sorted(base.glob("**/chrome"))
    return str(candidates[-1]) if candidates else None


ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "benches" / "fixtures"

BIN = os.environ.get("MMDR_BIN", str(ROOT / "target" / "release" / "mermaid-rs-renderer"))
MMD_CLI = os.environ.get("MMD_CLI", "npx -y @mermaid-js/mermaid-cli")
RUNS = int(os.environ.get("RUNS", "8"))
WARMUP = int(os.environ.get("WARMUP", "2"))

CASES = [
    ("flowchart_small", FIXTURES / "flowchart_small.mmd"),
    ("flowchart_medium", FIXTURES / "flowchart_medium.mmd"),
    ("flowchart_large", FIXTURES / "flowchart_large.mmd"),
    ("class_medium", FIXTURES / "class_medium.mmd"),
    ("state_medium", FIXTURES / "state_medium.mmd"),
    ("sequence_medium", FIXTURES / "sequence_medium.mmd"),
]


def run_cmd(cmd):
    env = os.environ.copy()
    if 'PUPPETEER_EXECUTABLE_PATH' not in env:
        chrome = find_puppeteer_chrome()
        if chrome:
            env['PUPPETEER_EXECUTABLE_PATH'] = chrome
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env)


def bench(cmd):
    times = []
    for _ in range(WARMUP):
        run_cmd(cmd)
    for _ in range(RUNS):
        start = time.perf_counter()
        run_cmd(cmd)
        times.append(time.perf_counter() - start)
    return times


def summarize(times):
    return {
        "mean_ms": statistics.mean(times) * 1000,
        "median_ms": statistics.median(times) * 1000,
        "min_ms": min(times) * 1000,
        "max_ms": max(times) * 1000,
    }


def main():
    results = {"mmdr": {}, "mermaid_cli": {}}

    for name, path in CASES:
        out = ROOT / "target" / f"bench-{name}.svg"
        cmd = [BIN, "-i", str(path), "-o", str(out), "-e", "svg"]
        times = bench(cmd)
        results["mmdr"][name] = summarize(times)

    if os.environ.get("SKIP_MERMAID_CLI"):
        print("SKIP_MERMAID_CLI set, skipping mermaid-cli")
    else:
        for name, path in CASES:
            out = ROOT / "target" / f"bench-{name}-mmdc.svg"
            cmd = MMD_CLI.split() + ["-i", str(path), "-o", str(out)]
            times = bench(cmd)
            results["mermaid_cli"][name] = summarize(times)

    print("Benchmark results (ms):")
    for tool, tool_results in results.items():
        if not tool_results:
            continue
        print(f"\n[{tool}]")
        for case, stats in tool_results.items():
            print(f"{case:16s} mean={stats['mean_ms']:.2f} median={stats['median_ms']:.2f} min={stats['min_ms']:.2f} max={stats['max_ms']:.2f}")

    # Write JSON for README updates
    out_json = ROOT / "target" / "bench-results.json"
    out_json.write_text(__import__("json").dumps(results, indent=2))
    print(f"\nWrote {out_json}")


if __name__ == "__main__":
    main()
