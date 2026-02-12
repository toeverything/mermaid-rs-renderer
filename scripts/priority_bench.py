#!/usr/bin/env python3
"""Rank layout improvement priorities by combining quality pain and layout time."""

from __future__ import annotations

import argparse
import datetime
import getpass
import importlib.util
import json
import math
import platform
import re
import socket
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]

# Fallback only; default scoring derives weights programmatically from fixture data.
MANUAL_WEIGHTS = {
    "edge_crossings": 8.0,
    "edge_node_crossings": 10.0,
    "node_overlap_count": 12.0,
    "edge_bends": 1.5,
    "port_congestion": 2.5,
    "edge_overlap_length": 1.0,
    "edge_detour_penalty": 35.0,
    "space_efficiency_penalty": 260.0,
    "margin_imbalance_ratio": 130.0,
    "edge_length_per_node": 0.4,
}

DEFAULT_PRIORITY_METRICS = [
    "edge_crossings",
    "edge_node_crossings",
    "node_overlap_count",
    "edge_bends",
    "crossing_angle_penalty",
    "angular_resolution_penalty",
    "port_congestion",
    "edge_overlap_length",
    "edge_node_near_miss_count",
    "node_spacing_violation_severity",
    "edge_detour_penalty",
    "wasted_space_ratio",
    "space_efficiency_penalty",
    "margin_imbalance_ratio",
    "component_gap_ratio",
    "component_balance_penalty",
    "content_center_offset_ratio",
    "content_aspect_elongation",
    "content_overflow_ratio",
    "edge_length_per_node",
    "label_overlap_count",
    "label_overlap_area",
    "label_edge_overlap_count",
    "label_edge_overlap_pairs",
    "label_out_of_bounds_count",
    "label_out_of_bounds_area",
    "label_out_of_bounds_ratio",
]


def load_layout_score():
    module_path = ROOT / "scripts" / "layout_score.py"
    spec = importlib.util.spec_from_file_location("layout_score", module_path)
    if spec is None or spec.loader is None:
        raise RuntimeError("failed to load layout_score.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # type: ignore[call-arg]
    return module


def load_quality_bench():
    module_path = ROOT / "scripts" / "quality_bench.py"
    spec = importlib.util.spec_from_file_location("quality_bench", module_path)
    if spec is None or spec.loader is None:
        raise RuntimeError("failed to load quality_bench.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # type: ignore[call-arg]
    return module


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def iso_utc_now() -> str:
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def git_metadata() -> dict[str, Any]:
    def git(args: list[str]) -> str:
        res = run(["git"] + args)
        if res.returncode != 0:
            return ""
        return res.stdout.strip()

    commit = git(["rev-parse", "HEAD"])
    short = commit[:12] if commit else ""
    branch = git(["rev-parse", "--abbrev-ref", "HEAD"])
    describe = git(["describe", "--always", "--dirty", "--tags"])
    status = git(["status", "--porcelain"])
    return {
        "commit": commit,
        "commit_short": short,
        "branch": branch,
        "describe": describe,
        "dirty": bool(status),
    }


def host_metadata() -> dict[str, str]:
    return {
        "hostname": socket.gethostname(),
        "user": getpass.getuser(),
        "python": sys.version.split()[0],
        "platform": platform.platform(),
    }


def append_benchmark_history(history_path: Path, record: dict[str, Any]) -> None:
    history_path.parent.mkdir(parents=True, exist_ok=True)
    with history_path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, sort_keys=True))
        handle.write("\n")


def resolve_bin(path_str: str) -> Path:
    path = Path(path_str)
    if path.exists() or path_str == "mmdr":
        return path
    return path


def build_release(bin_path: Path) -> None:
    if str(bin_path) == "mmdr":
        return
    target_release = ROOT / "target" / "release"
    should_build = not bin_path.exists()
    if not should_build:
        try:
            should_build = bin_path.resolve().is_relative_to(target_release.resolve())
        except ValueError:
            should_build = False
    if not should_build:
        return
    res = run(["cargo", "build", "--release"])
    if res.returncode != 0:
        raise RuntimeError(res.stderr.strip() or "cargo build failed")


def collect_fixtures(fixtures: list[Path], patterns: list[str], limit: int) -> list[Path]:
    files: list[Path] = []
    for base in fixtures:
        if base.is_file() and base.suffix == ".mmd":
            files.append(base)
            continue
        if base.exists():
            files.extend(sorted(base.glob("**/*.mmd")))
    if patterns:
        rx = [re.compile(pattern) for pattern in patterns]
        files = [path for path in files if any(r.search(str(path)) for r in rx)]
    if limit > 0:
        files = files[:limit]
    return files


