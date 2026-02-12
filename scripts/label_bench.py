#!/usr/bin/env python3
import argparse
import importlib.util
import json
import os
import re
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_quality_bench():
    module_path = ROOT / "scripts" / "quality_bench.py"
    spec = importlib.util.spec_from_file_location("quality_bench", module_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # type: ignore[call-arg]
    return module


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


def summarize(results):
    rows = [
        v
        for v in results.values()
        if isinstance(v, dict) and "edge_label_path_gap_mean" in v
    ]
    if not rows:
        return {}
    def avg(key):
        vals = [float(v.get(key, 0.0)) for v in rows]
        return sum(vals) / len(vals)

    return {
        "fixtures": len(rows),
        "avg_gap_mean": avg("edge_label_path_gap_mean"),
        "avg_gap_p95": avg("edge_label_path_gap_p95"),
        "avg_touch_ratio": avg("edge_label_path_touch_ratio"),
        "avg_bad_ratio": avg("edge_label_path_gap_bad_ratio"),
    }


def compare_metric(left, right, keys, metric, higher_is_better=False, eps=1e-9):
    better = 0
    equal = 0
    worse = 0
    regressions = []
    for key in keys:
        lval = left.get(key, {}).get(metric)
        rval = right.get(key, {}).get(metric)
        if not isinstance(lval, (int, float)) or not isinstance(rval, (int, float)):
            continue
        delta = lval - rval
        if higher_is_better:
            if delta > eps:
                better += 1
            elif delta < -eps:
                worse += 1
                regressions.append((-delta, key, lval, rval))
            else:
                equal += 1
        else:
            if delta < -eps:
                better += 1
            elif delta > eps:
                worse += 1
                regressions.append((delta, key, lval, rval))
            else:
                equal += 1
    regressions.sort(reverse=True, key=lambda item: item[0])
    return better, equal, worse, regressions


def main():
    parser = argparse.ArgumentParser(
        description="Benchmark edge-label placement by path gap (0 means touching path)"
    )
    parser.add_argument(
        "--fixtures",
        action="append",
        default=[],
        help="fixture dir (repeatable). default: tests/fixtures, benches/fixtures, docs/comparison_sources",
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
        "--out-dir",
        default=str(ROOT / "target" / "label-bench"),
        help="output directory",
    )
    parser.add_argument(
        "--output-json",
        default="",
        help="write metrics JSON to file (default: <out-dir>/label-compare.json)",
    )
    parser.add_argument(
        "--engine",
        choices=["mmdr", "mmdc", "both"],
        default="both",
        help="layout engine to benchmark (default: both)",
    )
    parser.add_argument(
        "--mmdc",
        default=os.environ.get("MMD_CLI", "npx -y @mermaid-js/mermaid-cli"),
        help="mermaid-cli command (default: env MMD_CLI or npx -y @mermaid-js/mermaid-cli)",
    )
    parser.add_argument("--limit", type=int, default=0, help="limit number of fixtures")
    parser.add_argument(
        "--pattern",
        action="append",
        default=[],
        help="regex pattern to filter fixture paths (repeatable)",
    )
    parser.add_argument(
        "--history-log",
        default=str(ROOT / "tmp" / "benchmark-history" / "label-runs.jsonl"),
        help="append run summary metadata to this JSONL path",
    )
    parser.add_argument(
        "--no-history-log",
        action="store_true",
        help="disable benchmark history JSONL logging for this run",
    )
    args = parser.parse_args()
    run_started_epoch = time.time()
    run_started_at = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())

    qb = load_quality_bench()
    fixtures = [Path(p) for p in args.fixtures if p]
    if not fixtures:
        fixtures = [
            ROOT / "tests" / "fixtures",
            ROOT / "benches" / "fixtures",
            ROOT / "docs" / "comparison_sources",
        ]
    files = collect_fixtures(fixtures, args.limit, args.pattern)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    config_path = Path(args.config)

    results = {}
    if args.engine in {"mmdr", "both"}:
        bin_path = qb.resolve_bin(args.bin)
        qb.build_release(bin_path)
        results["mmdr"] = qb.compute_mmdr_metrics(files, bin_path, config_path, out_dir)
    if args.engine in {"mmdc", "both"}:
        results["mermaid_cli"] = qb.compute_mmdc_metrics(files, args.mmdc, config_path, out_dir)

    if args.engine == "mmdr":
        payload = results["mmdr"]
        output_json = Path(args.output_json) if args.output_json else out_dir / "label-mmdr.json"
    elif args.engine == "mmdc":
        payload = results["mermaid_cli"]
        output_json = Path(args.output_json) if args.output_json else out_dir / "label-mermaid-cli.json"
    else:
        payload = results
        output_json = Path(args.output_json) if args.output_json else out_dir / "label-compare.json"
    output_json.write_text(json.dumps(payload, indent=2))
    print(f"Wrote {output_json}")

    if args.engine in {"mmdr", "both"}:
        summary = summarize(results["mmdr"])
        if summary:
            print(
                "mmdr label gap: "
                f"fixtures={summary['fixtures']} "
                f"avg_gap_mean={summary['avg_gap_mean']:.3f} "
                f"avg_gap_p95={summary['avg_gap_p95']:.3f} "
                f"avg_touch_ratio={summary['avg_touch_ratio']:.3f} "
                f"avg_bad_ratio={summary['avg_bad_ratio']:.3f}"
            )
        mmdr_summary = summary
    else:
        mmdr_summary = {}
    if args.engine in {"mmdc", "both"}:
        summary = summarize(results["mermaid_cli"])
        if summary:
            print(
                "mermaid-cli label gap: "
                f"fixtures={summary['fixtures']} "
                f"avg_gap_mean={summary['avg_gap_mean']:.3f} "
                f"avg_gap_p95={summary['avg_gap_p95']:.3f} "
                f"avg_touch_ratio={summary['avg_touch_ratio']:.3f} "
                f"avg_bad_ratio={summary['avg_bad_ratio']:.3f}"
            )
        mmdc_summary = summary
    else:
        mmdc_summary = {}
    compare_stats = {}
    if args.engine == "both":
        common = qb.collect_common_scored(results["mmdr"], results["mermaid_cli"])
        compare_stats["common_scored_fixtures"] = len(common)
        compare_stats["metrics"] = {}
        for metric in [
            "edge_label_path_gap_mean",
            "edge_label_path_gap_p95",
            "edge_label_path_touch_ratio",
            "edge_label_path_gap_bad_ratio",
        ]:
            higher_is_better = metric == "edge_label_path_touch_ratio"
            better, equal, worse, regressions = compare_metric(
                results["mmdr"],
                results["mermaid_cli"],
                common,
                metric,
                higher_is_better=higher_is_better,
            )
            metric_stats = {
                "better": better,
                "equal": equal,
                "worse": worse,
            }
            print(
                f"mmdr vs mermaid-cli `{metric}`: better {better}, equal {equal}, worse {worse}"
            )
            if regressions:
                top = regressions[0]
                metric_stats["worst_regression"] = {
                    "fixture": top[1],
                    "fixture_name": Path(top[1]).name,
                    "mmdr": float(top[2]),
                    "mermaid_cli": float(top[3]),
                }
                print(
                    "  worst regression: "
                    f"{Path(top[1]).name} (mmdr={top[2]:.3f}, mermaid-cli={top[3]:.3f})"
                )
            compare_stats["metrics"][metric] = metric_stats

    if not args.no_history_log:
        record = {
            "timestamp_utc": run_started_at,
            "completed_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "duration_sec": max(0.0, time.time() - run_started_epoch),
            "history_version": 1,
            "command": {
                "argv": sys.argv,
                "engine": args.engine,
                "fixtures": [str(path) for path in fixtures],
                "pattern": args.pattern,
                "limit": args.limit,
                "out_dir": str(out_dir),
                "output_json": str(output_json),
                "config": str(config_path),
                "bin": args.bin,
            },
            "host": qb.host_metadata(),
            "git": qb.git_metadata(),
            "summary": {
                "mmdr": mmdr_summary,
                "mermaid_cli": mmdc_summary,
            },
        }
        if compare_stats:
            record["comparison"] = compare_stats
        qb.append_benchmark_history(Path(args.history_log), record)


if __name__ == "__main__":
    main()
