#!/usr/bin/env python3
import argparse
import copy
import json
import random
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def run(cmd):
    return subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def load_base_config(path: Path):
    return json.loads(path.read_text())


def clamp(val, lo, hi):
    return max(lo, min(hi, val))


def make_buckets(scales, mins):
    return [{"minNodes": m, "scale": s} for m, s in zip(mins, scales)]


def random_scales(rng, base_scales, min_scale=0.25, max_scale=1.0, jitter=0.08):
    scales = [base_scales[0]]
    prev = base_scales[0]
    for s in base_scales[1:]:
        delta = rng.uniform(-jitter, jitter)
        cand = clamp(s + delta, min_scale, prev)
        scales.append(cand)
        prev = cand
    return scales


def score_for_config(config_path: Path, pattern: str):
    cmd = [
        "python",
        str(ROOT / "scripts" / "quality_bench.py"),
        "--config",
        str(config_path),
        "--pattern",
        pattern,
    ]
    res = run(cmd)
    if res.returncode != 0:
        raise RuntimeError(res.stderr.strip() or "quality_bench failed")
    out_path = ROOT / "target" / "quality" / "quality.json"
    data = json.loads(out_path.read_text())
    scores = [v["score"] for v in data.values() if "score" in v]
    if not scores:
        return float("inf")
    return sum(scores) / len(scores)


def main():
    parser = argparse.ArgumentParser(description="Tune flowchart auto spacing buckets")
    parser.add_argument(
        "--config",
        default=str(ROOT / "tests" / "fixtures" / "modern-config.json"),
        help="base config JSON",
    )
    parser.add_argument(
        "--iterations",
        type=int,
        default=20,
        help="random search iterations",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=7,
        help="random seed",
    )
    parser.add_argument(
        "--pattern",
        default="flowchart",
        help="regex pattern for fixtures",
    )
    parser.add_argument(
        "--out",
        default=str(ROOT / "target" / "tuned-flowchart-config.json"),
        help="output config path",
    )
    args = parser.parse_args()

    base_path = Path(args.config)
    base = load_base_config(base_path)
    flow = base.setdefault("flowchart", {})
    auto = flow.setdefault("autoSpacing", {})

    base_buckets = auto.get(
        "buckets",
        [
            {"minNodes": 0, "scale": 1.0},
            {"minNodes": 50, "scale": 0.75},
            {"minNodes": 80, "scale": 0.6},
            {"minNodes": 120, "scale": 0.45},
            {"minNodes": 160, "scale": 0.3},
        ],
    )

    mins = [b["minNodes"] for b in base_buckets]
    base_scales = [b["scale"] for b in base_buckets]

    rng = random.Random(args.seed)

    best_score = float("inf")
    best_scales = base_scales

    for i in range(args.iterations):
        scales = random_scales(rng, base_scales)
        candidate = copy.deepcopy(base)
        candidate_flow = candidate.setdefault("flowchart", {})
        candidate_auto = candidate_flow.setdefault("autoSpacing", {})
        candidate_auto["enabled"] = True
        candidate_auto.setdefault("minSpacing", 24)
        candidate_auto.setdefault("densityThreshold", 1.5)
        candidate_auto.setdefault("denseScaleFloor", 0.7)
        candidate_auto["buckets"] = make_buckets(scales, mins)

        tmp_path = ROOT / "target" / "tune-config.json"
        tmp_path.write_text(json.dumps(candidate, indent=2))
        score = score_for_config(tmp_path, args.pattern)
        print(f"iter {i+1:02d}: score={score:.2f} scales={scales}")
        if score < best_score:
            best_score = score
            best_scales = scales

    final = copy.deepcopy(base)
    final_flow = final.setdefault("flowchart", {})
    final_auto = final_flow.setdefault("autoSpacing", {})
    final_auto["enabled"] = True
    final_auto.setdefault("minSpacing", 24)
    final_auto.setdefault("densityThreshold", 1.5)
    final_auto.setdefault("denseScaleFloor", 0.7)
    final_auto["buckets"] = make_buckets(best_scales, mins)

    out_path = Path(args.out)
    out_path.write_text(json.dumps(final, indent=2))
    print(f"Best score: {best_score:.2f}")
    print(f"Wrote tuned config: {out_path}")


if __name__ == "__main__":
    main()
