#!/usr/bin/env python3
"""Generate benchmark SVG charts from bench-results.json."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CASES = [
    ("flowchart", "Flowchart"),
    ("class", "Class"),
    ("state", "State"),
    ("sequence", "Sequence"),
]

# Colors
GREEN = "#10b981"
ORANGE = "#f97316"
BLUE = "#3b82f6"
PURPLE = "#8b5cf6"


def load_results(path: Path) -> dict:
    if not path.exists():
        raise FileNotFoundError(f"Missing benchmark results at {path}. Run scripts/bench_compare.py first.")
    return json.loads(path.read_text())


def build_comparison_data(results: dict, cases: list[tuple[str, str]]):
    data = []
    for key, label in cases:
        mmdr = results.get("mmdr", {}).get(key)
        cli = results.get("mermaid_cli", {}).get(key)
        if not mmdr or not cli or cli.get("mean_ms") is None:
            continue
        data.append(
            {
                "name": label,
                "mmdr": float(mmdr["mean_ms"]),
                "cli": float(cli["mean_ms"]),
            }
        )
    if not data:
        raise RuntimeError("No comparison data found in bench-results.json.")
    return data


def build_breakdown_data(results: dict, cases: list[tuple[str, str]]):
    data = []
    for key, label in cases:
        mmdr = results.get("mmdr", {}).get(key)
        breakdown = (mmdr or {}).get("breakdown")
        if not breakdown:
            continue
        data.append(
            {
                "name": label,
                "parse": float(breakdown["parse_ms"]),
                "layout": float(breakdown["layout_ms"]),
                "render": float(breakdown["render_ms"]),
            }
        )
    if not data:
        raise RuntimeError("No breakdown data found in bench-results.json.")
    return data


def generate_comparison_chart(data):
    """Generate the mmdr vs mermaid-cli comparison chart."""
    width, height = 600, 320
    bar_width = 50
    spacing = 125
    start_x = 55
    chart_top = 105
    chart_bottom = 285
    chart_height = chart_bottom - chart_top

    max_val = max(d["cli"] for d in data)

    svg = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}">
  <style>
    .title {{ font: bold 18px system-ui, sans-serif; fill: #000; }}
    .label {{ font: bold 14px system-ui, sans-serif; fill: #000; }}
    .value {{ font: bold 11px system-ui, sans-serif; fill: white; }}
    .speedup {{ font: bold 14px system-ui, sans-serif; fill: #047857; }}
    .legend {{ font: 13px system-ui, sans-serif; fill: #000; }}
  </style>

  <text x="{width/2}" y="28" text-anchor="middle" class="title">Render Time Comparison (ms)</text>

  <!-- Legend -->
  <rect x="200" y="45" width="14" height="14" rx="2" fill="{GREEN}"/>
  <text x="218" y="57" class="legend">mmdr</text>
  <rect x="290" y="45" width="14" height="14" rx="2" fill="{ORANGE}"/>
  <text x="308" y="57" class="legend">mermaid-cli</text>
'''

    for i, d in enumerate(data):
        x = start_x + i * spacing
        center_x = x + bar_width

        mmdr_height = max(18, (d["mmdr"] / max_val) * chart_height)
        mmdr_y = chart_bottom - mmdr_height

        cli_height = (d["cli"] / max_val) * chart_height
        cli_y = chart_bottom - cli_height

        speedup = int(d["cli"] / d["mmdr"])

        svg += f'''
  <!-- {d["name"]} -->
  <text x="{center_x}" y="95" text-anchor="middle" class="label">{d["name"]}</text>
  <rect x="{x}" y="{mmdr_y}" width="{bar_width}" height="{mmdr_height}" rx="4" fill="{GREEN}"/>
  <text x="{x + bar_width/2}" y="{mmdr_y + 14}" text-anchor="middle" class="value">{d["mmdr"]:.1f}</text>
  <rect x="{x + bar_width + 10}" y="{cli_y}" width="{bar_width}" height="{cli_height}" rx="4" fill="{ORANGE}"/>
  <text x="{x + bar_width + 10 + bar_width/2}" y="{cli_y + 20}" text-anchor="middle" class="value">{d["cli"]:.0f}</text>
  <text x="{center_x}" y="305" text-anchor="middle" class="speedup">{speedup}×</text>
'''

    svg += "</svg>\n"
    return svg


def generate_breakdown_chart(data):
    """Generate the pipeline breakdown stacked bar chart."""
    width, height = 520, 300
    margin = {"top": 40, "right": 20, "bottom": 40, "left": 50}
    chart_width = width - margin["left"] - margin["right"]
    chart_height = height - margin["top"] - margin["bottom"]
    baseline = margin["top"] + chart_height

    bar_width = chart_width / max(1, len(data)) * 0.6
    spacing = chart_width / max(1, len(data))
    start_x = margin["left"] + spacing * 0.2

    max_total = max(d["parse"] + d["layout"] + d["render"] for d in data)
    ticks = 4

    svg = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}">
  <style>
    .title {{ font: bold 18px system-ui, sans-serif; fill: #000; }}
    .label {{ font: bold 13px system-ui, sans-serif; fill: #000; }}
    .value {{ font: bold 10px system-ui, sans-serif; fill: white; }}
    .axis {{ font: 12px system-ui, sans-serif; fill: #000; }}
    .legend {{ font: bold 12px system-ui, sans-serif; fill: #000; }}
  </style>

  <text x="{width/2}" y="28" text-anchor="middle" class="title">mmdr Pipeline Breakdown (ms)</text>

  <!-- Legend -->
  <rect x="140" y="46" width="14" height="14" rx="2" fill="{GREEN}"/>
  <text x="158" y="58" class="legend">Parse</text>
  <rect x="220" y="46" width="14" height="14" rx="2" fill="{BLUE}"/>
  <text x="238" y="58" class="legend">Layout</text>
  <rect x="310" y="46" width="14" height="14" rx="2" fill="{PURPLE}"/>
  <text x="328" y="58" class="legend">Render</text>
'''

    for i in range(ticks + 1):
        val = max_total * (i / ticks)
        y = baseline - (val / max_total) * chart_height
        svg += f'''
  <line x1="{margin["left"]}" y1="{y:.1f}" x2="{width - margin["right"]}" y2="{y:.1f}" stroke="#d1d5db" stroke-width="1"/>
  <text x="{margin["left"] - 6}" y="{y + 4:.1f}" text-anchor="end" class="axis">{val:.1f}</text>'''

    for i, d in enumerate(data):
        x = start_x + i * spacing
        center_x = x + bar_width / 2
        total = d["parse"] + d["layout"] + d["render"]

        parse_h = (d["parse"] / max_total) * chart_height
        layout_h = (d["layout"] / max_total) * chart_height
        render_h = (d["render"] / max_total) * chart_height

        parse_y = baseline - parse_h
        layout_y = parse_y - layout_h
        render_y = layout_y - render_h

        svg += f'''
  <!-- {d["name"]} ({total:.1f}ms) -->
  <rect x="{x}" y="{parse_y:.1f}" width="{bar_width}" height="{parse_h:.1f}" fill="{GREEN}"/>
  <text x="{center_x}" y="{parse_y + parse_h/2 + 4:.1f}" text-anchor="middle" class="value">{d["parse"]:.1f}</text>
  <rect x="{x}" y="{layout_y:.1f}" width="{bar_width}" height="{layout_h:.1f}" fill="{BLUE}"/>
  <text x="{center_x}" y="{layout_y + layout_h/2 + 4:.1f}" text-anchor="middle" class="value">{d["layout"]:.1f}</text>
  <rect x="{x}" y="{render_y:.1f}" width="{bar_width}" height="{render_h:.1f}" rx="3 3 0 0" fill="{PURPLE}"/>
  <text x="{center_x}" y="{baseline + 18}" text-anchor="middle" class="label">{d["name"]}</text>
'''

    svg += "</svg>\n"
    return svg


def main():
    parser = argparse.ArgumentParser(description="Generate benchmark charts from bench-results.json")
    parser.add_argument(
        "--input",
        default=str(ROOT / "target" / "bench-results.json"),
        help="Path to bench-results.json (default: target/bench-results.json)",
    )
    parser.add_argument(
        "--out-dir",
        default=str(ROOT / "docs" / "benchmarks"),
        help="Output directory for comparison.svg and breakdown.svg",
    )
    args = parser.parse_args()

    results = load_results(Path(args.input))
    cases = DEFAULT_CASES

    comparison_data = build_comparison_data(results, cases)
    breakdown_data = build_breakdown_data(results, cases)

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    (out_dir / "comparison.svg").write_text(generate_comparison_chart(comparison_data))
    print("Generated comparison.svg")
    (out_dir / "breakdown.svg").write_text(generate_breakdown_chart(breakdown_data))
    print("Generated breakdown.svg")


if __name__ == "__main__":
    main()
