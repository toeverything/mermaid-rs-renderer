#!/usr/bin/env python3
import argparse
import importlib.util
import json
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


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
    cmd = ["cargo", "build", "--release"]
    res = run(cmd)
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


def main():
    parser = argparse.ArgumentParser(description="Compute layout quality metrics")
    parser.add_argument(
        "--fixtures",
        action="append",
        default=[],
        help="fixture dir (repeatable). default: tests/fixtures, benches/fixtures",
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
        default=str(ROOT / "target" / "quality"),
        help="output directory",
    )
    parser.add_argument(
        "--output-json",
        default="",
        help="write metrics JSON to file (default: <out-dir>/quality.json)",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=0,
        help="limit number of fixtures",
    )
    parser.add_argument(
        "--pattern",
        action="append",
        default=[],
        help="regex pattern to filter fixture paths (repeatable)",
    )
    args = parser.parse_args()

    fixtures = [Path(p) for p in args.fixtures if p]
    if not fixtures:
        fixtures = [ROOT / "tests" / "fixtures", ROOT / "benches" / "fixtures"]

    bin_path = resolve_bin(args.bin)
    build_release(bin_path)

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    config_path = Path(args.config)
    config_args = []
    if config_path.exists():
        config_args = ["-c", str(config_path)]

    layout_score = load_layout_score()
    results = {}

    files = []
    patterns = [re.compile(p) for p in args.pattern] if args.pattern else []
    for base in fixtures:
        if base.exists():
            files.extend(sorted(base.glob("**/*.mmd")))
    if args.limit:
        files = files[: args.limit]
    if patterns:
        files = [f for f in files if any(p.search(str(f)) for p in patterns)]

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

    output_json = Path(args.output_json) if args.output_json else out_dir / "quality.json"
    output_json.write_text(json.dumps(results, indent=2))

    scored = [(k, v) for k, v in results.items() if "score" in v]
    if scored:
        scores = sorted(scored, key=lambda kv: kv[1]["score"], reverse=True)
        top = scores[:5]
        avg = sum(v["score"] for _, v in scored) / len(scored)
        print(f"Wrote {output_json}")
        print(f"Fixtures: {len(scored)}  Avg score: {avg:.2f}")
        print("Worst 5 by score:")
        for name, metrics in top:
            print(f"  {name}: {metrics['score']:.2f}")
    else:
        print(f"Wrote {output_json}")


if __name__ == "__main__":
    main()
