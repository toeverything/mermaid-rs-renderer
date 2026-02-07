#!/usr/bin/env python3
"""
Benchmark comparison script for mmdr vs mermaid-cli.

Features:
- Pipeline breakdown (parse/layout/render times)
- Memory usage tracking
- Visual SVG chart generation
"""
import json
import os
import re
import shlex
import statistics
import subprocess
import sys
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

BIN = os.environ.get("MMDR_BIN", str(ROOT / "target" / "release" / "mmdr"))
MMD_CLI = os.environ.get("MMD_CLI", "npx -y @mermaid-js/mermaid-cli")
RUNS = int(os.environ.get("RUNS", "8"))
WARMUP = int(os.environ.get("WARMUP", "2"))

CASE_NAMES = [
    "flowchart_small",
    "flowchart_medium",
    "flowchart_large",
    "flowchart_tiny",
    "flowchart_ports_heavy",
    "flowchart_weave",
    "flowchart_backedges_subgraphs",
    "flowchart_sparse_components",
    "flowchart_lanes_crossfeed",
    "flowchart_grid_feedback",
    "flowchart_fanout_returns",
    "flowchart_label_collision",
    "flowchart_nested_clusters",
    "flowchart_asymmetric_components",
    "flowchart_parallel_merges",
    "flowchart_long_edge_labels",
    "flowchart_selfloop_bidi",
    "flowchart_component_packing",
    "flowchart_direction_conflict",
    "flowchart_parallel_label_stack",
    "class_medium",
    "state_medium",
    "sequence_medium",
    "er_medium",
    "pie_medium",
    "mindmap_medium",
    "journey_medium",
    "timeline_medium",
    "gantt_medium",
    "requirement_medium",
    "gitgraph_medium",
    "c4_medium",
    "sankey_medium",
    "quadrant_medium",
    "zenuml_medium",
    "block_medium",
    "packet_medium",
    "kanban_medium",
    "architecture_medium",
    "radar_medium",
    "treemap_medium",
    "xychart_medium",
    "flowchart",
    "sequence",
    "class",
    "state",
    "class_tiny",
    "state_tiny",
    "sequence_tiny",
]


def resolve_cases():
    cases_env = os.environ.get("CASES")
    if cases_env:
        requested = [c.strip() for c in cases_env.split(",") if c.strip()]
        unknown = [c for c in requested if c not in CASE_NAMES]
        if unknown:
            raise ValueError(f"Unknown cases: {', '.join(unknown)}")
        names = requested
    else:
        names = CASE_NAMES
    return [(name, FIXTURES / f"{name}.mmd") for name in names]


CASES = resolve_cases()


def run_cmd(cmd, capture_stderr=False):
    """Run a command and return (success, stderr_output)."""
    env = os.environ.copy()
    if "PUPPETEER_EXECUTABLE_PATH" not in env:
        chrome = find_puppeteer_chrome()
        if chrome:
            env["PUPPETEER_EXECUTABLE_PATH"] = chrome
    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        env=env,
    )
    if result.returncode != 0 and not capture_stderr:
        raise subprocess.CalledProcessError(result.returncode, cmd)
    return result.returncode == 0, result.stderr


def short_error(text: str, limit: int = 180) -> str:
    cleaned = " ".join(text.split())
    if len(cleaned) <= limit:
        return cleaned
    return cleaned[: limit - 3] + "..."


def get_memory_usage(cmd) -> Optional[int]:
    """Get peak memory usage in KB using /usr/bin/time -v or resource module."""
    # Try GNU time first
    time_bin = "/usr/bin/time"
    if not os.path.exists(time_bin):
        # Try to find it elsewhere
        for path in ["/bin/time", "/usr/local/bin/time"]:
            if os.path.exists(path):
                time_bin = path
                break
        else:
            # Fall back to resource module for rough estimate
            try:
                import resource
                env = os.environ.copy()
                if "PUPPETEER_EXECUTABLE_PATH" not in env:
                    chrome = find_puppeteer_chrome()
                    if chrome:
                        env["PUPPETEER_EXECUTABLE_PATH"] = chrome
                subprocess.run(cmd, capture_output=True, env=env)
                # This only gets the current process, not subprocess, so it's not accurate
                # Return None to indicate we can't measure
                return None
            except ImportError:
                return None

    time_cmd = [time_bin, "-v"] + cmd
    env = os.environ.copy()
    if "PUPPETEER_EXECUTABLE_PATH" not in env:
        chrome = find_puppeteer_chrome()
        if chrome:
            env["PUPPETEER_EXECUTABLE_PATH"] = chrome
    try:
        result = subprocess.run(time_cmd, capture_output=True, text=True, env=env)
    except FileNotFoundError:
        return None
    if result.returncode != 0:
        return None
    # Parse "Maximum resident set size (kbytes): 12345"
    for line in result.stderr.split("\n"):
        if "Maximum resident set size" in line:
            match = re.search(r"(\d+)", line)
            if match:
                return int(match.group(1))
    return None