def parse_timing(stderr: str) -> dict[str, Any] | None:
    for line in reversed(stderr.strip().splitlines()):
        line = line.strip()
        if not line:
            continue
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(payload, dict) and {"parse_us", "layout_us", "render_us", "total_us"}.issubset(payload):
            return payload
    return None


def safe_num(value: Any) -> float:
    try:
        number = float(value)
    except (TypeError, ValueError):
        return 0.0
    if not math.isfinite(number):
        return 0.0
    return number


def correlation(values_a: list[float], values_b: list[float]) -> float:
    if len(values_a) != len(values_b) or len(values_a) < 2:
        return 0.0
    mean_a = statistics.mean(values_a)
    mean_b = statistics.mean(values_b)
    std_a = statistics.pstdev(values_a)
    std_b = statistics.pstdev(values_b)
    if std_a < 1e-9 or std_b < 1e-9:
        return 0.0
    cov = sum((a - mean_a) * (b - mean_b) for a, b in zip(values_a, values_b)) / len(values_a)
    return cov / (std_a * std_b)


def derive_weight_model(
    rows: list[dict[str, Any]],
    metric_keys: list[str],
    mode: str,
) -> dict[str, Any]:
    stats: dict[str, dict[str, float]] = {}
    normalized: dict[str, list[float]] = {}
    active_keys: list[str] = []

    for key in metric_keys:
        vals = [safe_num(row.get("metrics", {}).get(key, 0.0)) for row in rows]
        if not vals:
            continue
        lo = min(vals)
        hi = max(vals)
        span = hi - lo
        stats[key] = {"min": lo, "max": hi}
        if span < 1e-9:
            normalized[key] = [0.0 for _ in vals]
            continue
        nvals = [(v - lo) / span for v in vals]
        normalized[key] = nvals
        active_keys.append(key)

    if not active_keys:
        return {
            "mode": mode,
            "metrics": [],
            "weights": {},
            "normalization": stats,
            "raw_importance": {},
        }

    if mode == "manual":
        raw = {key: safe_num(MANUAL_WEIGHTS.get(key, 1.0)) for key in active_keys}
    else:
        # CRITIC-style data-driven importance:
        # high variance and low correlation to other metrics -> higher weight.
        raw = {}
        for key in active_keys:
            vals = normalized[key]
            std = statistics.pstdev(vals)
            contrast = 0.0
            for other in active_keys:
                if other == key:
                    continue
                corr = correlation(vals, normalized[other])
                contrast += 1.0 - abs(corr)
            raw[key] = std * max(contrast, 1e-6)

    total = sum(raw.values())
    if total <= 1e-12:
        eq = 1.0 / len(active_keys)
        weights = {key: eq for key in active_keys}
    else:
        weights = {key: val / total for key, val in raw.items()}

    return {
        "mode": mode,
        "metrics": active_keys,
        "weights": weights,
        "normalization": stats,
        "raw_importance": raw,
    }


def apply_priority_scores(results: list[dict[str, Any]], model: dict[str, Any]) -> None:
    metrics = model.get("metrics", [])
    weights = model.get("weights", {})
    norm = model.get("normalization", {})
    for item in results:
        if "error" in item:
            continue
        row = item.get("metrics", {})
        contrib: dict[str, float] = {}
        score = 0.0
        for key in metrics:
            value = safe_num(row.get(key, 0.0))
            n = norm.get(key, {})
            lo = safe_num(n.get("min", 0.0))
            hi = safe_num(n.get("max", 0.0))
            span = hi - lo
            if span < 1e-9:
                nv = 0.0
            else:
                nv = (value - lo) / span
            nv = max(0.0, min(1.0, nv))
            w = safe_num(weights.get(key, 0.0))
            term = w * nv
            contrib[key] = term
            score += term

        timing = item.get("timing", {})
        layout_ms = max(safe_num(timing.get("layout_ms", 0.0)), 0.1)
        item["priority"] = {
            "pain_score": score,
            "pain_per_layout_ms": score / layout_ms,
            "hard_violations": (
                int(safe_num(row.get("edge_crossings", 0.0)) > 0)
                + int(safe_num(row.get("edge_node_crossings", 0.0)) > 0)
                + int(safe_num(row.get("node_overlap_count", 0.0)) > 0)
                + int(safe_num(row.get("label_overlap_count", 0.0)) > 0)
            ),
            "components": contrib,
        }


