#!/usr/bin/env python3
import argparse
import datetime
import getpass
import hashlib
import importlib.util
import json
import math
import os
import platform
import re
import shlex
import shutil
import socket
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TOKEN_RE = re.compile(r"[AaCcHhLlMmQqSsTtVvZz]|[-+]?(?:\d*\.\d+|\d+)(?:[eE][-+]?\d+)?")
PIPE_EDGE_LABEL_RE = re.compile(r"\|[^|\n]+\|")
QUOTED_EDGE_LABEL_RE = re.compile(r"--\s*\"[^\"]+\"")
SEQUENCE_MESSAGE_LABEL_RE = re.compile(r"-{1,2}[x+o]?>{1,2}.*:\s*\S")
MMDC_CACHE_SCHEMA_VERSION = 1


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


def iso_utc_now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def git_metadata():
    def git(args):
        res = run(["git"] + args)
        if res.returncode != 0:
            return ""
        return res.stdout.strip()

    commit = git(["rev-parse", "HEAD"])
    short = commit[:12] if commit else ""
    branch = git(["rev-parse", "--abbrev-ref", "HEAD"])
    describe = git(["describe", "--always", "--dirty", "--tags"])
    status = git(["status", "--porcelain"])
    dirty = bool(status)
    return {
        "commit": commit,
        "commit_short": short,
        "branch": branch,
        "describe": describe,
        "dirty": dirty,
    }


def host_metadata():
    return {
        "hostname": socket.gethostname(),
        "user": getpass.getuser(),
        "python": sys.version.split()[0],
        "platform": platform.platform(),
    }


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


def bin_needs_rebuild(bin_path: Path):
    if not bin_path.exists():
        return True
    bin_mtime = bin_path.stat().st_mtime
    candidates = [ROOT / "Cargo.toml", ROOT / "Cargo.lock"]
    for path in candidates:
        if path.exists() and path.stat().st_mtime > bin_mtime:
            return True
    src_dir = ROOT / "src"
    if src_dir.exists():
        for path in src_dir.rglob("*.rs"):
            if path.stat().st_mtime > bin_mtime:
                return True
    return False


def build_release(bin_path: Path):
    if not bin_needs_rebuild(bin_path):
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


def detect_diagram_kind(path: Path):
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        return ""
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("%%"):
            continue
        if line.startswith("sequenceDiagram"):
            return "sequence"
        if line.startswith("flowchart") or line.startswith("graph"):
            return "flowchart"
        if line.startswith("classDiagram"):
            return "class"
        if line.startswith("stateDiagram"):
            return "state"
        if line.startswith("erDiagram"):
            return "er"
        if line.startswith("treemap"):
            return "treemap"
        break
    return ""


def expected_sequence_label_count(path: Path) -> int:
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        return 0
    count = 0
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("%%"):
            continue
        if SEQUENCE_MESSAGE_LABEL_RE.search(line):
            count += 1
    return count


def fixture_has_edge_label(path: Path, diagram_kind: str) -> bool:
    if diagram_kind == "sequence":
        return expected_sequence_label_count(path) > 0
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        return False
    return bool(PIPE_EDGE_LABEL_RE.search(text) or QUOTED_EDGE_LABEL_RE.search(text))


def layout_kind_name(diagram_kind: str):
    kind_map = {
        "sequence": "Sequence",
        "flowchart": "Flowchart",
        "class": "Class",
        "state": "State",
        "er": "Er",
        "treemap": "Treemap",
    }
    return kind_map.get(diagram_kind, "")


def parse_style_map(style: str):
    result = {}
    if not style:
        return result
    for part in style.split(";"):
        if ":" not in part:
            continue
        key, value = part.split(":", 1)
        key = key.strip().lower()
        value = value.strip()
        if key:
            result[key] = value
    return result


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
        cls_lower = cls.lower()
        is_edge_group = (
            in_edge_group
            or ("edgepaths" in cls_lower)
            or ("links" in cls_lower)
            or (cls_lower == "link")
        )
        is_edge_class = any(
            token in cls_lower
            for token in (
                "edgepath",
                "message",
                "signal",
                "arrow",
                "link",
                "relationship",
            )
        )
        if "actor-line" in cls_lower or "actorline" in cls_lower or "lifeline" in cls_lower:
            is_edge_class = False
        has_marker = "marker-end" in elem.attrib or "marker-start" in elem.attrib

        if tag == "path":
            if is_edge_group or is_edge_class or has_marker:
                d = elem.attrib.get("d", "")
                points = parse_path_points(d)
                if points:
                    points = [(x + cur_tx, y + cur_ty) for x, y in points]
                    edges.append(points)
        elif tag == "polyline":
            if is_edge_group or is_edge_class or has_marker:
                pts = parse_points(elem.attrib.get("points", ""))
                if pts:
                    points = [(x + cur_tx, y + cur_ty) for x, y in pts]
                    edges.append(points)
        elif tag == "line":
            if is_edge_group or is_edge_class or has_marker:
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
    width_attr = root.attrib.get("width", "")
    height_attr = root.attrib.get("height", "")
    width = parse_svg_number(width_attr)
    height = parse_svg_number(height_attr)
    if width <= 0.0 or height <= 0.0 or width_attr.strip().endswith("%") or height_attr.strip().endswith("%"):
        style = root.attrib.get("style", "")
        if style:
            for part in style.split(";"):
                if ":" not in part:
                    continue
                key, value = part.split(":", 1)
                key = key.strip().lower()
                if key == "width" and (width <= 0.0 or width_attr.strip().endswith("%")):
                    width = parse_svg_number(value)
                elif key == "height" and (height <= 0.0 or height_attr.strip().endswith("%")):
                    height = parse_svg_number(value)
    return width, height


def text_anchor(elem, style):
    anchor = elem.attrib.get("text-anchor")
    if not anchor:
        anchor = style.get("text-anchor", "")
    anchor = anchor.strip().lower()
    if anchor in {"middle", "end"}:
        return anchor
    return "start"


def text_font_size(elem, style):
    size = parse_svg_number(elem.attrib.get("font-size", ""))
    if size <= 0.0:
        size = parse_svg_number(style.get("font-size", ""))
    return size if size > 0.0 else 16.0


def first_attr_number(elem, attr):
    raw = elem.attrib.get(attr, "")
    if not raw:
        return None
    parts = [p for p in raw.replace(",", " ").split() if p]
    if not parts:
        return None
    return parse_svg_number(parts[0])


def extract_text_lines(text_elem):
    lines = []
    has_tspan = False
    for node in text_elem.iter():
        if strip_ns(node.tag) != "tspan":
            continue
        has_tspan = True
        raw = "".join(node.itertext()).strip()
        if raw:
            lines.append(raw)
    if has_tspan:
        return lines
    raw = "".join(text_elem.itertext()).strip()
    return [raw] if raw else []


