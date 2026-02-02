#!/usr/bin/env python3
import argparse
import importlib.util
import json
import math
import os
import re
import shlex
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TOKEN_RE = re.compile(r"[AaCcHhLlMmQqSsTtVvZz]|[-+]?(?:\d*\.\d+|\d+)(?:[eE][-+]?\d+)?")


def load_layout_score():
    module_path = ROOT / "scripts" / "layout_score.py"
    spec = importlib.util.spec_from_file_location("layout_score", module_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # type: ignore[call-arg]
    return module


def load_layout_diff():
    module_path = ROOT / "scripts" / "layout_diff.py"
    spec = importlib.util.spec_from_file_location("layout_diff", module_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # type: ignore[call-arg]
    return module


def run(cmd, env=None):
    return subprocess.run(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
    )


def find_puppeteer_chrome():
    base = Path.home() / ".cache" / "puppeteer" / "chrome"
    if not base.exists():
        return None
    candidates = sorted(base.glob("**/chrome"))
    return str(candidates[-1]) if candidates else None


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


def parse_transform(transform: str):
    if not transform:
        return 0.0, 0.0
    match = re.search(r"translate\(([^,\s]+)[,\s]+([^\)]+)\)", transform)
    if not match:
        return 0.0, 0.0
    return float(match.group(1)), float(match.group(2))


def strip_ns(tag: str) -> str:
    if "}" in tag:
        return tag.split("}", 1)[1]
    return tag


def parse_points(points: str):
    pts = []
    for part in points.replace(",", " ").split():
        try:
            pts.append(float(part))
        except ValueError:
            continue
    return list(zip(pts[0::2], pts[1::2]))


def parse_svg_number(value: str) -> float:
    if not value:
        return 0.0
    match = re.search(r"[-+]?(?:\d*\.\d+|\d+)", value)
    return float(match.group(0)) if match else 0.0


def cubic_point(p0, p1, p2, p3, t: float):
    it = 1.0 - t
    x = (
        it * it * it * p0[0]
        + 3.0 * it * it * t * p1[0]
        + 3.0 * it * t * t * p2[0]
        + t * t * t * p3[0]
    )
    y = (
        it * it * it * p0[1]
        + 3.0 * it * it * t * p1[1]
        + 3.0 * it * t * t * p2[1]
        + t * t * t * p3[1]
    )
    return (x, y)


def quad_point(p0, p1, p2, t: float):
    it = 1.0 - t
    x = it * it * p0[0] + 2.0 * it * t * p1[0] + t * t * p2[0]
    y = it * it * p0[1] + 2.0 * it * t * p1[1] + t * t * p2[1]
    return (x, y)


def parse_path_points(d: str, steps: int = 8):
    tokens = TOKEN_RE.findall(d)
    points = []
    if not tokens:
        return points
    idx = 0
    cmd = ""
    cur_x = 0.0
    cur_y = 0.0
    start_x = 0.0
    start_y = 0.0
    prev_ctrl = None
    prev_cmd = ""

    def add_point(pt):
        if not points:
            points.append(pt)
            return
        last = points[-1]
        if abs(last[0] - pt[0]) > 1e-4 or abs(last[1] - pt[1]) > 1e-4:
            points.append(pt)

    def read_float():
        nonlocal idx
        val = float(tokens[idx])
        idx += 1
        return val

    while idx < len(tokens):
        token = tokens[idx]
        if token.isalpha():
            cmd = token
            idx += 1
        if cmd in {"M", "m"}:
            first = True
            while idx + 1 < len(tokens) and not tokens[idx].isalpha():
                x = read_float()
                y = read_float()
                if cmd == "m":
                    x += cur_x
                    y += cur_y
                cur_x, cur_y = x, y
                if first:
                    start_x, start_y = x, y
                    add_point((cur_x, cur_y))
                    first = False
                else:
                    add_point((cur_x, cur_y))
                prev_ctrl = None
            prev_cmd = "M"
            continue
        if cmd in {"L", "l"}:
            while idx + 1 < len(tokens) and not tokens[idx].isalpha():
                x = read_float()
                y = read_float()
                if cmd == "l":
                    x += cur_x
                    y += cur_y
                cur_x, cur_y = x, y
                add_point((cur_x, cur_y))
            prev_ctrl = None
            prev_cmd = "L"
            continue
        if cmd in {"H", "h"}:
            while idx < len(tokens) and not tokens[idx].isalpha():
                x = read_float()
                if cmd == "h":
                    x += cur_x
                cur_x = x
                add_point((cur_x, cur_y))
            prev_ctrl = None
            prev_cmd = "H"
            continue
        if cmd in {"V", "v"}:
            while idx < len(tokens) and not tokens[idx].isalpha():
                y = read_float()
                if cmd == "v":
                    y += cur_y
                cur_y = y
                add_point((cur_x, cur_y))
            prev_ctrl = None
            prev_cmd = "V"
            continue
        if cmd in {"C", "c"}:
            while idx + 5 < len(tokens) and not tokens[idx].isalpha():
                x1 = read_float()
                y1 = read_float()
                x2 = read_float()
                y2 = read_float()
                x = read_float()
                y = read_float()
                if cmd == "c":
                    x1 += cur_x
                    y1 += cur_y
                    x2 += cur_x
                    y2 += cur_y
                    x += cur_x
                    y += cur_y
                p0 = (cur_x, cur_y)
                p1 = (x1, y1)
                p2 = (x2, y2)
                p3 = (x, y)
                for step in range(1, steps + 1):
                    t = step / steps
                    add_point(cubic_point(p0, p1, p2, p3, t))
                cur_x, cur_y = x, y
                prev_ctrl = (x2, y2)
            prev_cmd = "C"
            continue
        if cmd in {"S", "s"}:
            while idx + 3 < len(tokens) and not tokens[idx].isalpha():
                x2 = read_float()
                y2 = read_float()
                x = read_float()
                y = read_float()
                if cmd == "s":
                    x2 += cur_x
                    y2 += cur_y
                    x += cur_x
                    y += cur_y
                if prev_cmd in {"C", "S"} and prev_ctrl is not None:
                    x1 = 2.0 * cur_x - prev_ctrl[0]
                    y1 = 2.0 * cur_y - prev_ctrl[1]
                else:
                    x1 = cur_x
                    y1 = cur_y
                p0 = (cur_x, cur_y)
                p1 = (x1, y1)
                p2 = (x2, y2)
                p3 = (x, y)
                for step in range(1, steps + 1):
                    t = step / steps
                    add_point(cubic_point(p0, p1, p2, p3, t))
                cur_x, cur_y = x, y
                prev_ctrl = (x2, y2)
            prev_cmd = "S"
            continue
        if cmd in {"Q", "q"}:
            while idx + 3 < len(tokens) and not tokens[idx].isalpha():
                x1 = read_float()
                y1 = read_float()
                x = read_float()
                y = read_float()
                if cmd == "q":
                    x1 += cur_x
                    y1 += cur_y
                    x += cur_x
                    y += cur_y
                p0 = (cur_x, cur_y)
                p1 = (x1, y1)
                p2 = (x, y)
                for step in range(1, steps + 1):
                    t = step / steps
                    add_point(quad_point(p0, p1, p2, t))
                cur_x, cur_y = x, y
                prev_ctrl = (x1, y1)
            prev_cmd = "Q"
            continue
        if cmd in {"T", "t"}:
            while idx + 1 < len(tokens) and not tokens[idx].isalpha():
                x = read_float()
                y = read_float()
                if cmd == "t":
                    x += cur_x
                    y += cur_y
                if prev_cmd in {"Q", "T"} and prev_ctrl is not None:
                    x1 = 2.0 * cur_x - prev_ctrl[0]
                    y1 = 2.0 * cur_y - prev_ctrl[1]
                else:
                    x1 = cur_x
                    y1 = cur_y
                p0 = (cur_x, cur_y)
                p1 = (x1, y1)
                p2 = (x, y)
                for step in range(1, steps + 1):
                    t = step / steps
                    add_point(quad_point(p0, p1, p2, t))
                cur_x, cur_y = x, y
                prev_ctrl = (x1, y1)
            prev_cmd = "T"
            continue
        if cmd in {"A", "a"}:
            while idx + 6 < len(tokens) and not tokens[idx].isalpha():
                _rx = read_float()
                _ry = read_float()
                _rot = read_float()
                _laf = read_float()
                _sf = read_float()
                x = read_float()
                y = read_float()
                if cmd == "a":
                    x += cur_x
                    y += cur_y
                cur_x, cur_y = x, y
                add_point((cur_x, cur_y))
            prev_ctrl = None
            prev_cmd = "A"
            continue
        if cmd in {"Z", "z"}:
            cur_x, cur_y = start_x, start_y
            add_point((cur_x, cur_y))
            prev_ctrl = None
            prev_cmd = "Z"
            continue
        idx += 1

    return points


def parse_mermaid_edges(svg_path: Path):
    root = ET.fromstring(svg_path.read_text())
    edges = []

    def visit(elem, acc_tx, acc_ty, in_edge_group):
        tx, ty = parse_transform(elem.attrib.get("transform", ""))
        cur_tx = acc_tx + tx
        cur_ty = acc_ty + ty
        tag = strip_ns(elem.tag)
        cls = elem.attrib.get("class", "")
        is_edge_group = in_edge_group or ("edgePaths" in cls)

        if tag == "path":
            if is_edge_group or "edge" in cls or "edgePath" in cls:
                d = elem.attrib.get("d", "")
                points = parse_path_points(d)
                if points:
                    points = [(x + cur_tx, y + cur_ty) for x, y in points]
                    edges.append(points)
        elif tag == "polyline" and is_edge_group:
            pts = parse_points(elem.attrib.get("points", ""))
            if pts:
                points = [(x + cur_tx, y + cur_ty) for x, y in pts]
                edges.append(points)
        elif tag == "line" and is_edge_group:
            x1 = parse_svg_number(elem.attrib.get("x1", "0")) + cur_tx
            y1 = parse_svg_number(elem.attrib.get("y1", "0")) + cur_ty
            x2 = parse_svg_number(elem.attrib.get("x2", "0")) + cur_tx
            y2 = parse_svg_number(elem.attrib.get("y2", "0")) + cur_ty
            edges.append([(x1, y1), (x2, y2)])

        for child in list(elem):
            visit(child, cur_tx, cur_ty, is_edge_group)

    visit(root, 0.0, 0.0, False)
    return edges


def svg_size(root):
    view_box = root.attrib.get("viewBox", "")
    if view_box:
        parts = [p for p in view_box.replace(",", " ").split() if p]
        if len(parts) >= 4:
            return parse_svg_number(parts[2]), parse_svg_number(parts[3])
    width = parse_svg_number(root.attrib.get("width", "0"))
    height = parse_svg_number(root.attrib.get("height", "0"))
    return width, height


def match_endpoint(point, node_list):
    px, py = point
    best_id = None
    best_dist = None
    for node_id, node, cx, cy, pad in node_list:
        x = node["x"] - pad
        y = node["y"] - pad
        w = node["width"] + pad * 2.0
        h = node["height"] + pad * 2.0
        if px < x or px > x + w or py < y or py > y + h:
            continue
        dist = math.hypot(px - cx, py - cy)
        if best_dist is None or dist < best_dist:
            best_dist = dist
            best_id = node_id
    return best_id


def load_mermaid_svg_graph(svg_path: Path):
    layout_diff = load_layout_diff()
    nodes, _, _, _ = layout_diff.parse_mermaid_svg(svg_path)
    root = ET.fromstring(svg_path.read_text())
    width, height = svg_size(root)
    edge_paths = parse_mermaid_edges(svg_path)
    node_list = []
    for node_id, node in nodes.items():
        cx = node["x"] + node["width"] / 2.0
        cy = node["y"] + node["height"] / 2.0
        pad = max(6.0, min(node["width"], node["height"]) * 0.1)
        node_list.append((node_id, node, cx, cy, pad))

    edges = []
    for points in edge_paths:
        if len(points) < 2:
            continue
        from_id = match_endpoint(points[0], node_list)
        to_id = match_endpoint(points[-1], node_list)
        edges.append({"points": points, "from": from_id, "to": to_id})

    return {"width": width, "height": height}, nodes, edges

def layout_key(path: Path, base: Path) -> str:
    path = Path(path)
    base = Path(base)
    try:
        rel = path.relative_to(base)
    except ValueError:
        rel = Path(path.name)
    rel_no_ext = rel.with_suffix("")
    parts = [part.replace(" ", "_") for part in Path(rel_no_ext).parts]
    return "__".join(parts)


def run_mmdc(input_path: Path, svg_path: Path, cli_cmd: str, config_path: Path):
    cmd = shlex.split(cli_cmd) + ["-i", str(input_path), "-o", str(svg_path)]
    if config_path.exists():
        cmd += ["-c", str(config_path)]
    env = os.environ.copy()
    if "PUPPETEER_EXECUTABLE_PATH" not in env:
        chrome = find_puppeteer_chrome()
        if chrome:
            env["PUPPETEER_EXECUTABLE_PATH"] = chrome
    return run(cmd, env=env)


def compute_mmdr_metrics(files, bin_path, config_path, out_dir):
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


def compute_mmdc_metrics(files, cli_cmd, config_path, out_dir):
    layout_score = load_layout_score()
    out_dir.mkdir(parents=True, exist_ok=True)
    results = {}
    for file in files:
        key = layout_key(file, ROOT)
        svg_path = out_dir / f"{key}-mmdc.svg"
        if svg_path.exists():
            svg_path.unlink()
        res = run_mmdc(file, svg_path, cli_cmd, config_path)
        if res.returncode != 0:
            results[str(file)] = {"error": res.stderr.strip()[:200]}
            continue
        data, nodes, edges = load_mermaid_svg_graph(svg_path)
        metrics = layout_score.compute_metrics(data, nodes, edges)
        metrics["score"] = layout_score.weighted_score(metrics)
        results[str(file)] = metrics
    return results


def summarize_scores(results):
    scored = [v["score"] for v in results.values() if isinstance(v, dict) and "score" in v]
    if not scored:
        return 0.0, 0
    avg = sum(scored) / len(scored)
    return avg, len(scored)


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
        "--engine",
        choices=["mmdr", "mmdc", "both"],
        default="mmdr",
        help="layout engine to benchmark (default: mmdr)",
    )
    parser.add_argument(
        "--mmdc",
        default=os.environ.get("MMD_CLI", "npx -y @mermaid-js/mermaid-cli"),
        help="mermaid-cli command (default: env MMD_CLI or npx -y @mermaid-js/mermaid-cli)",
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
    if args.engine in {"mmdr", "both"}:
        build_release(bin_path)

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    config_path = Path(args.config)
    files = []
    patterns = [re.compile(p) for p in args.pattern] if args.pattern else []
    for base in fixtures:
        if base.exists():
            files.extend(sorted(base.glob("**/*.mmd")))
    if args.limit:
        files = files[: args.limit]
    if patterns:
        files = [f for f in files if any(p.search(str(f)) for p in patterns)]

    results = {}
    if args.engine in {"mmdr", "both"}:
        results["mmdr"] = compute_mmdr_metrics(files, bin_path, config_path, out_dir)
    if args.engine in {"mmdc", "both"}:
        results["mermaid_cli"] = compute_mmdc_metrics(files, args.mmdc, config_path, out_dir)

    if args.engine == "mmdr":
        output_json = Path(args.output_json) if args.output_json else out_dir / "quality.json"
        payload = results["mmdr"]
    elif args.engine == "mmdc":
        output_json = Path(args.output_json) if args.output_json else out_dir / "quality-mermaid-cli.json"
        payload = results["mermaid_cli"]
    else:
        output_json = Path(args.output_json) if args.output_json else out_dir / "quality-compare.json"
        payload = results

    output_json.write_text(json.dumps(payload, indent=2))
    print(f"Wrote {output_json}")

    if args.engine == "both":
        mmdr_avg, mmdr_count = summarize_scores(results.get("mmdr", {}))
        mmdc_avg, mmdc_count = summarize_scores(results.get("mermaid_cli", {}))
        if mmdr_count:
            print(f"mmdr: {mmdr_count} fixtures  Avg score: {mmdr_avg:.2f}")
        if mmdc_count:
            print(f"mermaid-cli: {mmdc_count} fixtures  Avg score: {mmdc_avg:.2f}")
    else:
        scored = [(k, v) for k, v in payload.items() if isinstance(v, dict) and "score" in v]
        if scored:
            scores = sorted(scored, key=lambda kv: kv[1]["score"], reverse=True)
            top = scores[:5]
            avg = sum(v["score"] for _, v in scored) / len(scored)
            print(f"Fixtures: {len(scored)}  Avg score: {avg:.2f}")
            print("Worst 5 by score:")
            for name, metrics in top:
                print(f"  {name}: {metrics['score']:.2f}")


if __name__ == "__main__":
    main()