def print_weight_model(model: dict[str, Any], top_n: int = 10) -> None:
    mode = model.get("mode", "unknown")
    weights = model.get("weights", {})
    if not weights:
        print("Priority model: no active metrics")
        return
    ranked = sorted(weights.items(), key=lambda item: item[1], reverse=True)
    print(f"Priority model: {mode} ({len(ranked)} active metrics)")
    print("Top metric weights:")
    for key, weight in ranked[:top_n]:
        print(f"  {key}: {weight:.4f}")
    if len(ranked) > top_n:
        print(f"  ... {len(ranked) - top_n} more")
    print()


def summarize_timing(samples: list[dict[str, Any]]) -> dict[str, float]:
    parse_ms = [sample["parse_us"] / 1000.0 for sample in samples]
    layout_ms = [sample["layout_us"] / 1000.0 for sample in samples]
    render_ms = [sample["render_us"] / 1000.0 for sample in samples]
    total_ms = [sample["total_us"] / 1000.0 for sample in samples]
    return {
        "parse_ms": statistics.mean(parse_ms),
        "layout_ms": statistics.mean(layout_ms),
        "render_ms": statistics.mean(render_ms),
        "total_ms": statistics.mean(total_ms),
        "layout_ms_std": statistics.pstdev(layout_ms) if len(layout_ms) > 1 else 0.0,
        "total_ms_std": statistics.pstdev(total_ms) if len(total_ms) > 1 else 0.0,
    }


def benchmark_fixture(
    fixture: Path,
    bin_path: Path,
    config_path: Path,
    runs: int,
    warmup: int,
    layout_score_mod,
    quality_bench_mod,
) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="priority-bench-") as tmp_dir:
        tmp = Path(tmp_dir)
        svg_path = tmp / "out.svg"
        layout_path = tmp / "layout.json"
        cmd = [
            str(bin_path),
            "-i",
            str(fixture),
            "-o",
            str(svg_path),
            "-e",
            "svg",
            "--dumpLayout",
            str(layout_path),
            "--timing",
        ]
        if config_path.exists():
            cmd += ["-c", str(config_path)]

        timings: list[dict[str, Any]] = []
        for idx in range(warmup + runs):
            result = run(cmd)
            if result.returncode != 0:
                return {
                    "fixture": str(fixture),
                    "error": result.stderr.strip()[:300] or "render failed",
                }
            timing = parse_timing(result.stderr)
            if timing is None:
                return {
                    "fixture": str(fixture),
                    "error": "missing --timing payload",
                }
            if idx >= warmup:
                timings.append(timing)

        data, nodes, edges = layout_score_mod.load_layout(layout_path)
        metrics = layout_score_mod.compute_metrics(data, nodes, edges)
        metrics["score"] = layout_score_mod.weighted_score(metrics)
        try:
            label_metrics = quality_bench_mod.compute_label_metrics(svg_path, nodes, edges)
            metrics.update(label_metrics)
        except Exception:
            # Keep benchmark resilient if label parsing fails on a fixture.
            pass
        timing_summary = summarize_timing(timings)

        return {
            "fixture": str(fixture),
            "metrics": metrics,
            "timing": timing_summary,
        }