def parse_text_boxes(svg_path: Path):
    root = ET.fromstring(svg_path.read_text())
    boxes = []

    def visit(elem, acc_tx, acc_ty):
        tag = strip_ns(elem.tag)
        if tag in {"defs", "style", "script"}:
            return
        tx, ty = parse_transform(elem.attrib.get("transform", ""))
        cur_tx = acc_tx + tx
        cur_ty = acc_ty + ty

        if tag == "foreignObject":
            width = parse_svg_number(elem.attrib.get("width", ""))
            height = parse_svg_number(elem.attrib.get("height", ""))
            if width > 0.0 and height > 0.0:
                x = parse_svg_number(elem.attrib.get("x", "")) + cur_tx
                y = parse_svg_number(elem.attrib.get("y", "")) + cur_ty
                boxes.append(
                    {
                        "x": x,
                        "y": y,
                        "width": width,
                        "height": height,
                        "class": elem.attrib.get("class", ""),
                    }
                )

        if tag == "text":
            style = parse_style_map(elem.attrib.get("style", ""))
            lines = extract_text_lines(elem)
            if lines:
                x = first_attr_number(elem, "x")
                y = first_attr_number(elem, "y")
                if x is None or y is None:
                    for node in elem.iter():
                        if strip_ns(node.tag) != "tspan":
                            continue
                        if x is None:
                            x = first_attr_number(node, "x")
                        if y is None:
                            y = first_attr_number(node, "y")
                        if x is not None and y is not None:
                            break
                if x is None:
                    x = 0.0
                if y is None:
                    y = 0.0
                x += cur_tx
                y += cur_ty
                font_size = text_font_size(elem, style)
                line_height = font_size * 1.2
                width = max(len(line) for line in lines) * font_size * 0.6
                height = max(font_size, len(lines) * line_height)
                anchor = text_anchor(elem, style)
                if anchor == "middle":
                    x -= width / 2.0
                elif anchor == "end":
                    x -= width
                # SVG y is text baseline; approximate top from baseline.
                y -= font_size * 0.8
                boxes.append(
                    {
                        "x": x,
                        "y": y,
                        "width": width,
                        "height": height,
                        "class": elem.attrib.get("class", ""),
                    }
                )

        for child in list(elem):
            visit(child, cur_tx, cur_ty)

    visit(root, 0.0, 0.0)
    return boxes


def parse_edge_label_boxes(svg_path: Path):
    root = ET.fromstring(svg_path.read_text())
    boxes = []

    def looks_like_edge_label_rect(elem, in_edge_label_group):
        if in_edge_label_group:
            return True
        h = parse_svg_number(elem.attrib.get("height", ""))
        if h <= 0.0 or h > 140.0:
            return False
        rx = parse_svg_number(elem.attrib.get("rx", ""))
        if rx > 6.0:
            return False
        style = parse_style_map(elem.attrib.get("style", ""))
        fill = (elem.attrib.get("fill") or style.get("fill") or "").strip().lower()
        stroke_opacity = parse_svg_number(
            elem.attrib.get("stroke-opacity", "") or style.get("stroke-opacity", "")
        )
        stroke_width = parse_svg_number(
            elem.attrib.get("stroke-width", "") or style.get("stroke-width", "")
        )
        # mmdr edge-label boxes are translucent rounded rects with rgba fill.
        if (
            fill.startswith("rgba(")
            and 0.0 < stroke_opacity <= 0.95
            and (stroke_width <= 0.0 or stroke_width <= 1.2)
        ):
            return True
        if stroke_opacity <= 0.0:
            return False
        return fill in {"#fff", "#ffffff", "white", "rgb(255,255,255)"}

    def visit(elem, acc_tx, acc_ty, in_edge_label_group):
        tag = strip_ns(elem.tag)
        if tag in {"defs", "style", "script"}:
            return
        tx, ty = parse_transform(elem.attrib.get("transform", ""))
        cur_tx = acc_tx + tx
        cur_ty = acc_ty + ty
        cls = elem.attrib.get("class", "").lower()
        is_edge_label_group = in_edge_label_group or "edgelabel" in cls

        if tag == "foreignObject" and is_edge_label_group:
            width = parse_svg_number(elem.attrib.get("width", ""))
            height = parse_svg_number(elem.attrib.get("height", ""))
            if width > 0.0 and height > 0.0:
                x = parse_svg_number(elem.attrib.get("x", "")) + cur_tx
                y = parse_svg_number(elem.attrib.get("y", "")) + cur_ty
                boxes.append({"x": x, "y": y, "width": width, "height": height})
        elif tag == "rect" and looks_like_edge_label_rect(elem, is_edge_label_group):
            width = parse_svg_number(elem.attrib.get("width", ""))
            height = parse_svg_number(elem.attrib.get("height", ""))
            if width > 0.0 and height > 0.0:
                x = parse_svg_number(elem.attrib.get("x", "")) + cur_tx
                y = parse_svg_number(elem.attrib.get("y", "")) + cur_ty
                boxes.append({"x": x, "y": y, "width": width, "height": height})

        for child in list(elem):
            visit(child, cur_tx, cur_ty, is_edge_label_group)

    visit(root, 0.0, 0.0, False)
    return boxes


def rect_overlap_area(a, b):
    ax1 = a["x"]
    ay1 = a["y"]
    ax2 = ax1 + a["width"]
    ay2 = ay1 + a["height"]
    bx1 = b["x"]
    by1 = b["y"]
    bx2 = bx1 + b["width"]
    by2 = by1 + b["height"]
    ix1 = max(ax1, bx1)
    iy1 = max(ay1, by1)
    ix2 = min(ax2, bx2)
    iy2 = min(ay2, by2)
    if ix2 <= ix1 or iy2 <= iy1:
        return 0.0
    return (ix2 - ix1) * (iy2 - iy1)


def orient(a, b, c):
    return (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])


def on_segment(a, b, c, eps):
    return (
        min(a[0], b[0]) - eps <= c[0] <= max(a[0], b[0]) + eps
        and min(a[1], b[1]) - eps <= c[1] <= max(a[1], b[1]) + eps
    )


def segments_intersect(a, b, c, d, eps=1e-6):
    o1 = orient(a, b, c)
    o2 = orient(a, b, d)
    o3 = orient(c, d, a)
    o4 = orient(c, d, b)

    if abs(o1) < eps and abs(o2) < eps and abs(o3) < eps and abs(o4) < eps:
        return False
    if o1 * o2 < 0 and o3 * o4 < 0:
        return True
    if abs(o1) < eps and on_segment(a, b, c, eps):
        return True
    if abs(o2) < eps and on_segment(a, b, d, eps):
        return True
    if abs(o3) < eps and on_segment(c, d, a, eps):
        return True
    if abs(o4) < eps and on_segment(c, d, b, eps):
        return True
    return False


def segment_intersects_rect(a, b, rect, eps=1e-6):
    x = rect["x"]
    y = rect["y"]
    w = rect["width"]
    h = rect["height"]
    x1, y1 = a
    x2, y2 = b
    min_x = min(x1, x2)
    max_x = max(x1, x2)
    min_y = min(y1, y2)
    max_y = max(y1, y2)
    if max_x < x - eps or min_x > x + w + eps or max_y < y - eps or min_y > y + h + eps:
        return False
    if x - eps <= x1 <= x + w + eps and y - eps <= y1 <= y + h + eps:
        return True
    if x - eps <= x2 <= x + w + eps and y - eps <= y2 <= y + h + eps:
        return True
    corners = [
        (x, y),
        (x + w, y),
        (x + w, y + h),
        (x, y + h),
    ]
    edges = [
        (corners[0], corners[1]),
        (corners[1], corners[2]),
        (corners[2], corners[3]),
        (corners[3], corners[0]),
    ]
    for c, d in edges:
        if segments_intersect(a, b, c, d):
            return True
    return False


def point_segment_distance(point, a, b):
    ax, ay = a
    bx, by = b
    px, py = point
    dx = bx - ax
    dy = by - ay
    len_sq = dx * dx + dy * dy
    if len_sq <= 1e-9:
        return math.hypot(px - ax, py - ay)
    t = ((px - ax) * dx + (py - ay) * dy) / len_sq
    t = max(0.0, min(1.0, t))
    proj_x = ax + dx * t
    proj_y = ay + dy * t
    return math.hypot(px - proj_x, py - proj_y)


