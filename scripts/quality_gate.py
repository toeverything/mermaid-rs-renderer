#!/usr/bin/env python3
import argparse
import importlib.util
import json
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


STRICT_METRICS = {
    "edge_crossings",
    "edge_node_crossings",
    "node_overlap_count",
}

RELATIVE_METRICS = {
    "total_edge_length",
    "edge_bends",
    "port_congestion",
    "edge_overlap_length",
    "layout_area",
    "node_overlap_area",
    "score",
}


def load_layout_score():
    module_path = ROOT / "scripts" / "layout_score.py"
    spec = importlib.util.spec_from_file_location("layout_score", module_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # type: ignore[call-arg]
    return module


def run(cmd):
    return subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def resolve_bin(path_str: str) -> Path:
    path = Path(path_str)
    if path.exists():
        return path
    if path_str == "mmdr":
        return path
    return path


def build_release(bin_path: Path):
    if bin_path.exists():
        return
    res = run(["cargo", "build", "--release"])
    if res.returncode != 0:
        raise RuntimeError(res.stderr.strip() or "cargo build failed")


def layout_key(path: Path, base: Path) -> str:
    try:
        rel = path.relative_to(base)
    except ValueError:
        rel = path.name
    rel_no_ext = rel.with_suffix("")
    parts = [part.replace(" ", "_") for part in Path(rel_no_ext).parts]
    return "__".join(parts)


def collect_fixtures(fixtures, limit, patterns):
    files = []
    for base in fixtures:
        if base.exists():
            files.extend(sorted(base.glob("**/*.mmd")))
    if limit:
        files = files[:limit]
    if patterns:
        rx = [re.compile(p) for p in patterns]
        files = [f for f in files if any(r.search(str(f)) for r in rx)]
    return files


def compute_metrics(files, bin_path, config_path, out_dir):
    layout_score = load_layout_score()
    out_dir.mkdir(parents=True, exist_ok=True)
    config_args = ["-c", str(config_path)] if config_path.exists() else []
    results = {}
    for file in files:
        key = layout_key(file, ROOT)
        layout_path = out_dir / f"{key}-layout.json"
        svg_path = out_dir / f"{key}.svg"
        for path in (layout_path, svg_path):
            if path.exists():
                path.unlink()
        cmd = [
            str(bin_path),
            "-i",
            str(file),
            "-o",
            str(svg_path),
            "-e",
            "svg",
            "--dumpLayout",
            str(layout_path),
        ] + config_args
        res = run(cmd)
        if res.returncode != 0:
            results[str(file)] = {"error": res.stderr.strip()[:200]}
            continue
        data, nodes, edges = layout_score.load_layout(layout_path)
        metrics = layout_score.compute_metrics(data, nodes, edges)
        metrics["score"] = layout_score.weighted_score(metrics)
        results[str(file)] = metrics
    return results


def compare_metrics(baseline, current, rel_tol, abs_tol):
    regressions = []
    for metric, base_val in baseline.items():
        if metric == "error":
            continue
        cur_val = current.get(metric)
        if cur_val is None:
            continue
        if metric in STRICT_METRICS:
            if cur_val > base_val:
                regressions.append((metric, base_val, cur_val, "strict"))
        elif metric in RELATIVE_METRICS and isinstance(base_val, (int, float)):
            limit = max(base_val * (1.0 + rel_tol), base_val + abs_tol)
            if cur_val > limit:
                regressions.append((metric, base_val, cur_val, f"> {limit:.2f}"))
    return regressions


def main():
    parser = argparse.ArgumentParser(description="Gate layout quality against a baseline")
    parser.add_argument(
        "--baseline",
        default=str(ROOT / "tests" / "quality_baseline.json"),
        help="baseline JSON file",
    )
    parser.add_argument(
        "--config",
        default=str(ROOT / "tests" / "fixtures" / "modern-config.json"),
        help="config JSON for mmdr",
    )
    parser.add_argument(
        "--bin",
        default=str(ROOT / "target" / "release" / "mmdr"),
        help="mmdr binary path",
    )
    parser.add_argument(
        "--fixtures",
        action="append",
        default=[],
        help="fixture dir (repeatable). default: tests/fixtures, benches/fixtures",
    )
    parser.add_argument("--limit", type=int, default=0, help="limit number of fixtures")
    parser.add_argument(
        "--pattern",
        action="append",
        default=[],
        help="regex pattern to filter fixture paths (repeatable)",
    )
    parser.add_argument(
        "--rel-tol",
        type=float,
        default=0.10,
        help="relative tolerance for soft metrics (default 0.10)",
    )
    parser.add_argument(
        "--abs-tol",
        type=float,
        default=1.0,
        help="absolute tolerance for soft metrics (default 1.0)",
    )
    parser.add_argument(
        "--write-baseline",
        action="store_true",
        help="write baseline file instead of gating",
    )
    args = parser.parse_args()

    fixtures = [Path(p) for p in args.fixtures if p]
    if not fixtures:
        fixtures = [ROOT / "tests" / "fixtures", ROOT / "benches" / "fixtures"]

    files = collect_fixtures(fixtures, args.limit, args.pattern)
    if not files:
        print("No fixtures found.", file=sys.stderr)
        return 2

    bin_path = resolve_bin(args.bin)
    build_release(bin_path)

    config_path = Path(args.config)
    out_dir = ROOT / "target" / "quality-gate"
    metrics = compute_metrics(files, bin_path, config_path, out_dir)

    baseline_path = Path(args.baseline)
    if args.write_baseline:
        payload = {
            "config": str(config_path),
            "fixtures": [str(f) for f in files],
            "metrics": metrics,
        }
        baseline_path.write_text(json.dumps(payload, indent=2))
        print(f"Wrote baseline: {baseline_path}")
        return 0

    if not baseline_path.exists():
        print(f"Baseline not found: {baseline_path}", file=sys.stderr)
        return 2

    baseline = json.loads(baseline_path.read_text())
    baseline_metrics = baseline.get("metrics", {})
    failures = []
    for fixture, base_metrics in baseline_metrics.items():
        cur_metrics = metrics.get(fixture)
        if cur_metrics is None:
            failures.append((fixture, "missing", "", ""))
            continue
        if "error" in cur_metrics:
            failures.append((fixture, "error", "", cur_metrics.get("error")))
            continue
        regressions = compare_metrics(base_metrics, cur_metrics, args.rel_tol, args.abs_tol)
        for metric, base_val, cur_val, limit in regressions:
            failures.append((fixture, metric, base_val, cur_val))

    if failures:
        print("Layout quality regressions detected:", file=sys.stderr)
        for fixture, metric, base_val, cur_val in failures:
            print(f"  {fixture}: {metric} baseline={base_val} current={cur_val}", file=sys.stderr)
        return 1

    print("Layout quality gate: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
