#!/usr/bin/env python3
import argparse
import json
import math
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

PATH_NUM_RE = re.compile(r"[-+]?(?:\d*\.\d+|\d+)(?:[eE][-+]?\d+)?")
STATE_START_RE = re.compile(r".*_start-(\d+)$")
STATE_END_RE = re.compile(r".*_end-(\d+)$")
MMD_START_RE = re.compile(r"^__start_(\d+)__$")
MMD_END_RE = re.compile(r"^__end_(\d+)__$")


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
        elif tag == 'path':
            d = child.attrib.get('d', '')
            if not d:
                continue
            nums = PATH_NUM_RE.findall(d)
            if len(nums) < 2:
                continue
            vals = []
            for n in nums:
                try:
                    vals.append(float(n))
                except ValueError:
                    continue
            xs = vals[0::2]
            ys = vals[1::2]
            if not xs or not ys:
                continue
            xs = [x + tx for x in xs]
            ys = [y + ty for y in ys]
            bbox = merge_bbox(bbox, (min(xs), min(ys), max(xs), max(ys)))
        elif tag == 'line':
            try:
                x1 = float(child.attrib.get('x1', '0')) + tx
                y1 = float(child.attrib.get('y1', '0')) + ty
                x2 = float(child.attrib.get('x2', '0')) + tx
                y2 = float(child.attrib.get('y2', '0')) + ty
            except ValueError:
                continue
            bbox = merge_bbox(bbox, (min(x1, x2), min(y1, y2), max(x1, x2), max(y1, y2)))
    return bbox


def normalize_mermaid_id(raw_id: str):
    parts = raw_id.split('-')
    if len(parts) >= 3 and parts[-1].isdigit():
        return '-'.join(parts[1:-1])
    return raw_id


def extract_label_lines(elem):
    lines = []
    for el in elem.iter():
        if el.text and el.text.strip():
            lines.append(el.text.strip())
    return lines


def parse_mermaid_svg(path: Path):
    root = ET.fromstring(path.read_text())
    nodes = {}
    nodes_by_label = {}
    clusters = {}
    special_nodes = []

    def visit(elem, acc_tx, acc_ty):
        tx, ty = parse_transform(elem.attrib.get('transform', ''))
        cur_tx = acc_tx + tx
        cur_ty = acc_ty + ty

        if strip_ns(elem.tag) == 'g':
            cls = elem.attrib.get('class', '')
            gid = elem.attrib.get('id')
            if gid and 'cluster' in cls and 'clusters' not in cls:
                rect = None
                for child in elem:
                    if strip_ns(child.tag) == 'rect':
                        rect = child
                        break
                if rect is not None:
                    try:
                        x = float(rect.attrib.get('x', '0')) + cur_tx
                        y = float(rect.attrib.get('y', '0')) + cur_ty
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
            if gid:
                if 'node' in cls and 'edge' not in cls and 'label' not in cls:
                    bbox = bbox_from_shapes(elem, cur_tx, cur_ty)
                    if bbox is not None:
                        x1, y1, x2, y2 = bbox
                        node = {
                            'x': x1,
                            'y': y1,
                            'width': x2 - x1,
                            'height': y2 - y1,
                            'raw_id': gid,
                            'class': cls,
                        }
                        norm_id = normalize_mermaid_id(gid)
                        nodes[norm_id] = node
                        label_lines = extract_label_lines(elem)
                        if label_lines:
                            label = label_lines[0]
                            nodes_by_label.setdefault(label, node)
                        if (match := STATE_START_RE.match(gid)):
                            special_nodes.append({
                                'kind': 'start',
                                'index': int(match.group(1)),
                                'node': node,
                            })
                        elif (match := STATE_END_RE.match(gid)):
                            special_nodes.append({
                                'kind': 'end',
                                'index': int(match.group(1)),
                                'node': node,
                            })

        # Sequence diagrams use actor rects instead of node groups.
        if strip_ns(elem.tag) == 'rect':
            cls = elem.attrib.get('class', '')
            if 'actor-top' in cls:
                name = elem.attrib.get('name')
                if name and name not in nodes:
                    try:
                        x = float(elem.attrib.get('x', '0')) + cur_tx
                        y = float(elem.attrib.get('y', '0')) + cur_ty
                        w = float(elem.attrib.get('width', '0'))
                        h = float(elem.attrib.get('height', '0'))
                    except ValueError:
                        x = y = w = h = None
                    if w and h and w > 0 and h > 0:
                        node = {
                            'x': x,
                            'y': y,
                            'width': w,
                            'height': h,
                            'raw_id': name,
                            'class': cls,
                        }
                        nodes[name] = node
                        nodes_by_label.setdefault(name, node)

        for child in list(elem):
            visit(child, cur_tx, cur_ty)

    visit(root, 0.0, 0.0)
    return nodes, nodes_by_label, clusters, special_nodes


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


def _build_special_map(mmdr_nodes, mer_specials):
    if not mer_specials:
        return {}
    mmdr_starts = []
    mmdr_ends = []
    for node_id, node in mmdr_nodes.items():
        if (match := MMD_START_RE.match(node_id)):
            mmdr_starts.append((int(match.group(1)), node_id, node))
        elif (match := MMD_END_RE.match(node_id)):
            mmdr_ends.append((int(match.group(1)), node_id, node))

    mer_starts = [s for s in mer_specials if s['kind'] == 'start']
    mer_ends = [s for s in mer_specials if s['kind'] == 'end']

    special_map = {}
    for (_, node_id, _), mer in zip(sorted(mmdr_starts), sorted(mer_starts, key=lambda s: s['index'])):
        special_map[node_id] = mer['node']
    for (_, node_id, _), mer in zip(sorted(mmdr_ends), sorted(mer_ends, key=lambda s: s['index'])):
        special_map[node_id] = mer['node']
    return special_map


def compute_diffs(mmdr_nodes, mer_nodes, mer_labels, mer_specials):
    diffs = []
    missing = []
    special_map = _build_special_map(mmdr_nodes, mer_specials)
    for node_id, node in mmdr_nodes.items():
        if node_id in special_map:
            mer = special_map[node_id]
        elif node_id in mer_nodes:
            mer = mer_nodes[node_id]
        elif node_id in mer_labels:
            mer = mer_labels[node_id]
        else:
            missing.append(node_id)
            continue
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
    mer_nodes, mer_labels, mer_clusters, mer_specials = parse_mermaid_svg(Path(args.mermaid_svg))

    diffs, missing = compute_diffs(mmdr_nodes, mer_nodes, mer_labels, mer_specials)
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