def point_polyline_distance(point, points):
    if not points:
        return float("inf")
    if len(points) == 1:
        return math.hypot(point[0] - points[0][0], point[1] - points[0][1])
    best = float("inf")
    for a, b in zip(points, points[1:]):
        best = min(best, point_segment_distance(point, a, b))
    return best


def point_rect_distance(point, rect):
    px, py = point
    x1 = rect["x"]
    y1 = rect["y"]
    x2 = x1 + rect["width"]
    y2 = y1 + rect["height"]
    dx = max(x1 - px, 0.0, px - x2)
    dy = max(y1 - py, 0.0, py - y2)
    return math.hypot(dx, dy)


def segment_rect_gap(a, b, rect):
    if segment_intersects_rect(a, b, rect):
        return 0.0
    x = rect["x"]
    y = rect["y"]
    w = rect["width"]
    h = rect["height"]
    corners = [
        (x, y),
        (x + w, y),
        (x + w, y + h),
        (x, y + h),
    ]
    best = min(point_segment_distance(corner, a, b) for corner in corners)
    best = min(best, point_rect_distance(a, rect), point_rect_distance(b, rect))
    return best


def polyline_rect_gap(points, rect):
    if len(points) < 2:
        return float("inf")
    best = float("inf")
    for a, b in zip(points, points[1:]):
        best = min(best, segment_rect_gap(a, b, rect))
        if best <= 1e-9:
            return 0.0
    return best


def collinear_overlap_length(a, b, c, d, eps=1e-6):
    if abs(orient(a, b, c)) > eps or abs(orient(a, b, d)) > eps:
        return 0.0
    dx = b[0] - a[0]
    dy = b[1] - a[1]
    seg_len_sq = dx * dx + dy * dy
    if seg_len_sq < eps:
        return 0.0

    def proj(p):
        return ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / seg_len_sq

    t1 = proj(c)
    t2 = proj(d)
    tmin = min(t1, t2)
    tmax = max(t1, t2)
    overlap = max(0.0, min(1.0, tmax) - max(0.0, tmin))
    return overlap * math.sqrt(seg_len_sq)


def compute_svg_edge_path_metrics(edges):
    segments = []
    for idx, edge in enumerate(edges):
        points = [tuple(p) for p in edge.get("points", [])]
        if len(points) < 2:
            continue
        for a, b in zip(points, points[1:]):
            segments.append((idx, a, b))

    crossings = 0
    overlap_length = 0.0
    for i in range(len(segments)):
        ei, a1, a2 = segments[i]
        for j in range(i + 1, len(segments)):
            ej, b1, b2 = segments[j]
            if ei == ej:
                continue
            if (
                math.hypot(a1[0] - b1[0], a1[1] - b1[1]) < 1e-6
                or math.hypot(a1[0] - b2[0], a1[1] - b2[1]) < 1e-6
                or math.hypot(a2[0] - b1[0], a2[1] - b1[1]) < 1e-6
                or math.hypot(a2[0] - b2[0], a2[1] - b2[1]) < 1e-6
            ):
                continue
            if segments_intersect(a1, a2, b1, b2):
                crossings += 1
            overlap_length += collinear_overlap_length(a1, a2, b1, b2)

    return {
        "svg_edge_crossings": crossings,
        "svg_edge_overlap_length": overlap_length,
    }


def infer_label_owner(label, nodes):
    cx = label["x"] + label["width"] / 2.0
    cy = label["y"] + label["height"] / 2.0
    best_id = None
    best_area = None
    for node_id, node in nodes.items():
        x = node.get("x", 0.0)
        y = node.get("y", 0.0)
        w = node.get("width", 0.0)
        h = node.get("height", 0.0)
        if w <= 0.0 or h <= 0.0:
            continue
        if cx < x or cx > x + w or cy < y or cy > y + h:
            continue
        area = w * h
        if best_area is None or area < best_area:
            best_area = area
            best_id = node_id
    return best_id