def bench_mmdr(path: Path):
    """Benchmark mmdr with timing breakdown."""
    out = ROOT / "target" / f"bench-{path.stem}.svg"
    extra_args = shlex.split(os.environ.get("MMDR_ARGS", ""))
    cmd_base = [BIN, "-i", str(path), "-o", str(out), "-e", "svg"] + extra_args
    cmd = cmd_base + ["--timing"]

    times = []
    breakdowns = []

    # Warmup
    for _ in range(WARMUP):
        run_cmd(cmd, capture_stderr=True)

    # Actual runs
    for _ in range(RUNS):
        start = time.perf_counter()
        success, stderr = run_cmd(cmd, capture_stderr=True)
        elapsed = time.perf_counter() - start
        times.append(elapsed)

        if success and stderr.strip():
            try:
                timing = json.loads(stderr.strip())
                breakdowns.append(timing)
            except json.JSONDecodeError:
                pass

    # Get memory usage
    memory_kb = get_memory_usage(cmd_base)

    return {
        "times": times,
        "breakdowns": breakdowns,
        "memory_kb": memory_kb,
    }


def bench_mermaid_cli(path: Path):
    """Benchmark mermaid-cli (no timing breakdown available)."""
    out = ROOT / "target" / f"bench-{path.stem}-mmdc.svg"
    cmd = MMD_CLI.split() + ["-i", str(path), "-o", str(out)]

    times = []

    # Warmup (and preflight to detect unsupported diagrams)
    if WARMUP == 0:
        success, stderr = run_cmd(cmd, capture_stderr=True)
        if not success:
            return {
                "times": [],
                "memory_kb": None,
                "error": short_error(stderr) or "mmdc failed",
            }
    else:
        for _ in range(WARMUP):
            success, stderr = run_cmd(cmd, capture_stderr=True)
            if not success:
                return {
                    "times": [],
                    "memory_kb": None,
                    "error": short_error(stderr) or "mmdc failed",
                }

    # Actual runs
    for _ in range(RUNS):
        start = time.perf_counter()
        success, stderr = run_cmd(cmd, capture_stderr=True)
        elapsed = time.perf_counter() - start
        if not success:
            return {
                "times": [],
                "memory_kb": None,
                "error": short_error(stderr) or "mmdc failed",
            }
        times.append(elapsed)

    # Get memory usage
    memory_kb = get_memory_usage(cmd)

    return {
        "times": times,
        "memory_kb": memory_kb,
    }


def summarize(times):
    """Summarize timing statistics."""
    return {
        "mean_ms": statistics.mean(times) * 1000,
        "median_ms": statistics.median(times) * 1000,
        "min_ms": min(times) * 1000,
        "max_ms": max(times) * 1000,
    }


def summarize_breakdowns(breakdowns):
    """Summarize pipeline breakdown statistics."""
    if not breakdowns:
        return None
    parse_us = [b["parse_us"] for b in breakdowns]
    layout_us = [b["layout_us"] for b in breakdowns]
    render_us = [b["render_us"] for b in breakdowns]
    return {
        "parse_ms": statistics.mean(parse_us) / 1000,
        "layout_ms": statistics.mean(layout_us) / 1000,
        "render_ms": statistics.mean(render_us) / 1000,
    }


