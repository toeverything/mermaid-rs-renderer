#!/usr/bin/env python3
import argparse
import json
import math
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path


def strip_ns(tag: str) -> str:
    if '}' in tag:
        return tag.split('}', 1)[1]
    return tag


def parse_transform(transform: str):
    if not transform:
        return 0.0, 0.0
    m = re.search(r"translate\(([^,\s]+)[,\s]+([^\)]+)\)", transform)
    if not m:
        return 0.0, 0.0
    return float(m.group(1)), float(m.group(2))


def parse_points(points: str):
    pts = []
    for part in points.replace(',', ' ').split():
        try:
            pts.append(float(part))
        except ValueError:
            continue
    return list(zip(pts[0::2], pts[1::2]))


def merge_bbox(bbox, other):
    if other is None:
        return bbox
    if bbox is None:
        return other
    x1, y1, x2, y2 = bbox
    ox1, oy1, ox2, oy2 = other
    return (min(x1, ox1), min(y1, oy1), max(x2, ox2), max(y2, oy2))


def bbox_from_shapes(elem, tx, ty):
    bbox = None
    for child in elem.iter():
        if child is elem:
            continue
        tag = strip_ns(child.tag)
        if tag == 'rect':
            try:
                x = float(child.attrib.get('x', '0')) + tx
                y = float(child.attrib.get('y', '0')) + ty
                w = float(child.attrib.get('width', '0'))
                h = float(child.attrib.get('height', '0'))
            except ValueError:
                continue
            if w <= 0 or h <= 0:
                continue
            bbox = merge_bbox(bbox, (x, y, x + w, y + h))
        elif tag == 'polygon':
            pts = parse_points(child.attrib.get('points', ''))
            if not pts:
                continue
            xs = [p[0] + tx for p in pts]
            ys = [p[1] + ty for p in pts]
            bbox = merge_bbox(bbox, (min(xs), min(ys), max(xs), max(ys)))
        elif tag == 'circle':
            try:
                cx = float(child.attrib.get('cx', '0')) + tx
                cy = float(child.attrib.get('cy', '0')) + ty
                r = float(child.attrib.get('r', '0'))
            except ValueError:
                continue
            if r <= 0:
                continue
            bbox = merge_bbox(bbox, (cx - r, cy - r, cx + r, cy + r))
        elif tag == 'ellipse':
            try:
                cx = float(child.attrib.get('cx', '0')) + tx
                cy = float(child.attrib.get('cy', '0')) + ty
                rx = float(child.attrib.get('rx', '0'))
                ry = float(child.attrib.get('ry', '0'))
            except ValueError:
                continue
            if rx <= 0 or ry <= 0:
                continue
            bbox = merge_bbox(bbox, (cx - rx, cy - ry, cx + rx, cy + ry))
    return bbox


def normalize_mermaid_id(raw_id: str):
    parts = raw_id.split('-')
    if len(parts) >= 3 and parts[-1].isdigit():
        return '-'.join(parts[1:-1])
    return raw_id


def parse_mermaid_svg(path: Path):
    root = ET.fromstring(path.read_text())
    nodes = {}
    clusters = {}

    for g in root.iter():
        if strip_ns(g.tag) != 'g':
            continue
        cls = g.attrib.get('class', '')
        gid = g.attrib.get('id')
        if gid and 'cluster' in cls and 'clusters' not in cls:
            rect = None
            for child in g:
                if strip_ns(child.tag) == 'rect':
                    rect = child
                    break
            if rect is not None:
                try:
                    x = float(rect.attrib.get('x', '0'))
                    y = float(rect.attrib.get('y', '0'))
                    w = float(rect.attrib.get('width', '0'))
                    h = float(rect.attrib.get('height', '0'))
                    clusters[gid] = {
                        'x': x,
                        'y': y,
                        'width': w,
                        'height': h,
                    }
                except ValueError:
                    pass
            continue
        if not gid:
            continue
        if 'node' not in cls or 'edge' in cls or 'label' in cls:
            continue
        tx, ty = parse_transform(g.attrib.get('transform', ''))
        bbox = bbox_from_shapes(g, tx, ty)
        if bbox is None:
            continue
        x1, y1, x2, y2 = bbox
        nodes[normalize_mermaid_id(gid)] = {
            'x': x1,
            'y': y1,
            'width': x2 - x1,
            'height': y2 - y1,
        }
    return nodes, clusters


def load_mmdr_layout(path: Path):
    data = json.loads(path.read_text())
    nodes = {}
    for node in data.get('nodes', []):
        if node.get('hidden'):
            continue
        if node.get('anchor_subgraph') is not None:
            continue
        nodes[node['id']] = node
    subgraphs = {sub['label']: sub for sub in data.get('subgraphs', [])}
    return nodes, subgraphs


def compute_diffs(mmdr_nodes, mer_nodes):
    diffs = []
    missing = []
    for node_id, node in mmdr_nodes.items():
        if node_id not in mer_nodes:
            missing.append(node_id)
            continue
        mer = mer_nodes[node_id]
        mx = node['x'] + node['width'] / 2.0
        my = node['y'] + node['height'] / 2.0
        ox = mer['x'] + mer['width'] / 2.0
        oy = mer['y'] + mer['height'] / 2.0
        dx = mx - ox
        dy = my - oy
        dist = math.hypot(dx, dy)
        diffs.append({
            'id': node_id,
            'dx': dx,
            'dy': dy,
            'distance': dist,
        })
    diffs.sort(key=lambda d: d['distance'], reverse=True)
    return diffs, missing


def summarize_diffs(diffs):
    if not diffs:
        return {
            'count': 0,
            'mean_abs_dx': 0.0,
            'mean_abs_dy': 0.0,
            'mean_distance': 0.0,
            'max_distance': 0.0,
        }
    mean_abs_dx = sum(abs(d['dx']) for d in diffs) / len(diffs)
    mean_abs_dy = sum(abs(d['dy']) for d in diffs) / len(diffs)
    mean_dist = sum(d['distance'] for d in diffs) / len(diffs)
    max_dist = max(d['distance'] for d in diffs)
    return {
        'count': len(diffs),
        'mean_abs_dx': mean_abs_dx,
        'mean_abs_dy': mean_abs_dy,
        'mean_distance': mean_dist,
        'max_distance': max_dist,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--mmdr-layout', required=True)
    parser.add_argument('--mermaid-svg', required=True)
    parser.add_argument('--output', required=False)
    args = parser.parse_args()

    mmdr_nodes, mmdr_subgraphs = load_mmdr_layout(Path(args.mmdr_layout))
    mer_nodes, mer_clusters = parse_mermaid_svg(Path(args.mermaid_svg))

    diffs, missing = compute_diffs(mmdr_nodes, mer_nodes)
    summary = summarize_diffs(diffs)
    report = {
        'summary': summary,
        'missing_nodes': missing,
        'top_nodes': diffs[:10],
    }

    if args.output:
        Path(args.output).write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))
    return 0


if __name__ == '__main__':
    sys.exit(main())