def compute_label_metrics(
    svg_path: Path,
    nodes,
    edges,
    diagram_kind="",
    allow_fallback_candidates=True,
    expected_edge_label_count=None,
):
    # Ignore tiny estimated text-box slivers that arise from font/rendering
    # differences across hosts; they are visually negligible but can create
    # unstable count deltas in cross-machine benchmark runs.
    min_overlap_area = 10.0
    labels = parse_text_boxes(svg_path)
    explicit_edge_label_boxes = parse_edge_label_boxes(svg_path)
    root = ET.fromstring(svg_path.read_text())
    canvas_width, canvas_height = svg_size(root)
    canvas_rect = {
        "x": 0.0,
        "y": 0.0,
        "width": max(0.0, canvas_width),
        "height": max(0.0, canvas_height),
    }
    for label in labels:
        label["owner"] = infer_label_owner(label, nodes)

    overlap_count = 0
    overlap_area = 0.0
    for i in range(len(labels)):
        for j in range(i + 1, len(labels)):
            area = rect_overlap_area(labels[i], labels[j])
            if area > min_overlap_area:
                overlap_count += 1
                overlap_area += area

    label_edge_pairs = 0
    labels_touching_edges = 0
    for label in labels:
        touched = False
        owner = label.get("owner")
        for edge in edges:
            if owner and (edge.get("from") == owner or edge.get("to") == owner):
                continue
            points = [tuple(p) for p in edge.get("points", [])]
            if len(points) < 2:
                continue
            edge_hit = False
            for a, b in zip(points, points[1:]):
                if segment_intersects_rect(a, b, label):
                    edge_hit = True
                    touched = True
                    break
            if edge_hit:
                label_edge_pairs += 1
        if touched:
            labels_touching_edges += 1

    label_total_area = 0.0
    label_out_of_bounds_count = 0
    label_out_of_bounds_area = 0.0
    if canvas_rect["width"] > 0.0 and canvas_rect["height"] > 0.0:
        for label in labels:
            area = max(0.0, label["width"]) * max(0.0, label["height"])
            label_total_area += area
            visible = rect_overlap_area(label, canvas_rect)
            clipped = max(0.0, area - visible)
            if clipped > 1e-3:
                label_out_of_bounds_count += 1
                label_out_of_bounds_area += clipped

    edge_label_distances = []
    edge_label_bad_count = 0
    edge_label_path_gaps = []
    edge_label_path_bad_count = 0
    edge_label_path_touch_count = 0
    edge_label_clearance_scores = []
    edge_label_in_band_count = 0
    edge_label_touch_eps = 0.5
    edge_label_clearance_target = 2.0
    edge_label_clearance_sigma = 2.0
    candidate_edge_labels = list(explicit_edge_label_boxes)
    use_fallback_candidate_filter = False
    if not candidate_edge_labels and allow_fallback_candidates:
        candidate_edge_labels = [label for label in labels if label.get("owner") is None]
        use_fallback_candidate_filter = True

    candidate_records = []
    for label in candidate_edge_labels:
        center = (
            label["x"] + label["width"] * 0.5,
            label["y"] + label["height"] * 0.5,
        )
        min_dist = float("inf")
        min_gap = float("inf")
        for edge in edges:
            points = [tuple(p) for p in edge.get("points", [])]
            if len(points) < 2:
                continue
            min_dist = min(min_dist, point_polyline_distance(center, points))
            min_gap = min(min_gap, polyline_rect_gap(points, label))
        if not math.isfinite(min_dist) or not math.isfinite(min_gap):
            continue
        # Exclude obvious non-edge labels (titles, distant captions).
        if diagram_kind == "sequence":
            candidate_dist_cutoff = max(28.0, label["height"] * 2.6)
            candidate_gap_cutoff = max(20.0, label["height"] * 1.8)
            bad_limit = max(20.0, label["height"] * 3.4)
            path_bad_limit = max(16.0, label["height"] * 2.0)
        else:
            # Fallback labels are noisy; only keep labels that are plausibly
            # attached to an edge by center or by rect-to-path clearance.
            candidate_dist_cutoff = max(16.0, label["height"] * 1.5)
            candidate_gap_cutoff = max(12.0, label["height"] * 1.25)
            bad_limit = max(10.0, label["height"] * 1.75)
            path_bad_limit = max(8.0, label["height"] * 0.9)
        if (
            use_fallback_candidate_filter
            and min_dist > candidate_dist_cutoff
            and min_gap > candidate_gap_cutoff
        ):
            continue
        candidate_records.append((label, min_dist, min_gap, bad_limit, path_bad_limit))

    # Sequence SVGs often contain actor labels in the same text style family.
    # Keep the nearest plausible labels, bounded by expected message-label count.
    if use_fallback_candidate_filter and diagram_kind == "sequence" and candidate_records:
        target_count = expected_edge_label_count
        if not isinstance(target_count, int) or target_count <= 0:
            target_count = sum(1 for edge in edges if len(edge.get("points", [])) >= 2)
        if target_count > 0 and len(candidate_records) > target_count:
            candidate_records.sort(key=lambda row: (row[2], row[1]))
            candidate_records = candidate_records[:target_count]

    for _label, min_dist, min_gap, bad_limit, path_bad_limit in candidate_records:
        edge_label_distances.append(min_dist)
        edge_label_path_gaps.append(min_gap)
        if min_dist > bad_limit:
            edge_label_bad_count += 1
        if min_gap > path_bad_limit:
            edge_label_path_bad_count += 1
        if min_gap <= edge_label_touch_eps:
            edge_label_path_touch_count += 1
            edge_label_clearance_scores.append(0.0)
        else:
            z = (min_gap - edge_label_clearance_target) / edge_label_clearance_sigma
            edge_label_clearance_scores.append(math.exp(-0.5 * z * z))
        if 1.0 <= min_gap <= 6.0:
            edge_label_in_band_count += 1

    edge_label_alignment_mean = (
        sum(edge_label_distances) / len(edge_label_distances)
        if edge_label_distances
        else 0.0
    )
    edge_label_alignment_p95 = 0.0
    if edge_label_distances:
        ordered = sorted(edge_label_distances)
        p95_idx = int(round((len(ordered) - 1) * 0.95))
        edge_label_alignment_p95 = ordered[p95_idx]
    edge_label_path_gap_mean = (
        sum(edge_label_path_gaps) / len(edge_label_path_gaps)
        if edge_label_path_gaps
        else 0.0
    )
    edge_label_path_gap_p95 = 0.0
    if edge_label_path_gaps:
        ordered = sorted(edge_label_path_gaps)
        p95_idx = int(round((len(ordered) - 1) * 0.95))
        edge_label_path_gap_p95 = ordered[p95_idx]

    metrics = {
        "label_count": len(labels),
        "label_overlap_count": overlap_count,
        "label_overlap_area": overlap_area,
        "label_edge_overlap_count": labels_touching_edges,
        "label_edge_overlap_pairs": label_edge_pairs,
        "label_total_area": label_total_area,
        "label_out_of_bounds_count": label_out_of_bounds_count,
        "label_out_of_bounds_area": label_out_of_bounds_area,
        "label_out_of_bounds_ratio": (
            label_out_of_bounds_area / label_total_area if label_total_area > 1e-9 else 0.0
        ),
        "edge_label_alignment_count": len(edge_label_distances),
        "edge_label_alignment_bad_count": edge_label_bad_count,
        "edge_label_path_gap_count": len(edge_label_path_gaps),
        "edge_label_detected_count": len(candidate_records),
    }
    if edge_label_distances:
        metrics.update(
            {
                "edge_label_alignment_mean": edge_label_alignment_mean,
                "edge_label_alignment_p95": edge_label_alignment_p95,
                "edge_label_alignment_bad_ratio": (
                    edge_label_bad_count / len(edge_label_distances)
                ),
            }
        )
    if edge_label_path_gaps:
        metrics.update(
            {
                "edge_label_path_gap_mean": edge_label_path_gap_mean,
                "edge_label_path_gap_p95": edge_label_path_gap_p95,
                "edge_label_path_touch_count": edge_label_path_touch_count,
                "edge_label_path_touch_ratio": (
                    edge_label_path_touch_count / len(edge_label_path_gaps)
                ),
                "edge_label_path_gap_bad_count": edge_label_path_bad_count,
                "edge_label_path_gap_bad_ratio": (
                    edge_label_path_bad_count / len(edge_label_path_gaps)
                ),
                # Score in [0,1]: 0 when touching path, highest near target
                # clearance band (~2px), and decays when labels drift too far.
                "edge_label_path_clearance_score_mean": (
                    sum(edge_label_clearance_scores) / len(edge_label_clearance_scores)
                    if edge_label_clearance_scores
                    else 0.0
                ),
                "edge_label_path_non_touch_ratio": (
                    1.0 - (edge_label_path_touch_count / len(edge_label_path_gaps))
                ),
                "edge_label_path_in_band_ratio": (
                    edge_label_in_band_count / len(edge_label_path_gaps)
                ),
            }
        )
    return metrics


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


def parse_anchor_tuple(value):
    if not isinstance(value, (list, tuple)) or len(value) < 2:
        return None
    try:
        return float(value[0]), float(value[1])
    except (TypeError, ValueError):
        return None


def compute_layout_anchor_metrics(layout_edges):
    distances = []
    miss_count = 0
    for edge in layout_edges:
        points = [tuple(p) for p in edge.get("points", [])]
        if len(points) < 2:
            continue
        for label_key, anchor_key in (
            ("label", "label_anchor"),
            ("start_label", "start_label_anchor"),
            ("end_label", "end_label_anchor"),
        ):
            if edge.get(label_key) is None:
                continue
            anchor = parse_anchor_tuple(edge.get(anchor_key))
            if anchor is None:
                continue
            dist = point_polyline_distance(anchor, points)
            distances.append(dist)
            if dist > 8.0:
                miss_count += 1

    mean_dist = sum(distances) / len(distances) if distances else 0.0
    max_dist = max(distances) if distances else 0.0
    return {
        "layout_anchor_label_count": len(distances),
        "layout_anchor_alignment_mean": mean_dist,
        "layout_anchor_alignment_max": max_dist,
        "layout_anchor_miss_count": miss_count,
        "layout_anchor_miss_ratio": (miss_count / len(distances)) if distances else 0.0,
    }


def layout_key(path: Path, base: Path) -> str:
    path = Path(path).resolve()
    base = Path(base).resolve()
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


def mmdc_cli_identity(cli_cmd: str) -> str:
    res = run(shlex.split(cli_cmd) + ["--version"])
    out = (res.stdout or "").strip()
    err = (res.stderr or "").strip()
    identity = out or err or f"rc={res.returncode}"
    return identity.splitlines()[-1][:240]


def mmdc_cache_digest(
    fixture_path: Path,
    config_path: Path,
    cli_cmd: str,
    cli_identity: str,
    script_digest: str,
) -> str:
    hasher = hashlib.sha256()
    hasher.update(f"schema:{MMDC_CACHE_SCHEMA_VERSION}\n".encode("utf-8"))
    hasher.update(f"cli:{cli_cmd}\n".encode("utf-8"))
    hasher.update(f"cli_identity:{cli_identity}\n".encode("utf-8"))
    hasher.update(f"script:{script_digest}\n".encode("utf-8"))
    hasher.update(fixture_path.read_bytes())
    if config_path.exists():
        hasher.update(config_path.read_bytes())
    return hasher.hexdigest()