def generate_svg_chart(results: dict, output_path: Path):
    """Generate an SVG bar chart comparing mmdr vs mermaid-cli."""
    # Chart dimensions
    width = 800
    height = 400
    margin = {"top": 40, "right": 150, "bottom": 80, "left": 120}
    chart_width = width - margin["left"] - margin["right"]
    chart_height = height - margin["top"] - margin["bottom"]

    # Prepare data
    cases = list(results["mmdr"].keys())
    mmdr_times = [results["mmdr"][c]["mean_ms"] for c in cases]
    cli_times = []
    for c in cases:
        entry = results.get("mermaid_cli", {}).get(c)
        if entry and entry.get("mean_ms") is not None:
            cli_times.append(entry["mean_ms"])

    # Use log scale for y-axis since values differ by 1000x
    import math

    max_time = max(mmdr_times + cli_times) if cli_times else max(mmdr_times)
    min_time = min(mmdr_times)

    # Create SVG
    svg_lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}">',
        '<style>',
        '  .title { font: bold 16px sans-serif; }',
        '  .axis-label { font: 12px sans-serif; }',
        '  .tick-label { font: 11px sans-serif; }',
        '  .legend { font: 12px sans-serif; }',
        '  .bar-mmdr { fill: #2e7d32; }',
        '  .bar-cli { fill: #f57c00; }',
        '  .bar-label { font: 10px sans-serif; fill: white; }',
        '</style>',
        f'<text x="{width/2}" y="25" text-anchor="middle" class="title">Render Time Comparison (log scale)</text>',
    ]

    # Y-axis (log scale)
    y_ticks = [1, 10, 100, 1000, 10000]
    for tick in y_ticks:
        if tick > max_time * 1.5:
            continue
        y = margin["top"] + chart_height - (math.log10(tick) / math.log10(max_time * 1.5)) * chart_height
        svg_lines.append(
            f'<line x1="{margin["left"]}" y1="{y}" x2="{margin["left"] + chart_width}" y2="{y}" stroke="#ddd" stroke-dasharray="2,2"/>'
        )
        svg_lines.append(
            f'<text x="{margin["left"] - 10}" y="{y + 4}" text-anchor="end" class="tick-label">{tick} ms</text>'
        )

    # Bars
    bar_width = chart_width / len(cases) * 0.35
    bar_gap = chart_width / len(cases)

    for i, case in enumerate(cases):
        x_base = margin["left"] + i * bar_gap + bar_gap * 0.15

        # mmdr bar
        mmdr_time = results["mmdr"][case]["mean_ms"]
        mmdr_height = (math.log10(max(mmdr_time, 0.1)) / math.log10(max_time * 1.5)) * chart_height
        mmdr_y = margin["top"] + chart_height - mmdr_height
        svg_lines.append(
            f'<rect x="{x_base}" y="{mmdr_y}" width="{bar_width}" height="{mmdr_height}" class="bar-mmdr" rx="2"/>'
        )
        svg_lines.append(
            f'<text x="{x_base + bar_width/2}" y="{mmdr_y + 15}" text-anchor="middle" class="bar-label">{mmdr_time:.1f}</text>'
        )

        # mermaid-cli bar
        cli_entry = results.get("mermaid_cli", {}).get(case)
        if cli_entry and cli_entry.get("mean_ms") is not None:
            cli_time = cli_entry["mean_ms"]
            cli_height = (math.log10(max(cli_time, 0.1)) / math.log10(max_time * 1.5)) * chart_height
            cli_y = margin["top"] + chart_height - cli_height
            svg_lines.append(
                f'<rect x="{x_base + bar_width + 5}" y="{cli_y}" width="{bar_width}" height="{cli_height}" class="bar-cli" rx="2"/>'
            )
            svg_lines.append(
                f'<text x="{x_base + bar_width + 5 + bar_width/2}" y="{cli_y + 15}" text-anchor="middle" class="bar-label">{cli_time:.0f}</text>'
            )

        # X-axis label
        label = case.replace("_", " ").replace("flowchart ", "flow ").replace("medium", "med")
        svg_lines.append(
            f'<text x="{x_base + bar_width}" y="{margin["top"] + chart_height + 20}" text-anchor="middle" class="tick-label" transform="rotate(-30 {x_base + bar_width} {margin["top"] + chart_height + 20})">{label}</text>'
        )

    # Legend
    legend_x = margin["left"] + chart_width + 20
    legend_y = margin["top"] + 20
    svg_lines.append(f'<rect x="{legend_x}" y="{legend_y}" width="15" height="15" class="bar-mmdr"/>')
    svg_lines.append(f'<text x="{legend_x + 20}" y="{legend_y + 12}" class="legend">mmdr</text>')
    if cli_times:
        svg_lines.append(f'<rect x="{legend_x}" y="{legend_y + 25}" width="15" height="15" class="bar-cli"/>')
        svg_lines.append(f'<text x="{legend_x + 20}" y="{legend_y + 37}" class="legend">mermaid-cli</text>')

    # Axes
    svg_lines.append(
        f'<line x1="{margin["left"]}" y1="{margin["top"]}" x2="{margin["left"]}" y2="{margin["top"] + chart_height}" stroke="#333" stroke-width="2"/>'
    )
    svg_lines.append(
        f'<line x1="{margin["left"]}" y1="{margin["top"] + chart_height}" x2="{margin["left"] + chart_width}" y2="{margin["top"] + chart_height}" stroke="#333" stroke-width="2"/>'
    )

    svg_lines.append("</svg>")

    output_path.write_text("\n".join(svg_lines))
    print(f"Wrote chart: {output_path}")


