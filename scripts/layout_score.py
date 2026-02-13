#!/usr/bin/env python3
import argparse
import json
import math
from pathlib import Path


WEIGHTS = {
    # Crossing/overlap terms are still dominant readability drivers.
    "edge_crossings": 5.0,
    "edge_node_crossings": 6.0,
    "node_overlap_count": 12.0,
    "edge_bends": 2.0,
    "port_congestion": 2.0,
    "edge_overlap_length": 1.0,
    "crossing_angle_penalty": 3.0,
    "angular_resolution_penalty": 2.5,
    "node_spacing_violation_count": 2.0,
    "edge_node_near_miss_count": 1.5,
    # Normalize geometric terms so large diagrams are comparable with small ones.
    "edge_length_per_node": 0.75,
    "edge_detour_penalty": 80.0,
    # Explicit whitespace and composition quality.
    "space_efficiency_penalty": 320.0,
    "margin_imbalance_ratio": 160.0,
    # Ownership-aware edge-label quality (0 touch gap is worst).
    "edge_label_owned_path_touch_ratio": 180.0,
    "edge_label_owned_path_gap_bad_ratio": 160.0,
    "edge_label_owned_alignment_bad_ratio": 120.0,
    "edge_label_owned_path_clearance_penalty": 140.0,
    "edge_label_owned_path_gap_mean": 4.0,
}


def load_layout(path: Path):
    data = json.loads(path.read_text())
    nodes = {}
    for node in data.get("nodes", []):
        if node.get("hidden"):
            continue
        if node.get("anchor_subgraph") is not None:
            continue
        clean = dict(node)
        for key in ("x", "y", "width", "height"):
            try:
                clean[key] = float(clean.get(key, 0.0))
            except (TypeError, ValueError):
                clean[key] = 0.0
        nodes[node["id"]] = clean
    edges = data.get("edges", [])
    return data, nodes, edges


def dist(a, b):
    return math.hypot(a[0] - b[0], a[1] - b[1])


def safe_ratio(num, den, default=0.0):
    if abs(den) < 1e-9:
        return default
    return num / den


def clamp(value, lo, hi):
    return max(lo, min(hi, value))


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


def rect_contains(a, b, eps=1e-6, min_margin=1.0):
    ax1, ay1 = a["x"], a["y"]
    ax2, ay2 = ax1 + a["width"], ay1 + a["height"]
    bx1, by1 = b["x"], b["y"]
    bx2, by2 = bx1 + b["width"], by1 + b["height"]
    if bx1 < ax1 - eps or by1 < ay1 - eps or bx2 > ax2 + eps or by2 > ay2 + eps:
        return False
    left_margin = bx1 - ax1
    right_margin = ax2 - bx2
    top_margin = by1 - ay1
    bottom_margin = ay2 - by2
    # Require a visible margin so exact duplicates are still treated as overlaps.
    return (
        left_margin >= min_margin
        and right_margin >= min_margin
        and top_margin >= min_margin
        and bottom_margin >= min_margin
    )


def node_overlap_metrics(nodes, allow_containment=False):
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
                if allow_containment and (rect_contains(a, b) or rect_contains(b, a)):
                    continue
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


def crossing_angle_degrees(a, b, c, d):
    v1 = (b[0] - a[0], b[1] - a[1])
    v2 = (d[0] - c[0], d[1] - c[1])
    l1 = math.hypot(v1[0], v1[1])
    l2 = math.hypot(v2[0], v2[1])
    if l1 < 1e-9 or l2 < 1e-9:
        return 90.0
    # Use acute crossing angle as readability proxy.
    cosv = abs((v1[0] * v2[0] + v1[1] * v2[1]) / (l1 * l2))
    cosv = clamp(cosv, -1.0, 1.0)
    return math.degrees(math.acos(cosv))


def direction_from_points(a, b):
    dx = b[0] - a[0]
    dy = b[1] - a[1]
    length = math.hypot(dx, dy)
    if length < 1e-9:
        return None
    return (dx / length, dy / length)


def min_angular_resolution_penalty(node_dirs):
    if len(node_dirs) < 2:
        return None
    min_angle = 180.0
    for i in range(len(node_dirs)):
        for j in range(i + 1, len(node_dirs)):
            a = node_dirs[i]
            b = node_dirs[j]
            dot = clamp(a[0] * b[0] + a[1] * b[1], -1.0, 1.0)
            angle = math.degrees(math.acos(dot))
            if angle < min_angle:
                min_angle = angle
    penalty = max(0.0, (35.0 - min_angle) / 35.0)
    return min_angle, penalty