def load_json_if_exists(path: Path):
    if not path.exists():
        return None
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return None
    return data if isinstance(data, dict) else None


def save_json(path: Path, payload: dict):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, separators=(",", ":"), sort_keys=True), encoding="utf-8")


def compute_mmdc_svg_metrics(
    svg_path: Path,
    layout_score,
    diagram_kind: str,
    allow_fallback_labels: bool,
    expected_sequence_labels,
):
    data, nodes, edges = load_mermaid_svg_graph(svg_path)
    kind_name = layout_kind_name(diagram_kind)
    if kind_name:
        data["kind"] = kind_name
    metrics = layout_score.compute_metrics(data, nodes, edges)
    metrics["score"] = layout_score.weighted_score(metrics)
    metrics.update(
        compute_label_metrics(
            svg_path,
            nodes,
            edges,
            diagram_kind,
            allow_fallback_candidates=allow_fallback_labels,
            expected_edge_label_count=expected_sequence_labels,
        )
    )
    svg_metrics = compute_svg_edge_path_metrics(edges)
    metrics.update(svg_metrics)
    metrics["arrow_path_intersections"] = svg_metrics.get("svg_edge_crossings", 0)
    metrics["arrow_path_overlap_length"] = svg_metrics.get("svg_edge_overlap_length", 0.0)
    return metrics


def compute_mmdr_metrics(files, bin_path, config_path, out_dir):
    layout_score = load_layout_score()
    out_dir.mkdir(parents=True, exist_ok=True)
    config_args = ["-c", str(config_path)] if config_path.exists() else []
    results = {}
    for file in files:
        diagram_kind = detect_diagram_kind(file)
        allow_fallback_labels = fixture_has_edge_label(file, diagram_kind)
        expected_sequence_labels = (
            expected_sequence_label_count(file) if diagram_kind == "sequence" else None
        )
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
        _, _, svg_edges = load_mermaid_svg_graph(svg_path)
        metrics.update(
            compute_label_metrics(
                svg_path,
                nodes,
                svg_edges,
                diagram_kind,
                allow_fallback_candidates=allow_fallback_labels,
                expected_edge_label_count=expected_sequence_labels,
            )
        )
        svg_metrics = compute_svg_edge_path_metrics(svg_edges)
        metrics.update(svg_metrics)
        metrics["arrow_path_intersections"] = svg_metrics.get("svg_edge_crossings", 0)
        metrics["arrow_path_overlap_length"] = svg_metrics.get("svg_edge_overlap_length", 0.0)
        layout_data = json.loads(layout_path.read_text())
        metrics.update(compute_layout_anchor_metrics(layout_data.get("edges", [])))
        results[str(file)] = metrics
    return results


def compute_mmdc_metrics(files, cli_cmd, config_path, out_dir, cache_dir=None, use_cache=True):
    layout_score = load_layout_score()
    out_dir.mkdir(parents=True, exist_ok=True)
    cache_dir = Path(cache_dir) if cache_dir else ROOT / "tmp" / "benchmark-cache" / "mmdc"
    if use_cache:
        cache_dir.mkdir(parents=True, exist_ok=True)
    cli_identity = mmdc_cli_identity(cli_cmd) if use_cache else ""
    script_digest = (
        hashlib.sha256(Path(__file__).read_bytes()).hexdigest() if use_cache else ""
    )
    results = {}
    for file in files:
        diagram_kind = detect_diagram_kind(file)
        allow_fallback_labels = fixture_has_edge_label(file, diagram_kind)
        expected_sequence_labels = (
            expected_sequence_label_count(file) if diagram_kind == "sequence" else None
        )
        key = layout_key(file, ROOT)
        svg_path = out_dir / f"{key}-mmdc.svg"
        cache_svg_path = None
        cache_metrics_path = None
        if use_cache:
            digest = mmdc_cache_digest(
                file, config_path, cli_cmd, cli_identity, script_digest
            )
            cache_svg_path = cache_dir / f"{digest}.svg"
            cache_metrics_path = cache_dir / f"{digest}.metrics.json"

        cached_metrics = load_json_if_exists(cache_metrics_path) if use_cache else None
        if cached_metrics is not None and cache_svg_path and cache_svg_path.exists():
            shutil.copy2(cache_svg_path, svg_path)
            results[str(file)] = cached_metrics
            continue

        source_svg_path = cache_svg_path if (use_cache and cache_svg_path) else svg_path
        if source_svg_path.exists():
            source_svg_path.unlink()
        res = run_mmdc(file, source_svg_path, cli_cmd, config_path)
        if res.returncode != 0:
            metrics = {"error": res.stderr.strip()[:200]}
            results[str(file)] = metrics
            if use_cache and cache_metrics_path:
                save_json(cache_metrics_path, metrics)
            continue

        metrics = compute_mmdc_svg_metrics(
            source_svg_path,
            layout_score,
            diagram_kind,
            allow_fallback_labels,
            expected_sequence_labels,
        )
        if use_cache and cache_metrics_path:
            save_json(cache_metrics_path, metrics)
        if source_svg_path != svg_path:
            shutil.copy2(source_svg_path, svg_path)
        results[str(file)] = metrics
    return results


def summarize_scores(results):
    scored = [v["score"] for v in results.values() if isinstance(v, dict) and "score" in v]
    if not scored:
        return 0.0, 0
    avg = sum(scored) / len(scored)
    return avg, len(scored)


def summarize_metric(results, key):
    values = [
        v[key]
        for v in results.values()
        if isinstance(v, dict) and isinstance(v.get(key), (int, float))
    ]
    if not values:
        return None, 0
    return sum(values) / len(values), len(values)


def collect_common_scored(left, right):
    common = []
    for key, lval in left.items():
        rval = right.get(key)
        if not isinstance(lval, dict) or not isinstance(rval, dict):
            continue
        if "score" not in lval or "score" not in rval:
            continue
        common.append(key)
    return common


def metric_compare_counts(left, right, keys, metric, eps=1e-9):
    better = 0
    equal = 0
    worse = 0
    regressions = []
    for key in keys:
        lval = left[key].get(metric)
        rval = right[key].get(metric)
        if not isinstance(lval, (int, float)) or not isinstance(rval, (int, float)):
            continue
        delta = lval - rval
        if delta < -eps:
            better += 1
        elif delta > eps:
            worse += 1
            regressions.append((delta, key, lval, rval))
        else:
            equal += 1
    regressions.sort(reverse=True, key=lambda item: item[0])
    return better, equal, worse, regressions


def common_comparison_stats(left, right):
    common = collect_common_scored(left, right)
    if not common:
        return {}
    core_metrics = [
        "score",
        "edge_crossings",
        "svg_edge_crossings",
        "arrow_path_intersections",
        "label_overlap_count",
        "label_out_of_bounds_count",
    ]
    metrics = {}
    for metric in core_metrics:
        better, equal, worse, regressions = metric_compare_counts(left, right, common, metric)
        metric_stats = {
            "better": better,
            "equal": equal,
            "worse": worse,
        }
        if regressions:
            top = regressions[0]
            metric_stats["worst_regression"] = {
                "fixture": top[1],
                "fixture_name": Path(top[1]).name,
                "delta": float(top[0]),
                "mmdr": float(top[2]),
                "mermaid_cli": float(top[3]),
            }
        metrics[metric] = metric_stats

    dominance_metrics = [
        "score",
        "edge_crossings",
        "arrow_path_intersections",
        "label_overlap_count",
        "label_out_of_bounds_count",
    ]
    non_worse = 0
    strict = 0
    comparable = 0
    for key in common:
        comparable_metrics = []
        for metric in dominance_metrics:
            lval = left[key].get(metric)
            rval = right[key].get(metric)
            if isinstance(lval, (int, float)) and isinstance(rval, (int, float)):
                comparable_metrics.append((lval, rval))
        if len(comparable_metrics) != len(dominance_metrics):
            continue
        comparable += 1
        is_non_worse = all(l <= r + 1e-9 for l, r in comparable_metrics)
        is_strict = is_non_worse and any(l < r - 1e-9 for l, r in comparable_metrics)
        if is_non_worse:
            non_worse += 1
        if is_strict:
            strict += 1
    return {
        "common_scored_fixtures": len(common),
        "metrics": metrics,
        "core_dominance": {
            "metrics": dominance_metrics,
            "comparable": comparable,
            "non_worse": non_worse,
            "strictly_better": strict,
        },
    }