def generate_breakdown_chart(results: dict, output_path: Path):
    """Generate an SVG showing pipeline breakdown for mmdr."""
    width = 600
    height = 300
    margin = {"top": 40, "right": 30, "bottom": 80, "left": 100}
    chart_width = width - margin["left"] - margin["right"]
    chart_height = height - margin["top"] - margin["bottom"]

    cases = [c for c in results["mmdr"].keys() if results["mmdr"][c].get("breakdown")]
    if not cases:
        print("No breakdown data available")
        return

    svg_lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}">',
        '<style>',
        '  .title { font: bold 14px sans-serif; }',
        '  .tick-label { font: 10px sans-serif; }',
        '  .legend { font: 11px sans-serif; }',
        '  .bar-parse { fill: #4CAF50; }',
        '  .bar-layout { fill: #2196F3; }',
        '  .bar-render { fill: #FF9800; }',
        '</style>',
        f'<text x="{width/2}" y="25" text-anchor="middle" class="title">mmdr Pipeline Breakdown</text>',
    ]

    # Calculate max time for scale
    max_time = 0
    for case in cases:
        bd = results["mmdr"][case]["breakdown"]
        total = bd["parse_ms"] + bd["layout_ms"] + bd["render_ms"]
        max_time = max(max_time, total)

    bar_height = chart_height / len(cases) * 0.7
    bar_gap = chart_height / len(cases)

    for i, case in enumerate(cases):
        y_base = margin["top"] + i * bar_gap + bar_gap * 0.15
        bd = results["mmdr"][case]["breakdown"]

        # Stacked bars
        x = margin["left"]
        for stage, cls in [("parse_ms", "bar-parse"), ("layout_ms", "bar-layout"), ("render_ms", "bar-render")]:
            w = (bd[stage] / max_time) * chart_width
            svg_lines.append(f'<rect x="{x}" y="{y_base}" width="{w}" height="{bar_height}" class="{cls}"/>')
            if w > 25:
                svg_lines.append(
                    f'<text x="{x + w/2}" y="{y_base + bar_height/2 + 4}" text-anchor="middle" fill="white" font-size="9">{bd[stage]:.2f}</text>'
                )
            x += w

        # Case label
        label = case.replace("_", " ")
        svg_lines.append(
            f'<text x="{margin["left"] - 5}" y="{y_base + bar_height/2 + 4}" text-anchor="end" class="tick-label">{label}</text>'
        )

    # Legend
    legend_y = height - 30
    svg_lines.append(f'<rect x="{margin["left"]}" y="{legend_y}" width="12" height="12" class="bar-parse"/>')
    svg_lines.append(f'<text x="{margin["left"] + 17}" y="{legend_y + 10}" class="legend">Parse</text>')
    svg_lines.append(f'<rect x="{margin["left"] + 80}" y="{legend_y}" width="12" height="12" class="bar-layout"/>')
    svg_lines.append(f'<text x="{margin["left"] + 97}" y="{legend_y + 10}" class="legend">Layout</text>')
    svg_lines.append(f'<rect x="{margin["left"] + 170}" y="{legend_y}" width="12" height="12" class="bar-render"/>')
    svg_lines.append(f'<text x="{margin["left"] + 187}" y="{legend_y + 10}" class="legend">Render</text>')

    svg_lines.append("</svg>")
    output_path.write_text("\n".join(svg_lines))
    print(f"Wrote breakdown chart: {output_path}")


