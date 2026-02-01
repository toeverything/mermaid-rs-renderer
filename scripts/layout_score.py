#!/usr/bin/env python3
import argparse
import json
import math
from pathlib import Path


WEIGHTS = {
    "edge_crossings": 5.0,
    "edge_node_crossings": 6.0,
    "total_edge_length": 2.0,
    "edge_bends": 2.0,
    "port_congestion": 2.0,
    "edge_overlap_length": 1.0,
    "layout_area": 1.0,
}


def load_layout(path: Path):
    data = json.loads(path.read_text())
    nodes = {}
    for node in data.get("nodes", []):
        if node.get("hidden"):
            continue
        if node.get("anchor_subgraph") is not None:
            continue
        nodes[node["id"]] = node
    edges = data.get("edges", [])
    return data, nodes, edges


def dist(a, b):
    return math.hypot(a[0] - b[0], a[1] - b[1])


def segments_from_points(points):
    if len(points) < 2:
        return []
    return list(zip(points, points[1:]))


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


def bend_count(points, eps=1e-6):
    if len(points) < 3:
        return 0
    bends = 0
    for i in range(1, len(points) - 1):
        a = points[i - 1]
        b = points[i]
        c = points[i + 1]
        v1 = (b[0] - a[0], b[1] - a[1])
        v2 = (c[0] - b[0], c[1] - b[1])
        if abs(v1[0]) < eps and abs(v1[1]) < eps:
            continue
        if abs(v2[0]) < eps and abs(v2[1]) < eps:
            continue
        cross = v1[0] * v2[1] - v1[1] * v2[0]
        if abs(cross) > eps:
            bends += 1
    return bends


def infer_side(node, point, tol=1.0):
    x = node["x"]
    y = node["y"]
    w = node["width"]
    h = node["height"]
    px, py = point
    sides = {
        "left": abs(px - x),
        "right": abs(px - (x + w)),
        "top": abs(py - y),
        "bottom": abs(py - (y + h)),
    }
    side, delta = min(sides.items(), key=lambda item: item[1])
    if delta <= tol:
        return side
    return "unknown"


def node_overlap_metrics(nodes):
    ids = list(nodes.keys())
    overlap_count = 0
    overlap_area = 0.0
    for i in range(len(ids)):
        a = nodes[ids[i]]
        ax1, ay1 = a["x"], a["y"]
        ax2, ay2 = ax1 + a["width"], ay1 + a["height"]
        for j in range(i + 1, len(ids)):
            b = nodes[ids[j]]
            bx1, by1 = b["x"], b["y"]
            bx2, by2 = bx1 + b["width"], by1 + b["height"]
            ix1 = max(ax1, bx1)
            iy1 = max(ay1, by1)
            ix2 = min(ax2, bx2)
            iy2 = min(ay2, by2)
            if ix2 > ix1 and iy2 > iy1:
                overlap_count += 1
                overlap_area += (ix2 - ix1) * (iy2 - iy1)
    return overlap_count, overlap_area


def segment_intersects_rect(a, b, rect, eps=1e-6):
    x, y, w, h = rect
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
        if segments_intersect(a, b, c, d, eps=eps):
            return True
    return False


def compute_metrics(data, nodes, edges):
    total_edge_length = 0.0
    edge_bends = 0
    edge_crossings = 0
    edge_overlap_length = 0.0
    port_congestion = 0
    edge_node_crossings = 0

    segments = []
    edge_points = []

    for idx, edge in enumerate(edges):
        points = [tuple(p) for p in edge.get("points", [])]
        edge_points.append(points)
        edge_bends += bend_count(points)
        for a, b in segments_from_points(points):
            total_edge_length += dist(a, b)
            segments.append((idx, a, b))

    for i in range(len(segments)):
        ei, a1, a2 = segments[i]
        edge = edges[ei]
        from_id = edge.get("from")
        to_id = edge.get("to")
        for node_id, node in nodes.items():
            if node_id == from_id or node_id == to_id:
                continue
            rect = (node["x"], node["y"], node["width"], node["height"])
            if segment_intersects_rect(a1, a2, rect):
                edge_node_crossings += 1
        for j in range(i + 1, len(segments)):
            ej, b1, b2 = segments[j]
            if ei == ej:
                continue
            if dist(a1, b1) < 1e-6 or dist(a1, b2) < 1e-6 or dist(a2, b1) < 1e-6 or dist(a2, b2) < 1e-6:
                continue
            if segments_intersect(a1, a2, b1, b2):
                edge_crossings += 1
            edge_overlap_length += collinear_overlap_length(a1, a2, b1, b2)

    port_counts = {node_id: {"left": 0, "right": 0, "top": 0, "bottom": 0} for node_id in nodes}
    for edge, points in zip(edges, edge_points):
        if len(points) < 2:
            continue
        from_id = edge.get("from")
        to_id = edge.get("to")
        if from_id in nodes:
            side = infer_side(nodes[from_id], points[0])
            if side in port_counts[from_id]:
                port_counts[from_id][side] += 1
        if to_id in nodes:
            side = infer_side(nodes[to_id], points[-1])
            if side in port_counts[to_id]:
                port_counts[to_id][side] += 1

    for counts in port_counts.values():
        for count in counts.values():
            if count > 1:
                port_congestion += count - 1

    overlap_count, overlap_area = node_overlap_metrics(nodes)
    width = data.get("width", 0.0) or 0.0
    height = data.get("height", 0.0) or 0.0
    layout_area = width * height

    return {
        "node_count": len(nodes),
        "edge_count": len(edges),
        "edge_crossings": edge_crossings,
        "edge_node_crossings": edge_node_crossings,
        "total_edge_length": total_edge_length,
        "edge_bends": edge_bends,
        "port_congestion": port_congestion,
        "edge_overlap_length": edge_overlap_length,
        "layout_area": layout_area,
        "node_overlap_count": overlap_count,
        "node_overlap_area": overlap_area,
    }


def weighted_score(metrics):
    score = 0.0
    for key, weight in WEIGHTS.items():
        score += metrics.get(key, 0.0) * weight
    return score


def main():
    parser = argparse.ArgumentParser(description="Score layout dumps for objective metrics")
    parser.add_argument("--input", required=True, help="layout dump file or directory")
    parser.add_argument("--output", default="", help="write JSON summary to file")
    args = parser.parse_args()

    input_path = Path(args.input)
    if input_path.is_dir():
        files = sorted(input_path.glob("**/*-layout.json"))
    else:
        files = [input_path]

    results = {}
    for path in files:
        data, nodes, edges = load_layout(path)
        metrics = compute_metrics(data, nodes, edges)
        metrics["score"] = weighted_score(metrics)
        results[path.name] = metrics

    if args.output:
        Path(args.output).write_text(json.dumps(results, indent=2))
    else:
        print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