def summarize_common_comparison(left, right):
    stats = common_comparison_stats(left, right)
    if not stats:
        return []
    lines = [f"Common scored fixtures: {stats['common_scored_fixtures']}"]
    for metric in [
        "score",
        "edge_crossings",
        "svg_edge_crossings",
        "arrow_path_intersections",
        "label_overlap_count",
        "label_out_of_bounds_count",
    ]:
        metric_stats = stats["metrics"].get(metric, {})
        lines.append(
            "mmdr vs mermaid-cli "
            f"`{metric}`: better {metric_stats.get('better', 0)}, "
            f"equal {metric_stats.get('equal', 0)}, "
            f"worse {metric_stats.get('worse', 0)}"
        )
        top = metric_stats.get("worst_regression")
        if top:
            lines.append(
                "  worst regression: "
                f"{top['fixture_name']} (mmdr={top['mmdr']:.3f}, mermaid-cli={top['mermaid_cli']:.3f})"
            )
    core = stats.get("core_dominance", {})
    if core.get("comparable", 0) > 0:
        metrics = core.get("metrics", [])
        lines.append(
            "Core-dominance "
            f"({', '.join(metrics)}): non-worse {core['non_worse']}/{core['comparable']}, "
            f"strictly better {core['strictly_better']}/{core['comparable']}"
        )
    return lines


def weighted_dominance_stats(left, right):
    common = collect_common_scored(left, right)
    if not common:
        return {}
    layout_score = load_layout_score()
    weights = getattr(layout_score, "WEIGHTS", {})
    if not weights:
        return {}

    by_metric_debt = {metric: 0.0 for metric in weights}
    comparable = 0
    non_worse = 0
    strict = 0
    worst_fixture = ("", 0.0)

    for key in common:
        fixture_debt = 0.0
        has_weighted_metric = False
        fixture_worse = False
        fixture_better = False
        for metric, weight in weights.items():
            lval = left[key].get(metric)
            rval = right[key].get(metric)
            if not isinstance(lval, (int, float)) or not isinstance(rval, (int, float)):
                continue
            has_weighted_metric = True
            delta = lval - rval
            if delta > 1e-9:
                fixture_worse = True
                debt = delta * weight
                fixture_debt += debt
                by_metric_debt[metric] += debt
            elif delta < -1e-9:
                fixture_better = True
        if not has_weighted_metric:
            continue
        comparable += 1
        if not fixture_worse:
            non_worse += 1
            if fixture_better:
                strict += 1
        if fixture_debt > worst_fixture[1]:
            worst_fixture = (key, fixture_debt)

    if comparable == 0:
        return {}

    ranked = sorted(
        ((metric, debt) for metric, debt in by_metric_debt.items() if debt > 0.0),
        key=lambda item: item[1],
        reverse=True,
    )
    top_contributors = []
    for metric, debt in ranked[:6]:
        better, equal, worse, _ = metric_compare_counts(left, right, common, metric)
        top_contributors.append(
            {
                "metric": metric,
                "debt": debt,
                "better": better,
                "equal": equal,
                "worse": worse,
            }
        )

    worst_fixture_stats = {}
    if worst_fixture[1] > 0.0:
        worst_fixture_stats = {
            "fixture": worst_fixture[0],
            "fixture_name": Path(worst_fixture[0]).name,
            "debt": worst_fixture[1],
        }

    return {
        "weighted_metrics": len(weights),
        "comparable": comparable,
        "non_worse": non_worse,
        "strictly_better": strict,
        "total_debt": sum(by_metric_debt.values()),
        "debt_by_metric": by_metric_debt,
        "top_contributors": top_contributors,
        "worst_fixture": worst_fixture_stats,
    }


def summarize_weighted_dominance(left, right):
    stats = weighted_dominance_stats(left, right)
    if not stats:
        return []
    lines = [
        "Weighted-dominance "
        f"({stats['weighted_metrics']} weighted metrics): non-worse {stats['non_worse']}/{stats['comparable']}, "
        f"strictly better {stats['strictly_better']}/{stats['comparable']}",
        f"Weighted regression debt vs mermaid-cli: {stats['total_debt']:.2f}",
    ]
    if stats["top_contributors"]:
        lines.append("Top weighted regression contributors:")
        for entry in stats["top_contributors"]:
            lines.append(
                f"  {entry['metric']}: debt={entry['debt']:.2f}, "
                f"better={entry['better']}, equal={entry['equal']}, worse={entry['worse']}"
            )
    if stats["worst_fixture"]:
        lines.append(
            "Worst weighted-regression fixture: "
            f"{stats['worst_fixture']['fixture_name']} (debt={stats['worst_fixture']['debt']:.2f})"
        )
    return lines


def compute_sequence_cli_conformance(layout_path: Path, mermaid_svg_path: Path):
    layout_diff = load_layout_diff()
    mmdr_nodes, _ = layout_diff.load_mmdr_layout(layout_path)
    mer_nodes, mer_labels, _, mer_specials = layout_diff.parse_mermaid_svg(mermaid_svg_path)
    diffs, missing = layout_diff.compute_diffs(mmdr_nodes, mer_nodes, mer_labels, mer_specials)
    summary = layout_diff.summarize_diffs(diffs)
    _, _, aligned_summary, _ = layout_diff.align_diffs(diffs)
    return {
        "sequence_cli_match_count": summary.get("count", 0),
        "sequence_cli_missing_nodes": len(missing),
        "sequence_cli_mean_distance": summary.get("mean_distance", 0.0),
        "sequence_cli_max_distance": summary.get("max_distance", 0.0),
        "sequence_cli_aligned_mean_distance": aligned_summary.get("mean_distance", 0.0),
        "sequence_cli_aligned_max_distance": aligned_summary.get("max_distance", 0.0),
    }


def augment_sequence_cli_conformance(files, mmdr_results, out_dir):
    for file in files:
        if detect_diagram_kind(file) != "sequence":
            continue
        file_key_str = str(file)
        metrics = mmdr_results.get(file_key_str)
        if not isinstance(metrics, dict) or "score" not in metrics:
            continue
        key = layout_key(file, ROOT)
        layout_path = out_dir / f"{key}-layout.json"
        mermaid_svg_path = out_dir / f"{key}-mmdc.svg"
        if not layout_path.exists() or not mermaid_svg_path.exists():
            continue
        try:
            metrics.update(compute_sequence_cli_conformance(layout_path, mermaid_svg_path))
        except Exception as exc:
            metrics["sequence_cli_conformance_error"] = str(exc)[:200]