def main():
    results = {"mmdr": {}, "mermaid_cli": {}}

    print("Benchmarking mmdr...")
    for name, path in CASES:
        print(f"  {name}...", end=" ", flush=True)
        data = bench_mmdr(path)
        results["mmdr"][name] = {
            **summarize(data["times"]),
            "breakdown": summarize_breakdowns(data["breakdowns"]),
            "memory_kb": data["memory_kb"],
        }
        bd = results["mmdr"][name]["breakdown"]
        if bd:
            print(f"total={results['mmdr'][name]['mean_ms']:.2f}ms (parse={bd['parse_ms']:.2f} layout={bd['layout_ms']:.2f} render={bd['render_ms']:.2f})")
        else:
            print(f"total={results['mmdr'][name]['mean_ms']:.2f}ms")

    if os.environ.get("SKIP_MERMAID_CLI"):
        print("SKIP_MERMAID_CLI set, skipping mermaid-cli")
    else:
        print("\nBenchmarking mermaid-cli...")
        for name, path in CASES:
            print(f"  {name}...", end=" ", flush=True)
            data = bench_mermaid_cli(path)
            if data.get("error"):
                results["mermaid_cli"][name] = {
                    "error": data["error"],
                    "memory_kb": data["memory_kb"],
                }
                print(f"error={data['error']}")
            else:
                results["mermaid_cli"][name] = {
                    **summarize(data["times"]),
                    "memory_kb": data["memory_kb"],
                }
                print(f"total={results['mermaid_cli'][name]['mean_ms']:.2f}ms")

    # Print summary
    print("\n" + "=" * 70)
    print("SUMMARY")
    print("=" * 70)

    print("\nTiming (mean ms):")
    print(f"{'Case':<20} {'mmdr':>10} {'mermaid-cli':>12} {'Speedup':>10}")
    print("-" * 55)
    for name in results["mmdr"].keys():
        mmdr_ms = results["mmdr"][name]["mean_ms"]
        cli_entry = results.get("mermaid_cli", {}).get(name)
        if cli_entry and cli_entry.get("error"):
            print(f"{name:<20} {mmdr_ms:>10.2f} {'ERR':>12}")
        elif cli_entry and cli_entry.get("mean_ms") is not None:
            cli_ms = cli_entry["mean_ms"]
            speedup = cli_ms / mmdr_ms
            print(f"{name:<20} {mmdr_ms:>10.2f} {cli_ms:>12.0f} {speedup:>9.0f}x")
        else:
            print(f"{name:<20} {mmdr_ms:>10.2f} {'N/A':>12}")

    print("\nPipeline Breakdown (mmdr, mean ms):")
    print(f"{'Case':<20} {'Parse':>8} {'Layout':>8} {'Render':>8} {'Total':>8}")
    print("-" * 55)
    for name in results["mmdr"].keys():
        bd = results["mmdr"][name].get("breakdown")
        if bd:
            total = bd["parse_ms"] + bd["layout_ms"] + bd["render_ms"]
            print(f"{name:<20} {bd['parse_ms']:>8.2f} {bd['layout_ms']:>8.2f} {bd['render_ms']:>8.2f} {total:>8.2f}")

    print("\nMemory Usage (peak RSS):")
    print(f"{'Case':<20} {'mmdr':>12} {'mermaid-cli':>12}")
    print("-" * 45)
    for name in results["mmdr"].keys():
        mmdr_mem = results["mmdr"][name].get("memory_kb")
        mmdr_str = f"{mmdr_mem / 1024:.1f} MB" if mmdr_mem else "N/A"
        cli_entry = results.get("mermaid_cli", {}).get(name)
        if cli_entry and cli_entry.get("error"):
            cli_str = "ERR"
        elif cli_entry:
            cli_mem = cli_entry.get("memory_kb")
            cli_str = f"{cli_mem / 1024:.1f} MB" if cli_mem else "N/A"
        else:
            cli_str = "N/A"
        print(f"{name:<20} {mmdr_str:>12} {cli_str:>12}")

    # Write JSON
    out_json = Path(os.environ.get("OUT_JSON", str(ROOT / "target" / "bench-results.json")))
    out_json.write_text(json.dumps(results, indent=2))
    print(f"\nWrote {out_json}")

    # Generate charts unless explicitly skipped
    if not os.environ.get("SKIP_CHARTS"):
        charts_dir = Path(os.environ.get("CHARTS_DIR", str(ROOT / "docs" / "benchmarks")))
        charts_dir.mkdir(parents=True, exist_ok=True)
        generate_svg_chart(results, charts_dir / "comparison_chart.svg")
        generate_breakdown_chart(results, charts_dir / "breakdown_chart.svg")


if __name__ == "__main__":
    main()