def compute_content_bounds(nodes, edges):
    min_x = float("inf")
    min_y = float("inf")
    max_x = float("-inf")
    max_y = float("-inf")
    seen = False

    for node in nodes.values():
        x1 = node["x"]
        y1 = node["y"]
        x2 = x1 + node["width"]
        y2 = y1 + node["height"]
        min_x = min(min_x, x1)
        min_y = min(min_y, y1)
        max_x = max(max_x, x2)
        max_y = max(max_y, y2)
        seen = True

    for edge in edges:
        for point in edge.get("points", []):
            px, py = point
            min_x = min(min_x, px)
            min_y = min(min_y, py)
            max_x = max(max_x, px)
            max_y = max(max_y, py)
            seen = True

    if not seen:
        return 0.0, 0.0, 0.0, 0.0
    return min_x, min_y, max_x, max_y


def connected_components(nodes, edges):
    adjacency = {node_id: set() for node_id in nodes}
    for edge in edges:
        from_id = edge.get("from")
        to_id = edge.get("to")
        if from_id in adjacency and to_id in adjacency:
            adjacency[from_id].add(to_id)
            adjacency[to_id].add(from_id)

    comps = []
    seen = set()
    for node_id in adjacency:
        if node_id in seen:
            continue
        stack = [node_id]
        seen.add(node_id)
        comp = []
        while stack:
            cur = stack.pop()
            comp.append(cur)
            for nxt in adjacency[cur]:
                if nxt in seen:
                    continue
                seen.add(nxt)
                stack.append(nxt)
        comps.append(comp)
    return comps