def print_priorities(results: list[dict[str, Any]], top_n: int) -> None:
    ok = [entry for entry in results if "error" not in entry]
    failed = [entry for entry in results if "error" in entry]

    if not ok:
        print("No successful benchmarks.")
        if failed:
            print("Failures:")
            for item in failed:
                print(f"  {item['fixture']}: {item['error']}")
        return

    by_pain = sorted(ok, key=lambda entry: entry["priority"]["pain_score"], reverse=True)
    by_roi = sorted(ok, key=lambda entry: entry["priority"]["pain_per_layout_ms"], reverse=True)

    print(f"Benchmarked {len(ok)} fixtures ({len(failed)} failed)")
    print()
    print(f"Top {top_n} by quality pain:")
    for idx, item in enumerate(by_pain[:top_n], start=1):
        metrics = item["metrics"]
        timing = item["timing"]
        priority = item["priority"]
        print(
            f"{idx}. {item['fixture']}  "
            f"pain={priority['pain_score']:.3f}  "
            f"layout={timing['layout_ms']:.2f}ms  "
            f"cross={metrics['edge_crossings']}  "
            f"edge-node={metrics['edge_node_crossings']}  "
            f"overlap={metrics['node_overlap_count']}  "
            f"bends={metrics['edge_bends']}  "
            f"ports={metrics['port_congestion']}  "
            f"lbl-overlap={metrics.get('label_overlap_count', 0)}  "
            f"lbl-edge={metrics.get('label_edge_overlap_count', 0)}  "
            f"lbl-oob={metrics.get('label_out_of_bounds_count', 0)}  "
            f"waste={metrics.get('wasted_space_ratio', 0.0):.2f}  "
            f"comp-gap={metrics.get('component_gap_ratio', 0.0):.2f}  "
            f"fill={metrics.get('content_fill_ratio', 0.0):.2f}  "
            f"detour={metrics.get('avg_edge_detour_ratio', 1.0):.2f}"
        )

    print()
    print(f"Top {top_n} quick wins (pain/layout-ms):")
    for idx, item in enumerate(by_roi[:top_n], start=1):
        priority = item["priority"]
        timing = item["timing"]
        print(
            f"{idx}. {item['fixture']}  "
            f"pain/ms={priority['pain_per_layout_ms']:.3f}  "
            f"pain={priority['pain_score']:.3f}  "
            f"layout={timing['layout_ms']:.2f}ms"
        )

    by_space = sorted(
        ok,
        key=lambda entry: (
            safe_num(entry["metrics"].get("wasted_space_ratio", 0.0))
            + safe_num(entry["metrics"].get("component_gap_ratio", 0.0))
            + safe_num(entry["metrics"].get("content_center_offset_ratio", 0.0))
            + safe_num(entry["metrics"].get("content_overflow_ratio", 0.0))
        ),
        reverse=True,
    )
    print()
    print(f"Top {top_n} by space inefficiency:")
    for idx, item in enumerate(by_space[:top_n], start=1):
        metrics = item["metrics"]
        timing = item["timing"]
        print(
            f"{idx}. {item['fixture']}  "
            f"space_stress={safe_num(metrics.get('wasted_space_ratio', 0.0)) + safe_num(metrics.get('component_gap_ratio', 0.0)) + safe_num(metrics.get('content_center_offset_ratio', 0.0)) + safe_num(metrics.get('content_overflow_ratio', 0.0)):.3f}  "
            f"wasted={metrics.get('wasted_space_ratio', 0.0):.2f}  "
            f"comp-gap={metrics.get('component_gap_ratio', 0.0):.2f}  "
            f"center={metrics.get('content_center_offset_ratio', 0.0):.2f}  "
            f"overflow={metrics.get('content_overflow_ratio', 0.0):.2f}  "
            f"fill={metrics.get('content_fill_ratio', 0.0):.2f}  "
            f"imbalance={metrics.get('margin_imbalance_ratio', 0.0):.2f}  "
            f"layout={timing['layout_ms']:.2f}ms"
        )

    if failed:
        print()
        print("Failures:")
        for item in failed:
            print(f"- {item['fixture']}: {item['error']}")