def summarize_sequence_cli_conformance(results):
    rows = [
        v
        for v in results.values()
        if isinstance(v, dict) and "sequence_cli_aligned_mean_distance" in v
    ]
    if not rows:
        return []
    aligned_means = [float(v["sequence_cli_aligned_mean_distance"]) for v in rows]
    aligned_maxes = [float(v["sequence_cli_aligned_max_distance"]) for v in rows]
    missing_nodes = [int(v.get("sequence_cli_missing_nodes", 0)) for v in rows]
    lines = []
    lines.append(
        "sequence vs mermaid-cli conformance: "
        f"{len(rows)} fixtures, "
        f"avg aligned mean node distance={sum(aligned_means)/len(aligned_means):.2f}px, "
        f"avg aligned max node distance={sum(aligned_maxes)/len(aligned_maxes):.2f}px"
    )
    lines.append(
        "sequence vs mermaid-cli conformance: "
        f"fixtures with missing mapped nodes={sum(1 for n in missing_nodes if n > 0)}/{len(rows)}"
    )
    return lines


def append_benchmark_history(history_path: Path, record: dict):
    history_path.parent.mkdir(parents=True, exist_ok=True)
    with history_path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, separators=(",", ":")))
        handle.write("\n")


def main():
    parser = argparse.ArgumentParser(description="Compute layout quality metrics")
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
        "--mmdc-cache-dir",
        default=str(ROOT / "tmp" / "benchmark-cache" / "mmdc"),
        help="cache dir for mermaid-cli SVG/metrics reuse across runs",
    )
    parser.add_argument(
        "--no-mmdc-cache",
        action="store_true",
        help="disable mermaid-cli cache reuse",
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
    parser.add_argument(
        "--history-log",
        default=str(ROOT / "tmp" / "benchmark-history" / "quality-runs.jsonl"),
        help="append run summary metadata to this JSONL path",
    )
    parser.add_argument(
        "--no-history-log",
        action="store_true",
        help="disable benchmark history JSONL logging for this run",
    )
    args = parser.parse_args()
    run_started_at = iso_utc_now()
    run_started_epoch = time.time()
    git_info = git_metadata()
    host_info = host_metadata()

    fixtures = [Path(p) for p in args.fixtures if p]
    if not fixtures:
        fixtures = [
            ROOT / "tests" / "fixtures",
            ROOT / "benches" / "fixtures",
            ROOT / "docs" / "comparison_sources",
        ]

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
        results["mermaid_cli"] = compute_mmdc_metrics(
            files,
            args.mmdc,
            config_path,
            out_dir,
            cache_dir=Path(args.mmdc_cache_dir),
            use_cache=not args.no_mmdc_cache,
        )
    if args.engine == "both":
        augment_sequence_cli_conformance(files, results.get("mmdr", {}), out_dir)

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

    history_summary = {}
    comparison_stats = {}
    weighted_stats = {}

    if args.engine == "both":
        mmdr_avg, mmdr_count = summarize_scores(results.get("mmdr", {}))
        mmdc_avg, mmdc_count = summarize_scores(results.get("mermaid_cli", {}))
        history_summary.update(
            {
                "mmdr_count": mmdr_count,
                "mmdr_avg_score": mmdr_avg,
                "mermaid_cli_count": mmdc_count,
                "mermaid_cli_avg_score": mmdc_avg,
            }
        )
        if mmdr_count:
            print(f"mmdr: {mmdr_count} fixtures  Avg score: {mmdr_avg:.2f}")
        if mmdc_count:
            print(f"mermaid-cli: {mmdc_count} fixtures  Avg score: {mmdc_avg:.2f}")
        mmdr_waste, mmdr_waste_count = summarize_metric(results.get("mmdr", {}), "wasted_space_ratio")
        mmdc_waste, mmdc_waste_count = summarize_metric(results.get("mermaid_cli", {}), "wasted_space_ratio")
        if mmdr_waste_count:
            print(f"mmdr: avg wasted space ratio: {mmdr_waste:.3f}")
        if mmdc_waste_count:
            print(f"mermaid-cli: avg wasted space ratio: {mmdc_waste:.3f}")
        mmdr_detour, mmdr_detour_count = summarize_metric(results.get("mmdr", {}), "avg_edge_detour_ratio")
        mmdc_detour, mmdc_detour_count = summarize_metric(results.get("mermaid_cli", {}), "avg_edge_detour_ratio")
        if mmdr_detour_count:
            print(f"mmdr: avg edge detour ratio: {mmdr_detour:.3f}")
        if mmdc_detour_count:
            print(f"mermaid-cli: avg edge detour ratio: {mmdc_detour:.3f}")
        mmdr_comp_gap, mmdr_comp_gap_count = summarize_metric(results.get("mmdr", {}), "component_gap_ratio")
        mmdc_comp_gap, mmdc_comp_gap_count = summarize_metric(
            results.get("mermaid_cli", {}), "component_gap_ratio"
        )
        if mmdr_comp_gap_count:
            print(f"mmdr: avg component gap ratio: {mmdr_comp_gap:.3f}")
        if mmdc_comp_gap_count:
            print(f"mermaid-cli: avg component gap ratio: {mmdc_comp_gap:.3f}")
        mmdr_label_oob, mmdr_label_oob_count = summarize_metric(
            results.get("mmdr", {}), "label_out_of_bounds_count"
        )
        mmdc_label_oob, mmdc_label_oob_count = summarize_metric(
            results.get("mermaid_cli", {}), "label_out_of_bounds_count"
        )
        if mmdr_label_oob_count:
            print(f"mmdr: avg label out-of-bounds count: {mmdr_label_oob:.3f}")
        if mmdc_label_oob_count:
            print(f"mermaid-cli: avg label out-of-bounds count: {mmdc_label_oob:.3f}")
        mmdr_intersections, mmdr_intersections_count = summarize_metric(
            results.get("mmdr", {}), "arrow_path_intersections"
        )
        mmdc_intersections, mmdc_intersections_count = summarize_metric(
            results.get("mermaid_cli", {}), "arrow_path_intersections"
        )
        if mmdr_intersections_count:
            print(f"mmdr: avg arrow-path intersections: {mmdr_intersections:.3f}")
        if mmdc_intersections_count:
            print(f"mermaid-cli: avg arrow-path intersections: {mmdc_intersections:.3f}")
        mmdr_label_align, mmdr_label_align_count = summarize_metric(
            results.get("mmdr", {}), "edge_label_alignment_mean"
        )
        mmdc_label_align, mmdc_label_align_count = summarize_metric(
            results.get("mermaid_cli", {}), "edge_label_alignment_mean"
        )
        mmdr_label_gap, mmdr_label_gap_count = summarize_metric(
            results.get("mmdr", {}), "edge_label_path_gap_mean"
        )
        mmdc_label_gap, mmdc_label_gap_count = summarize_metric(
            results.get("mermaid_cli", {}), "edge_label_path_gap_mean"
        )
        mmdr_label_clearance, mmdr_label_clearance_count = summarize_metric(
            results.get("mmdr", {}), "edge_label_path_clearance_score_mean"
        )
        mmdc_label_clearance, mmdc_label_clearance_count = summarize_metric(
            results.get("mermaid_cli", {}), "edge_label_path_clearance_score_mean"
        )
        mmdr_label_nontouch, mmdr_label_nontouch_count = summarize_metric(
            results.get("mmdr", {}), "edge_label_path_non_touch_ratio"
        )
        mmdc_label_nontouch, mmdc_label_nontouch_count = summarize_metric(
            results.get("mermaid_cli", {}), "edge_label_path_non_touch_ratio"
        )
        history_summary.update(
            {
                "mmdr_avg_wasted_space_ratio": mmdr_waste,
                "mermaid_cli_avg_wasted_space_ratio": mmdc_waste,
                "mmdr_avg_edge_detour_ratio": mmdr_detour,
                "mermaid_cli_avg_edge_detour_ratio": mmdc_detour,
                "mmdr_avg_component_gap_ratio": mmdr_comp_gap,
                "mermaid_cli_avg_component_gap_ratio": mmdc_comp_gap,
                "mmdr_avg_label_oob_count": mmdr_label_oob,
                "mermaid_cli_avg_label_oob_count": mmdc_label_oob,
                "mmdr_avg_arrow_path_intersections": mmdr_intersections,
                "mermaid_cli_avg_arrow_path_intersections": mmdc_intersections,
                "mmdr_avg_edge_label_distance": mmdr_label_align,
                "mermaid_cli_avg_edge_label_distance": mmdc_label_align,
                "mmdr_avg_edge_label_path_gap": mmdr_label_gap,
                "mermaid_cli_avg_edge_label_path_gap": mmdc_label_gap,
                "mmdr_avg_edge_label_clearance_score": mmdr_label_clearance,
                "mermaid_cli_avg_edge_label_clearance_score": mmdc_label_clearance,
                "mmdr_avg_edge_label_non_touch_ratio": mmdr_label_nontouch,
                "mermaid_cli_avg_edge_label_non_touch_ratio": mmdc_label_nontouch,
            }
        )
        if mmdr_label_align_count:
            print(f"mmdr: avg edge-label distance to nearest edge: {mmdr_label_align:.3f}")
        if mmdc_label_align_count:
            print(f"mermaid-cli: avg edge-label distance to nearest edge: {mmdc_label_align:.3f}")
        if mmdr_label_gap_count:
            print(f"mmdr: avg edge-label path gap (0=touching): {mmdr_label_gap:.3f}")
        if mmdc_label_gap_count:
            print(f"mermaid-cli: avg edge-label path gap (0=touching): {mmdc_label_gap:.3f}")
        if mmdr_label_clearance_count:
            print(
                "mmdr: avg edge-label clearance score "
                f"(touch=0, best near ~2px): {mmdr_label_clearance:.3f}"
            )
        if mmdc_label_clearance_count:
            print(
                "mermaid-cli: avg edge-label clearance score "
                f"(touch=0, best near ~2px): {mmdc_label_clearance:.3f}"
            )
        if mmdr_label_nontouch_count:
            print(f"mmdr: avg edge-label non-touch ratio: {mmdr_label_nontouch:.3f}")
        if mmdc_label_nontouch_count:
            print(f"mermaid-cli: avg edge-label non-touch ratio: {mmdc_label_nontouch:.3f}")
        comparison_stats = common_comparison_stats(
            results.get("mmdr", {}), results.get("mermaid_cli", {})
        )
        weighted_stats = weighted_dominance_stats(
            results.get("mmdr", {}), results.get("mermaid_cli", {})
        )
        for line in summarize_common_comparison(
            results.get("mmdr", {}), results.get("mermaid_cli", {})
        ):
            print(line)
        for line in summarize_weighted_dominance(
            results.get("mmdr", {}), results.get("mermaid_cli", {})
        ):
            print(line)
        for line in summarize_sequence_cli_conformance(results.get("mmdr", {})):
            print(line)
    else:
        scored = [(k, v) for k, v in payload.items() if isinstance(v, dict) and "score" in v]
        if scored:
            scores = sorted(scored, key=lambda kv: kv[1]["score"], reverse=True)
            top = scores[:5]
            avg = sum(v["score"] for _, v in scored) / len(scored)
            history_summary.update(
                {
                    "engine_count": len(scored),
                    "engine_avg_score": avg,
                    "engine_worst_fixture": top[0][0] if top else "",
                    "engine_worst_score": top[0][1]["score"] if top else None,
                }
            )
            print(f"Fixtures: {len(scored)}  Avg score: {avg:.2f}")
            print("Worst 5 by score:")
            for name, metrics in top:
                print(f"  {name}: {metrics['score']:.2f}")
            by_space = sorted(
                scored,
                key=lambda kv: kv[1].get("space_efficiency_penalty", 0.0),
                reverse=True,
            )[:5]
            print("Worst 5 by wasted-space penalty:")
            for name, metrics in by_space:
                print(
                    "  "
                    f"{name}: penalty={metrics.get('space_efficiency_penalty', 0.0):.3f} "
                    f"(wasted={metrics.get('wasted_space_ratio', 0.0):.2f}, "
                    f"fill={metrics.get('content_fill_ratio', 0.0):.2f})"
                )
            by_detour = sorted(
                scored,
                key=lambda kv: kv[1].get("avg_edge_detour_ratio", 1.0),
                reverse=True,
            )[:5]
            print("Worst 5 by edge detour ratio:")
            for name, metrics in by_detour:
                print(f"  {name}: detour={metrics.get('avg_edge_detour_ratio', 1.0):.2f}")
            by_label_oob = sorted(
                scored,
                key=lambda kv: kv[1].get("label_out_of_bounds_count", 0.0),
                reverse=True,
            )[:5]
            print("Worst 5 by label out-of-bounds:")
            for name, metrics in by_label_oob:
                print(
                    "  "
                    f"{name}: count={metrics.get('label_out_of_bounds_count', 0)}, "
                    f"ratio={metrics.get('label_out_of_bounds_ratio', 0.0):.3f}"
                )
            by_intersections = sorted(
                scored,
                key=lambda kv: kv[1].get("arrow_path_intersections", 0.0),
                reverse=True,
            )[:5]
            print("Worst 5 by arrow-path intersections:")
            for name, metrics in by_intersections:
                print(
                    "  "
                    f"{name}: intersections={metrics.get('arrow_path_intersections', 0)}, "
                    f"overlap={metrics.get('arrow_path_overlap_length', 0.0):.2f}"
                )
            by_label_alignment = sorted(
                scored,
                key=lambda kv: kv[1].get("edge_label_alignment_bad_count", 0.0),
                reverse=True,
            )[:5]
            print("Worst 5 by edge-label alignment misses:")
            for name, metrics in by_label_alignment:
                print(
                    "  "
                    f"{name}: bad={metrics.get('edge_label_alignment_bad_count', 0)}, "
                    f"mean={metrics.get('edge_label_alignment_mean', 0.0):.2f}"
                )
            by_label_gap = sorted(
                scored,
                key=lambda kv: kv[1].get("edge_label_path_gap_mean", 0.0),
                reverse=True,
            )[:5]
            print("Worst 5 by edge-label path gap (0 = touching):")
            for name, metrics in by_label_gap:
                print(
                    "  "
                    f"{name}: gap_mean={metrics.get('edge_label_path_gap_mean', 0.0):.2f}, "
                    f"touch_ratio={metrics.get('edge_label_path_touch_ratio', 0.0):.3f}"
                )
            by_clearance_score = sorted(
                scored,
                key=lambda kv: kv[1].get("edge_label_path_clearance_score_mean", 0.0),
            )[:5]
            print("Worst 5 by edge-label clearance score (touch = 0):")
            for name, metrics in by_clearance_score:
                print(
                    "  "
                    f"{name}: score={metrics.get('edge_label_path_clearance_score_mean', 0.0):.3f}, "
                    f"non_touch={metrics.get('edge_label_path_non_touch_ratio', 0.0):.3f}"
                )

    if not args.no_history_log:
        history_path = Path(args.history_log)
        record = {
            "timestamp_utc": run_started_at,
            "completed_utc": iso_utc_now(),
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
                "bin": str(bin_path),
                "mmdc_cache_dir": str(Path(args.mmdc_cache_dir)),
                "mmdc_cache_enabled": (not args.no_mmdc_cache),
            },
            "host": host_info,
            "git": git_info,
            "fixture_input_count": len(files),
            "summary": history_summary,
        }
        if comparison_stats:
            record["comparison"] = comparison_stats
        if weighted_stats:
            record["weighted_dominance"] = weighted_stats
        append_benchmark_history(history_path, record)


if __name__ == "__main__":
    main()