def compute_metrics(data, nodes, edges):
    total_edge_length = 0.0
    edge_bends = 0
    edge_crossings = 0
    edge_overlap_length = 0.0
    port_congestion = 0
    edge_node_crossings = 0
    edge_detour_sum = 0.0
    edge_detour_count = 0
    crossing_angle_penalty = 0.0
    crossing_count_with_angle = 0

    segments = []
    edge_points = []
    edge_node_near_miss_pairs = set()
    node_dirs = {}

    for idx, edge in enumerate(edges):
        points = [tuple(p) for p in edge.get("points", [])]
        edge_points.append(points)
        edge_bends += bend_count(points)
        path_len = 0.0
        for a, b in segments_from_points(points):
            seg_len = dist(a, b)
            path_len += seg_len
            total_edge_length += seg_len
            segments.append((idx, a, b))
        if len(points) >= 2:
            direct_len = dist(points[0], points[-1])
            if direct_len > 1e-3:
                edge_detour_sum += path_len / direct_len
                edge_detour_count += 1
            from_id = edge.get("from")
            to_id = edge.get("to")
            start_dir = direction_from_points(points[0], points[1])
            end_dir = direction_from_points(points[-1], points[-2])
            if from_id and start_dir is not None:
                node_dirs.setdefault(from_id, []).append(start_dir)
            if to_id and end_dir is not None:
                node_dirs.setdefault(to_id, []).append(end_dir)

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
                crossing_count_with_angle += 1
                angle = crossing_angle_degrees(a1, a2, b1, b2)
                crossing_angle_penalty += max(0.0, (35.0 - angle) / 35.0)
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

    node_ids = list(nodes.keys())
    node_spacing_violation_count = 0
    node_spacing_violation_severity = 0.0
    median_node_span = 0.0
    if node_ids:
        spans = []
        for node in nodes.values():
            spans.append(min(max(node["width"], 0.0), max(node["height"], 0.0)))
        spans = sorted(spans)
        median_node_span = spans[len(spans) // 2]
    spacing_target = clamp(median_node_span * 0.25 if median_node_span > 0.0 else 12.0, 8.0, 24.0)
    for i in range(len(node_ids)):
        a = nodes[node_ids[i]]
        ax1, ay1 = a["x"], a["y"]
        ax2, ay2 = ax1 + a["width"], ay1 + a["height"]
        for j in range(i + 1, len(node_ids)):
            b = nodes[node_ids[j]]
            bx1, by1 = b["x"], b["y"]
            bx2, by2 = bx1 + b["width"], by1 + b["height"]
            if ax1 < bx2 and bx1 < ax2 and ay1 < by2 and by1 < ay2:
                # Overlap is handled by node overlap metrics.
                continue
            gap_x = max(0.0, max(bx1 - ax2, ax1 - bx2))
            gap_y = max(0.0, max(by1 - ay2, ay1 - by2))
            gap = math.hypot(gap_x, gap_y)
            if gap < spacing_target:
                node_spacing_violation_count += 1
                node_spacing_violation_severity += (spacing_target - gap) / max(spacing_target, 1e-6)

    near_miss_pad = max(4.0, spacing_target * 0.5)
    for ei, a1, a2 in segments:
        edge = edges[ei]
        from_id = edge.get("from")
        to_id = edge.get("to")
        for node_id, node in nodes.items():
            if node_id == from_id or node_id == to_id:
                continue
            rect = (node["x"], node["y"], node["width"], node["height"])
            if segment_intersects_rect(a1, a2, rect):
                continue
            padded = (
                rect[0] - near_miss_pad,
                rect[1] - near_miss_pad,
                rect[2] + 2.0 * near_miss_pad,
                rect[3] + 2.0 * near_miss_pad,
            )
            if segment_intersects_rect(a1, a2, padded):
                edge_node_near_miss_pairs.add((ei, node_id))

    angular_resolution_penalty = 0.0
    low_angular_resolution_nodes = 0
    min_angular_resolution = 180.0
    for dirs in node_dirs.values():
        result = min_angular_resolution_penalty(dirs)
        if result is None:
            continue
        angle, penalty = result
        min_angular_resolution = min(min_angular_resolution, angle)
        angular_resolution_penalty += penalty
        if penalty > 0.0:
            low_angular_resolution_nodes += 1

    kind = str(data.get("kind", "")).strip().lower()
    allow_containment = kind == "treemap"
    overlap_count, overlap_area = node_overlap_metrics(nodes, allow_containment=allow_containment)
    node_area_total = sum(
        max(0.0, node["width"]) * max(0.0, node["height"]) for node in nodes.values()
    )
    width = data.get("width", 0.0) or 0.0
    height = data.get("height", 0.0) or 0.0
    layout_area = width * height
    node_count = len(nodes)
    edge_count = len(edges)

    min_x, min_y, max_x, max_y = compute_content_bounds(nodes, edges)
    content_width = max(0.0, max_x - min_x)
    content_height = max(0.0, max_y - min_y)
    content_bbox_area = content_width * content_height
    content_fill_ratio = safe_ratio(content_bbox_area, layout_area, default=0.0)
    # Keep bounded so pathological inputs (e.g. wrong canvas metadata) do not explode scores.
    content_fill_ratio = max(0.0, min(1.2, content_fill_ratio))
    wasted_space_ratio = max(0.0, 1.0 - min(1.0, content_fill_ratio))

    target_fill = 0.60
    space_efficiency_penalty = max(0.0, target_fill - content_fill_ratio)

    left_margin = max(0.0, min_x)
    right_margin = max(0.0, width - max_x)
    top_margin = max(0.0, min_y)
    bottom_margin = max(0.0, height - max_y)
    margin_imbalance_ratio = safe_ratio(abs(left_margin - right_margin), max(width, 1.0), default=0.0)
    margin_imbalance_ratio += safe_ratio(
        abs(top_margin - bottom_margin), max(height, 1.0), default=0.0
    )

    avg_edge_detour_ratio = (
        edge_detour_sum / edge_detour_count if edge_detour_count > 0 else 1.0
    )
    edge_detour_penalty = max(0.0, avg_edge_detour_ratio - 1.30)
    edge_length_per_node = safe_ratio(total_edge_length, max(node_count, 1), default=0.0)

    components = connected_components(nodes, edges)
    component_count = len(components)
    disconnected_components = max(0, component_count - 1)
    component_bbox_area_sum = 0.0
    component_areas = []
    for comp in components:
        c_min_x = float("inf")
        c_min_y = float("inf")
        c_max_x = float("-inf")
        c_max_y = float("-inf")
        for node_id in comp:
            node = nodes[node_id]
            x1 = node["x"]
            y1 = node["y"]
            x2 = x1 + node["width"]
            y2 = y1 + node["height"]
            c_min_x = min(c_min_x, x1)
            c_min_y = min(c_min_y, y1)
            c_max_x = max(c_max_x, x2)
            c_max_y = max(c_max_y, y2)
        if c_max_x > c_min_x and c_max_y > c_min_y:
            area = (c_max_x - c_min_x) * (c_max_y - c_min_y)
            component_bbox_area_sum += area
            component_areas.append(area)

    component_gap_ratio = max(
        0.0,
        1.0 - safe_ratio(component_bbox_area_sum, max(content_bbox_area, 1e-6), default=0.0),
    )
    component_balance_penalty = 0.0
    if len(component_areas) > 1:
        total_component_area = sum(component_areas)
        if total_component_area > 1e-9:
            shares = [area / total_component_area for area in component_areas]
            entropy = 0.0
            for share in shares:
                if share > 1e-12:
                    entropy -= share * math.log(share)
            max_entropy = math.log(len(shares))
            if max_entropy > 1e-12:
                component_balance_penalty = 1.0 - (entropy / max_entropy)

    content_aspect_ratio = safe_ratio(content_width, max(content_height, 1e-6), default=1.0)
    content_aspect_elongation = max(
        content_aspect_ratio,
        safe_ratio(1.0, max(content_aspect_ratio, 1e-6), default=1.0),
    )
    content_center_x = (min_x + max_x) * 0.5
    content_center_y = (min_y + max_y) * 0.5
    canvas_center_x = width * 0.5
    canvas_center_y = height * 0.5
    content_center_offset_ratio = safe_ratio(
        dist((content_center_x, content_center_y), (canvas_center_x, canvas_center_y)),
        max(math.hypot(width, height), 1.0),
        default=0.0,
    )
    overflow_x = max(0.0, -min_x) + max(0.0, max_x - width)
    overflow_y = max(0.0, -min_y) + max(0.0, max_y - height)
    content_overflow_ratio = safe_ratio(overflow_x, max(width, 1.0), 0.0) + safe_ratio(
        overflow_y, max(height, 1.0), 0.0
    )

    layout_area_per_node = safe_ratio(layout_area, max(node_count, 1), default=0.0)
    layout_area_per_edge = safe_ratio(layout_area, max(edge_count, 1), default=0.0)
    node_fill_ratio = safe_ratio(node_area_total, content_bbox_area, default=0.0)

    return {
        "node_count": node_count,
        "edge_count": edge_count,
        "edge_crossings": edge_crossings,
        "edge_node_crossings": edge_node_crossings,
        "total_edge_length": total_edge_length,
        "edge_length_per_node": edge_length_per_node,
        "edge_bends": edge_bends,
        "port_congestion": port_congestion,
        "edge_overlap_length": edge_overlap_length,
        "crossing_angle_penalty": crossing_angle_penalty,
        "crossing_count_with_angle": crossing_count_with_angle,
        "avg_crossing_angle_penalty": safe_ratio(crossing_angle_penalty, max(crossing_count_with_angle, 1), 0.0),
        "avg_edge_detour_ratio": avg_edge_detour_ratio,
        "edge_detour_penalty": edge_detour_penalty,
        "edge_node_near_miss_count": len(edge_node_near_miss_pairs),
        "layout_area": layout_area,
        "layout_area_per_node": layout_area_per_node,
        "layout_area_per_edge": layout_area_per_edge,
        "node_area_total": node_area_total,
        "node_fill_ratio": node_fill_ratio,
        "content_min_x": min_x,
        "content_min_y": min_y,
        "content_max_x": max_x,
        "content_max_y": max_y,
        "content_width": content_width,
        "content_height": content_height,
        "content_bbox_area": content_bbox_area,
        "content_aspect_ratio": content_aspect_ratio,
        "content_aspect_elongation": content_aspect_elongation,
        "content_fill_ratio": content_fill_ratio,
        "wasted_space_ratio": wasted_space_ratio,
        "space_efficiency_penalty": space_efficiency_penalty,
        "content_center_offset_ratio": content_center_offset_ratio,
        "content_overflow_ratio": content_overflow_ratio,
        "component_count": component_count,
        "disconnected_components": disconnected_components,
        "component_bbox_area_sum": component_bbox_area_sum,
        "component_gap_ratio": component_gap_ratio,
        "component_balance_penalty": component_balance_penalty,
        "node_spacing_target": spacing_target,
        "node_spacing_violation_count": node_spacing_violation_count,
        "node_spacing_violation_severity": node_spacing_violation_severity,
        "angular_resolution_penalty": angular_resolution_penalty,
        "low_angular_resolution_nodes": low_angular_resolution_nodes,
        "min_angular_resolution_deg": min_angular_resolution if min_angular_resolution < 180.0 else 180.0,
        "margin_left": left_margin,
        "margin_right": right_margin,
        "margin_top": top_margin,
        "margin_bottom": bottom_margin,
        "margin_imbalance_ratio": margin_imbalance_ratio,
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