def summarize_priority_history(results: list[dict[str, Any]]) -> dict[str, Any]:
    ok = [entry for entry in results if "error" not in entry]
    failed = [entry for entry in results if "error" in entry]

    pain_scores = [safe_num(entry.get("priority", {}).get("pain_score", 0.0)) for entry in ok]
    roi_scores = [safe_num(entry.get("priority", {}).get("pain_per_layout_ms", 0.0)) for entry in ok]
    layout_ms = [safe_num(entry.get("timing", {}).get("layout_ms", 0.0)) for entry in ok]

    top_pain = max(
        ok,
        key=lambda entry: safe_num(entry.get("priority", {}).get("pain_score", 0.0)),
        default=None,
    )
    top_roi = max(
        ok,
        key=lambda entry: safe_num(entry.get("priority", {}).get("pain_per_layout_ms", 0.0)),
        default=None,
    )

    pain_p95 = 0.0
    if pain_scores:
        if len(pain_scores) == 1:
            pain_p95 = pain_scores[0]
        else:
            pain_p95 = statistics.quantiles(pain_scores, n=20, method="inclusive")[18]

    return {
        "fixture_count": len(results),
        "success_count": len(ok),
        "failure_count": len(failed),
        "pain_score_mean": statistics.mean(pain_scores) if pain_scores else 0.0,
        "pain_score_p95": pain_p95,
        "pain_per_layout_ms_mean": statistics.mean(roi_scores) if roi_scores else 0.0,
        "layout_ms_mean": statistics.mean(layout_ms) if layout_ms else 0.0,
        "top_pain_fixture": top_pain.get("fixture") if top_pain else "",
        "top_pain_score": safe_num(top_pain.get("priority", {}).get("pain_score", 0.0)) if top_pain else 0.0,
        "top_roi_fixture": top_roi.get("fixture") if top_roi else "",
        "top_roi_score": safe_num(top_roi.get("priority", {}).get("pain_per_layout_ms", 0.0)) if top_roi else 0.0,
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Rank layout improvement priorities from quality + timing benchmarks"
    )
    parser.add_argument(
        "--fixtures",
        action="append",
        default=[],
        help="Fixture files or directories (repeatable). Default: tests/fixtures + benches/fixtures",
    )
    parser.add_argument(
        "--bin",
        default=str(ROOT / "target" / "release" / "mmdr"),
        help="Path to mmdr binary",
    )
    parser.add_argument(
        "--config",
        default=str(ROOT / "tests" / "fixtures" / "modern-config.json"),
        help="Config JSON path",
    )
    parser.add_argument("--runs", type=int, default=3, help="Measured runs per fixture")
    parser.add_argument("--warmup", type=int, default=1, help="Warmup runs per fixture")
    parser.add_argument(
        "--weight-mode",
        choices=["auto", "manual"],
        default="auto",
        help="priority weighting model (default: auto)",
    )
    parser.add_argument(
        "--metric",
        action="append",
        default=[],
        help="metric key to include in priority model (repeatable)",
    )
    parser.add_argument(
        "--pattern",
        action="append",
        default=[],
        help="Regex filter for fixture path (repeatable)",
    )
    parser.add_argument("--limit", type=int, default=0, help="Limit fixture count")
    parser.add_argument("--top", type=int, default=10, help="Top rows to print")
    parser.add_argument(
        "--output-json",
        default=str(ROOT / "target" / "priority-bench.json"),
        help="Path for machine-readable report JSON",
    )
    parser.add_argument(
        "--history-log",
        default=str(ROOT / "tmp" / "benchmark-history" / "priority-runs.jsonl"),
        help="Path to append benchmark run history JSONL",
    )
    parser.add_argument(
        "--no-history-log",
        action="store_true",
        help="disable benchmark history JSONL logging for this run",
    )
    args = parser.parse_args()

    fixture_roots = [Path(path) for path in args.fixtures if path]
    if not fixture_roots:
        fixture_roots = [ROOT / "tests" / "fixtures", ROOT / "benches" / "fixtures"]

    fixtures = collect_fixtures(fixture_roots, args.pattern, args.limit)
    if not fixtures:
        raise SystemExit("No fixtures found")

    bin_path = resolve_bin(args.bin)
    build_release(bin_path)
    config_path = Path(args.config)

    layout_score_mod = load_layout_score()
    quality_bench_mod = load_quality_bench()

    results: list[dict[str, Any]] = []
    for fixture in fixtures:
        results.append(
            benchmark_fixture(
                fixture=fixture,
                bin_path=bin_path,
                config_path=config_path,
                runs=args.runs,
                warmup=args.warmup,
                layout_score_mod=layout_score_mod,
                quality_bench_mod=quality_bench_mod,
            )
        )

    metric_keys = args.metric if args.metric else DEFAULT_PRIORITY_METRICS
    ok_results = [entry for entry in results if "error" not in entry]
    model = derive_weight_model(ok_results, metric_keys, mode=args.weight_mode)
    apply_priority_scores(results, model)

    output_path = Path(args.output_json)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "weight_model": model,
        "metric_keys_requested": metric_keys,
        "fixtures": [str(path) for path in fixtures],
        "results": results,
    }
    output_path.write_text(json.dumps(payload, indent=2))

    print_weight_model(model, top_n=12)
    print_priorities(results, top_n=max(args.top, 1))
    print()
    print(f"Wrote {output_path}")

    if not args.no_history_log:
        history_path = Path(args.history_log)
        record = {
            "timestamp_utc": iso_utc_now(),
            "history_version": 1,
            "tool": "priority_bench",
            "cwd": str(ROOT),
            "argv": sys.argv[1:],
            "git": git_metadata(),
            "host": host_metadata(),
            "settings": {
                "bin": str(bin_path),
                "config": str(config_path),
                "runs": args.runs,
                "warmup": args.warmup,
                "weight_mode": args.weight_mode,
                "metrics_requested": metric_keys,
                "patterns": args.pattern,
                "fixture_roots": [str(path) for path in fixture_roots],
                "fixture_limit": args.limit,
                "output_json": str(output_path),
            },
            "summary": summarize_priority_history(results),
        }
        append_benchmark_history(history_path, record)
        print(f"Wrote history: {history_path}")


if __name__ == "__main__":
    main()
