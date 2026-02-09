// Label placement and collision avoidance for edge labels.
// Moved from render.rs — all functions here work with pure geometry,
// no SVG dependency.

use super::{EdgeLayout, NodeLayout, SubgraphLayout};
use crate::config::LayoutConfig;
use crate::ir::DiagramKind;
use crate::theme::Theme;
use std::collections::{BTreeMap, HashMap, HashSet};

const LABEL_OVERLAP_WIDE_THRESHOLD: f32 = 1e-4;
const LABEL_ANCHOR_FRACTIONS: [f32; 3] = [0.5, 0.35, 0.65];
const LABEL_ANCHOR_POS_EPS: f32 = 1.0;
const LABEL_ANCHOR_DIR_EPS: f32 = 0.02;

type Rect = (f32, f32, f32, f32);
type EdgeObstacle = (usize, Rect);

pub(crate) fn edge_label_padding(kind: DiagramKind, config: &LayoutConfig) -> (f32, f32) {
    match kind {
        DiagramKind::Requirement => (
            config.requirement.edge_label_padding_x,
            config.requirement.edge_label_padding_y,
        ),
        DiagramKind::State => (3.0, 1.6),
        DiagramKind::Flowchart => (4.5, 2.2),
        _ => (4.0, 2.0),
    }
}

pub(crate) fn endpoint_label_padding(kind: DiagramKind) -> (f32, f32) {
    match kind {
        DiagramKind::State => (2.6, 1.4),
        DiagramKind::Flowchart => (3.4, 1.8),
        DiagramKind::Class => (3.2, 1.6),
        _ => (3.0, 1.6),
    }
}

/// Resolve all edge label positions using collision avoidance.
///
/// After this function returns, every edge that has a label will have
/// `label_anchor` set to `Some(...)`. Edges with `start_label` or
/// `end_label` will have `start_label_anchor`/`end_label_anchor` set.
pub fn resolve_all_label_positions(
    layout: &mut super::Layout,
    theme: &Theme,
    config: &LayoutConfig,
) {
    // Sequence and ZenUML diagrams place labels inline with trivial midpoint
    // math in the renderer — skip them.
    if layout.kind == DiagramKind::Sequence || layout.kind == DiagramKind::ZenUML {
        return;
    }

    let bounds = Some((layout.width, layout.height));

    // Step 1: Resolve center labels (label_anchor).
    resolve_center_labels(
        &mut layout.edges,
        &layout.nodes,
        &layout.subgraphs,
        bounds,
        layout.kind,
        theme,
        config,
    );

    // Step 2: Resolve endpoint labels (start_label_anchor, end_label_anchor).
    resolve_endpoint_labels(
        &mut layout.edges,
        &layout.nodes,
        &layout.subgraphs,
        bounds,
        layout.kind,
        theme,
        config,
    );
}

/// Resolve center label positions for all edges, writing into `edge.label_anchor`.
fn resolve_center_labels(
    edges: &mut [EdgeLayout],
    nodes: &BTreeMap<String, NodeLayout>,
    subgraphs: &[SubgraphLayout],
    bounds: Option<(f32, f32)>,
    kind: DiagramKind,
    theme: &Theme,
    config: &LayoutConfig,
) {
    let (label_pad_x, label_pad_y) = edge_label_padding(kind, config);
    let node_obstacle_pad = (theme.font_size * 0.45).max(label_pad_x.max(label_pad_y));
    let edge_obstacle_pad = (theme.font_size * 0.35).max(label_pad_y);
    let step_normal_pad = (theme.font_size * 0.25).max(label_pad_y);
    let step_tangent_pad = (theme.font_size * 0.35).max(label_pad_x);
    let subgraph_label_pad = (theme.font_size * 0.35).max(3.0);

    let mut occupied: Vec<Rect> = build_label_obstacles(
        nodes,
        subgraphs,
        kind,
        theme,
        node_obstacle_pad,
        subgraph_label_pad,
    );
    let node_obstacle_count = occupied.len();
    let edge_obstacles = build_edge_obstacles(edges, edge_obstacle_pad);
    let edge_obs_rects: Vec<Rect> = edge_obstacles.iter().map(|(_, r)| *r).collect();
    let edge_grid = ObstacleGrid::new(48.0, &edge_obs_rects);
    let mut occupied_grid = ObstacleGrid::new(48.0, &occupied);

    // Sort edges by constraint level: shorter edges and edges with pre-set
    // anchors first, so they get first pick of placement spots.
    let mut order: Vec<usize> = (0..edges.len())
        .filter(|&i| edges[i].label.is_some())
        .collect();
    order.sort_by(|&a, &b| {
        let a_fixed = edges[a].label_anchor.is_some();
        let b_fixed = edges[b].label_anchor.is_some();
        // Pre-set anchors go first (they get first pick near their preferred spot).
        match (a_fixed, b_fixed) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        // Then by edge path length ascending (shorter = more constrained).
        let len_a = edge_path_length(&edges[a]);
        let len_b = edge_path_length(&edges[b]);
        len_a
            .partial_cmp(&len_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for idx in order {
        let label = match edges[idx].label.clone() {
            Some(l) => l,
            None => continue,
        };
        let pad_w = label.width + 2.0 * label_pad_x;
        let pad_h = label.height + 2.0 * label_pad_y;

        let edge = &edges[idx];
        let mut anchors: Vec<(f32, f32, f32, f32)> = Vec::new();

        if let Some((ax, ay)) = edges[idx].label_anchor {
            if let Some(candidate) = edge_label_anchor_from_point(edge, (ax, ay)) {
                push_anchor_unique(&mut anchors, candidate);
            }
        }
        for frac in LABEL_ANCHOR_FRACTIONS {
            if let Some(candidate) = edge_label_anchor_at_fraction(edge, frac) {
                push_anchor_unique(&mut anchors, candidate);
            }
        }
        if anchors.is_empty() {
            anchors.push(edge_label_anchor(edge));
        } else {
            push_anchor_unique(&mut anchors, edge_label_anchor(edge));
        }
        let normal_steps = [
            0.0, 0.15, -0.15, 0.35, -0.35, 0.6, -0.6, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0,
        ];
        let tangent_steps = [0.0, 0.2, -0.2, 0.6, -0.6, 1.2, -1.2];
        let mut best_pos = (anchors[0].0, anchors[0].1);
        let mut best_penalty = (f32::INFINITY, f32::INFINITY);
        let evaluate_candidates = |anchor: (f32, f32, f32, f32),
                                   tangents: &[f32],
                                   normals: &[f32],
                                   best_penalty: &mut (f32, f32),
                                   best_pos: &mut (f32, f32)| {
            let (anchor_x, anchor_y, dir_x, dir_y) = anchor;
            let normal_x = -dir_y;
            let normal_y = dir_x;
            let step_n = if normal_x.abs() > normal_y.abs() {
                label.width + label_pad_x + step_normal_pad
            } else {
                label.height + label_pad_y + step_normal_pad
            };
            let step_t = if dir_x.abs() > dir_y.abs() {
                label.width + label_pad_x + step_tangent_pad
            } else {
                label.height + label_pad_y + step_tangent_pad
            };
            for t in tangents {
                let base_x = anchor_x + dir_x * step_t * *t;
                let base_y = anchor_y + dir_y * step_t * *t;
                for n in normals {
                    let x = base_x + normal_x * step_n * *n;
                    let y = base_y + normal_y * step_n * *n;
                    let rect = (
                        x - label.width / 2.0 - label_pad_x,
                        y - label.height / 2.0 - label_pad_y,
                        pad_w,
                        pad_h,
                    );
                    let penalty = label_penalties(
                        rect,
                        (anchor_x, anchor_y),
                        label.width,
                        label.height,
                        &occupied,
                        &occupied_grid,
                        node_obstacle_count,
                        &edge_obstacles,
                        &edge_grid,
                        idx,
                        bounds,
                    );
                    if candidate_better(penalty, *best_penalty) {
                        *best_penalty = penalty;
                        *best_pos = (x, y);
                    }
                }
            }
        };
        for anchor in &anchors {
            evaluate_candidates(
                *anchor,
                &tangent_steps,
                &normal_steps,
                &mut best_penalty,
                &mut best_pos,
            );
        }
        if best_penalty.0 > LABEL_OVERLAP_WIDE_THRESHOLD {
            let normal_steps_wide = [0.0, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0, 4.0, -4.0, 5.0, -5.0];
            let tangent_steps_wide = [0.0, 0.6, -0.6, 1.2, -1.2, 1.8, -1.8, 2.4, -2.4];
            for anchor in &anchors {
                evaluate_candidates(
                    *anchor,
                    &tangent_steps_wide,
                    &normal_steps_wide,
                    &mut best_penalty,
                    &mut best_pos,
                );
            }
        }
        let clamped_pos = if let Some(bound) = bounds {
            clamp_label_center_to_bounds(
                best_pos,
                label.width,
                label.height,
                label_pad_x,
                label_pad_y,
                bound,
            )
        } else {
            best_pos
        };
        let rect = (
            clamped_pos.0 - label.width / 2.0 - label_pad_x,
            clamped_pos.1 - label.height / 2.0 - label_pad_y,
            pad_w,
            pad_h,
        );
        occupied_grid.insert(occupied.len(), &rect);
        occupied.push(rect);
        edges[idx].label_anchor = Some(clamped_pos);
    }
}

/// Resolve start/end label positions for all edges.
fn resolve_endpoint_labels(
    edges: &mut [EdgeLayout],
    nodes: &BTreeMap<String, NodeLayout>,
    subgraphs: &[SubgraphLayout],
    bounds: Option<(f32, f32)>,
    kind: DiagramKind,
    theme: &Theme,
    config: &LayoutConfig,
) {
    let has_endpoint_labels = edges
        .iter()
        .any(|e| e.start_label.is_some() || e.end_label.is_some());
    if !has_endpoint_labels {
        return;
    }

    let (center_pad_x, center_pad_y) = edge_label_padding(kind, config);
    let node_obstacle_pad = (theme.font_size * 0.45).max(center_pad_x.max(center_pad_y));
    let edge_obstacle_pad = (theme.font_size * 0.35).max(center_pad_y);
    let subgraph_label_pad = (theme.font_size * 0.35).max(3.0);
    let (endpoint_pad_x, endpoint_pad_y) = endpoint_label_padding(kind);

    let edge_obstacles = build_edge_obstacles(edges, edge_obstacle_pad);
    let edge_obs_rects: Vec<Rect> = edge_obstacles.iter().map(|(_, r)| *r).collect();
    let endpoint_edge_grid = ObstacleGrid::new(48.0, &edge_obs_rects);

    // Start with node/subgraph obstacles + center label positions as obstacles.
    let mut endpoint_occupied = build_label_obstacles(
        nodes,
        subgraphs,
        kind,
        theme,
        node_obstacle_pad,
        subgraph_label_pad,
    );
    let endpoint_node_obstacle_count = endpoint_occupied.len();
    for edge in edges.iter() {
        if let (Some(label), Some((ax, ay))) = (&edge.label, edge.label_anchor) {
            endpoint_occupied.push((
                ax - label.width / 2.0 - center_pad_x,
                ay - label.height / 2.0 - center_pad_y,
                label.width + 2.0 * center_pad_x,
                label.height + 2.0 * center_pad_y,
            ));
        }
    }

    let end_label_offset = match kind {
        DiagramKind::Class | DiagramKind::Flowchart => (theme.font_size * 0.75).max(9.0),
        _ => (theme.font_size * 0.6).max(8.0),
    };
    let state_font_size = if kind == DiagramKind::State {
        theme.font_size * 0.85
    } else {
        theme.font_size
    };
    let endpoint_label_scale = if kind == DiagramKind::State {
        (state_font_size / theme.font_size).min(1.0)
    } else {
        1.0
    };

    let mut endpoint_grid = ObstacleGrid::new(48.0, &endpoint_occupied);

    for idx in 0..edges.len() {
        // Start label
        if let Some(label) = edges[idx].start_label.clone() {
            let label_w = label.width * endpoint_label_scale;
            let label_h = label.height * endpoint_label_scale;
            if let Some((x, y)) = edge_endpoint_label_position_with_avoid(
                &edges[idx],
                idx,
                true,
                end_label_offset,
                label_w,
                label_h,
                endpoint_pad_x,
                endpoint_pad_y,
                &endpoint_occupied,
                &endpoint_grid,
                endpoint_node_obstacle_count,
                &edge_obstacles,
                &endpoint_edge_grid,
                bounds,
            ) {
                edges[idx].start_label_anchor = Some((x, y));
                let rect = (
                    x - label_w / 2.0 - endpoint_pad_x,
                    y - label_h / 2.0 - endpoint_pad_y,
                    label_w + endpoint_pad_x * 2.0,
                    label_h + endpoint_pad_y * 2.0,
                );
                endpoint_grid.insert(endpoint_occupied.len(), &rect);
                endpoint_occupied.push(rect);
            }
        }

        // End label
        if let Some(label) = edges[idx].end_label.clone() {
            let label_w = label.width * endpoint_label_scale;
            let label_h = label.height * endpoint_label_scale;
            if let Some((x, y)) = edge_endpoint_label_position_with_avoid(
                &edges[idx],
                idx,
                false,
                end_label_offset,
                label_w,
                label_h,
                endpoint_pad_x,
                endpoint_pad_y,
                &endpoint_occupied,
                &endpoint_grid,
                endpoint_node_obstacle_count,
                &edge_obstacles,
                &endpoint_edge_grid,
                bounds,
            ) {
                edges[idx].end_label_anchor = Some((x, y));
                let rect = (
                    x - label_w / 2.0 - endpoint_pad_x,
                    y - label_h / 2.0 - endpoint_pad_y,
                    label_w + endpoint_pad_x * 2.0,
                    label_h + endpoint_pad_y * 2.0,
                );
                endpoint_grid.insert(endpoint_occupied.len(), &rect);
                endpoint_occupied.push(rect);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Geometry helpers (moved from render.rs)
// ---------------------------------------------------------------------------

fn edge_path_length(edge: &EdgeLayout) -> f32 {
    let mut total = 0.0f32;
    for pair in edge.points.windows(2) {
        let dx = pair[1].0 - pair[0].0;
        let dy = pair[1].1 - pair[0].1;
        total += (dx * dx + dy * dy).sqrt();
    }
    total
}

fn subgraph_label_rect(sub: &SubgraphLayout, kind: DiagramKind, theme: &Theme) -> Option<Rect> {
    if sub.label.trim().is_empty() {
        return None;
    }
    let width = sub.label_block.width;
    let height = sub.label_block.height;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    if kind == DiagramKind::State {
        let header_h = (height + theme.font_size * 0.75).max(theme.font_size * 1.4);
        let label_pad_x = (theme.font_size * 0.6).max(height * 0.35);
        let x = sub.x + label_pad_x;
        let y = sub.y + header_h / 2.0 - height / 2.0;
        Some((x, y, width, height))
    } else {
        let x = sub.x + sub.width / 2.0 - width / 2.0;
        let y = sub.y + 12.0;
        Some((x, y, width, height))
    }
}

fn build_label_obstacles(
    nodes: &BTreeMap<String, NodeLayout>,
    subgraphs: &[SubgraphLayout],
    kind: DiagramKind,
    theme: &Theme,
    node_obstacle_pad: f32,
    subgraph_label_pad: f32,
) -> Vec<Rect> {
    let mut occupied: Vec<Rect> = Vec::new();
    for node in nodes.values() {
        if node.anchor_subgraph.is_some() || node.hidden {
            continue;
        }
        occupied.push((
            node.x - node_obstacle_pad,
            node.y - node_obstacle_pad,
            node.width + 2.0 * node_obstacle_pad,
            node.height + 2.0 * node_obstacle_pad,
        ));
    }
    for sub in subgraphs {
        if let Some(rect) = subgraph_label_rect(sub, kind, theme) {
            occupied.push((
                rect.0 - subgraph_label_pad,
                rect.1 - subgraph_label_pad,
                rect.2 + subgraph_label_pad * 2.0,
                rect.3 + subgraph_label_pad * 2.0,
            ));
        }
    }
    occupied
}

fn build_edge_obstacles(edges: &[EdgeLayout], pad: f32) -> Vec<EdgeObstacle> {
    let mut obstacles = Vec::new();
    for (idx, edge) in edges.iter().enumerate() {
        for segment in edge.points.windows(2) {
            let (a, b) = (segment[0], segment[1]);
            let min_x = a.0.min(b.0) - pad;
            let max_x = a.0.max(b.0) + pad;
            let min_y = a.1.min(b.1) - pad;
            let max_y = a.1.max(b.1) + pad;
            obstacles.push((idx, (min_x, min_y, max_x - min_x, max_y - min_y)));
        }
    }
    obstacles
}

fn edge_label_anchor(edge: &EdgeLayout) -> (f32, f32, f32, f32) {
    if edge.points.len() < 2 {
        return (0.0, 0.0, 1.0, 0.0);
    }
    let segment_count = edge.points.len() - 1;
    let mut best_idx: Option<usize> = None;
    let mut best_len = 0.0;

    let (start_idx, end_idx) = if segment_count >= 3 {
        (1, segment_count - 1)
    } else {
        (0, segment_count)
    };

    for idx in start_idx..end_idx {
        let p1 = edge.points[idx];
        let p2 = edge.points[idx + 1];
        let dx = p2.0 - p1.0;
        let dy = p2.1 - p1.1;
        let len = dx * dx + dy * dy;
        if len > best_len {
            best_len = len;
            best_idx = Some(idx);
        }
    }

    if best_idx.is_none() {
        for idx in 0..segment_count {
            let p1 = edge.points[idx];
            let p2 = edge.points[idx + 1];
            let dx = p2.0 - p1.0;
            let dy = p2.1 - p1.1;
            let len = dx * dx + dy * dy;
            if len > best_len {
                best_len = len;
                best_idx = Some(idx);
            }
        }
    }

    let idx = best_idx.unwrap_or(0);
    let p1 = edge.points[idx];
    let p2 = edge.points[idx + 1];
    let dx = p2.0 - p1.0;
    let dy = p2.1 - p1.1;
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    ((p1.0 + p2.0) / 2.0, (p1.1 + p2.1) / 2.0, dx / len, dy / len)
}

fn edge_label_anchor_at_fraction(edge: &EdgeLayout, t: f32) -> Option<(f32, f32, f32, f32)> {
    if edge.points.len() < 2 {
        return None;
    }
    let segment_count = edge.points.len() - 1;
    let (mut start_idx, mut end_idx) = if segment_count >= 3 {
        (1, segment_count - 1)
    } else {
        (0, segment_count)
    };
    if start_idx >= end_idx {
        start_idx = 0;
        end_idx = segment_count;
    }

    let mut total_len = 0.0f32;
    for idx in start_idx..end_idx {
        let p1 = edge.points[idx];
        let p2 = edge.points[idx + 1];
        let dx = p2.0 - p1.0;
        let dy = p2.1 - p1.1;
        total_len += (dx * dx + dy * dy).sqrt();
    }

    if total_len <= 1e-3 {
        return Some(edge_label_anchor(edge));
    }

    let mut remaining = total_len * t.clamp(0.0, 1.0);
    for idx in start_idx..end_idx {
        let p1 = edge.points[idx];
        let p2 = edge.points[idx + 1];
        let dx = p2.0 - p1.0;
        let dy = p2.1 - p1.1;
        let seg_len = (dx * dx + dy * dy).sqrt();
        if seg_len <= 1e-6 {
            continue;
        }
        if remaining <= seg_len {
            let alpha = (remaining / seg_len).clamp(0.0, 1.0);
            return Some((
                p1.0 + dx * alpha,
                p1.1 + dy * alpha,
                dx / seg_len,
                dy / seg_len,
            ));
        }
        remaining -= seg_len;
    }

    Some(edge_label_anchor(edge))
}

fn edge_label_anchor_from_point(
    edge: &EdgeLayout,
    point: (f32, f32),
) -> Option<(f32, f32, f32, f32)> {
    if edge.points.len() < 2 {
        return None;
    }
    let mut best_dist2 = f32::INFINITY;
    let mut best_dir: Option<(f32, f32)> = None;
    for segment in edge.points.windows(2) {
        let p1 = segment[0];
        let p2 = segment[1];
        let dx = p2.0 - p1.0;
        let dy = p2.1 - p1.1;
        let seg_len2 = dx * dx + dy * dy;
        if seg_len2 <= 1e-6 {
            continue;
        }
        let t = ((point.0 - p1.0) * dx + (point.1 - p1.1) * dy) / seg_len2;
        let t_clamped = t.clamp(0.0, 1.0);
        let proj_x = p1.0 + dx * t_clamped;
        let proj_y = p1.1 + dy * t_clamped;
        let dist2 =
            (point.0 - proj_x) * (point.0 - proj_x) + (point.1 - proj_y) * (point.1 - proj_y);
        if dist2 < best_dist2 {
            best_dist2 = dist2;
            best_dir = Some((dx, dy));
        }
    }
    let (dx, dy) = best_dir?;
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    Some((point.0, point.1, dx / len, dy / len))
}

fn push_anchor_unique(anchors: &mut Vec<(f32, f32, f32, f32)>, candidate: (f32, f32, f32, f32)) {
    let duplicate = anchors.iter().any(|anchor| {
        (anchor.0 - candidate.0).abs() <= LABEL_ANCHOR_POS_EPS
            && (anchor.1 - candidate.1).abs() <= LABEL_ANCHOR_POS_EPS
            && (anchor.2 - candidate.2).abs() <= LABEL_ANCHOR_DIR_EPS
            && (anchor.3 - candidate.3).abs() <= LABEL_ANCHOR_DIR_EPS
    });
    if !duplicate {
        anchors.push(candidate);
    }
}

fn overlap_area(a: &Rect, b: &Rect) -> f32 {
    let x0 = a.0.max(b.0);
    let y0 = a.1.max(b.1);
    let x1 = (a.0 + a.2).min(b.0 + b.2);
    let y1 = (a.1 + a.3).min(b.1 + b.3);
    let w = (x1 - x0).max(0.0);
    let h = (y1 - y0).max(0.0);
    w * h
}

fn outside_area(rect: &Rect, bounds: (f32, f32)) -> f32 {
    let (w, h) = bounds;
    let rect_area = rect.2.max(0.0) * rect.3.max(0.0);
    if rect_area <= 0.0 {
        return 0.0;
    }
    let x0 = rect.0.max(0.0);
    let y0 = rect.1.max(0.0);
    let x1 = (rect.0 + rect.2).min(w);
    let y1 = (rect.1 + rect.3).min(h);
    let inside_w = (x1 - x0).max(0.0);
    let inside_h = (y1 - y0).max(0.0);
    rect_area - inside_w * inside_h
}

fn clamp_label_center_to_bounds(
    center: (f32, f32),
    label_w: f32,
    label_h: f32,
    pad_x: f32,
    pad_y: f32,
    bounds: (f32, f32),
) -> (f32, f32) {
    let (w, h) = bounds;
    if w <= 0.0 || h <= 0.0 {
        return center;
    }
    let min_x = label_w * 0.5 + pad_x;
    let min_y = label_h * 0.5 + pad_y;
    let max_x = w - label_w * 0.5 - pad_x;
    let max_y = h - label_h * 0.5 - pad_y;

    let x = if max_x < min_x {
        w * 0.5
    } else {
        center.0.clamp(min_x, max_x)
    };
    let y = if max_y < min_y {
        h * 0.5
    } else {
        center.1.clamp(min_y, max_y)
    };
    (x, y)
}

/// Spatial index for fast overlap queries during label placement.
struct ObstacleGrid {
    cell: f32,
    /// Maps grid cell (ix, iy) to indices into the obstacle list.
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl ObstacleGrid {
    fn new(cell: f32, rects: &[Rect]) -> Self {
        let cell = cell.max(16.0);
        let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (i, rect) in rects.iter().enumerate() {
            let x0 = (rect.0 / cell).floor() as i32;
            let y0 = (rect.1 / cell).floor() as i32;
            let x1 = ((rect.0 + rect.2) / cell).floor() as i32;
            let y1 = ((rect.1 + rect.3) / cell).floor() as i32;
            for ix in x0..=x1 {
                for iy in y0..=y1 {
                    cells.entry((ix, iy)).or_default().push(i);
                }
            }
        }
        Self { cell, cells }
    }

    /// Add a new obstacle at the given index to the grid.
    fn insert(&mut self, idx: usize, rect: &Rect) {
        let x0 = (rect.0 / self.cell).floor() as i32;
        let y0 = (rect.1 / self.cell).floor() as i32;
        let x1 = ((rect.0 + rect.2) / self.cell).floor() as i32;
        let y1 = ((rect.1 + rect.3) / self.cell).floor() as i32;
        for ix in x0..=x1 {
            for iy in y0..=y1 {
                self.cells.entry((ix, iy)).or_default().push(idx);
            }
        }
    }

    /// Return indices of obstacles that could overlap with `rect`.
    fn query(&self, rect: &Rect) -> impl Iterator<Item = usize> + '_ {
        let x0 = (rect.0 / self.cell).floor() as i32;
        let y0 = (rect.1 / self.cell).floor() as i32;
        let x1 = ((rect.0 + rect.2) / self.cell).floor() as i32;
        let y1 = ((rect.1 + rect.3) / self.cell).floor() as i32;
        let mut seen = HashSet::new();
        (x0..=x1)
            .flat_map(move |ix| (y0..=y1).map(move |iy| (ix, iy)))
            .flat_map(move |key| {
                self.cells
                    .get(&key)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[])
                    .iter()
                    .copied()
            })
            .filter(move |idx| seen.insert(*idx))
    }
}

// Overlap penalty weights: node/subgraph overlap is worst, label overlap is
// moderate, edge overlap is mild (labels on edges is common and often acceptable).
const WEIGHT_NODE_OVERLAP: f32 = 1.0;
const WEIGHT_LABEL_OVERLAP: f32 = 0.7;
const WEIGHT_EDGE_OVERLAP: f32 = 0.25;
const WEIGHT_OUTSIDE: f32 = 1.2;

fn label_penalties(
    rect: Rect,
    anchor: (f32, f32),
    label_w: f32,
    label_h: f32,
    occupied: &[Rect],
    occupied_grid: &ObstacleGrid,
    node_obstacle_count: usize,
    edge_obstacles: &[EdgeObstacle],
    edge_grid: &ObstacleGrid,
    edge_idx: usize,
    bounds: Option<(f32, f32)>,
) -> (f32, f32) {
    let area = (label_w * label_h).max(1.0);
    let mut overlap = 0.0;
    for i in occupied_grid.query(&rect) {
        let ov = overlap_area(&rect, &occupied[i]);
        if ov > 0.0 {
            let weight = if i < node_obstacle_count {
                WEIGHT_NODE_OVERLAP
            } else {
                WEIGHT_LABEL_OVERLAP
            };
            overlap += ov * weight;
        }
    }
    for i in edge_grid.query(&rect) {
        let (idx, ref obs) = edge_obstacles[i];
        if idx == edge_idx {
            continue;
        }
        overlap += overlap_area(&rect, obs) * WEIGHT_EDGE_OVERLAP;
    }
    if let Some(bound) = bounds {
        overlap += outside_area(&rect, bound) * WEIGHT_OUTSIDE;
    }
    let dx = (rect.0 + rect.2 * 0.5) - anchor.0;
    let dy = (rect.1 + rect.3 * 0.5) - anchor.1;
    let dist = (dx * dx + dy * dy).sqrt();
    (overlap / area, dist / (label_w + label_h + 1.0))
}

fn candidate_better(candidate: (f32, f32), best: (f32, f32)) -> bool {
    if candidate.0 + 1e-6 < best.0 {
        return true;
    }
    (candidate.0 - best.0).abs() <= 1e-6 && candidate.1 + 1e-6 < best.1
}

pub(crate) fn edge_endpoint_label_position(
    edge: &EdgeLayout,
    start: bool,
    offset: f32,
) -> Option<(f32, f32)> {
    if edge.points.len() < 2 {
        return None;
    }
    let (p0, p1) = if start {
        (edge.points[0], edge.points[1])
    } else {
        (
            edge.points[edge.points.len() - 1],
            edge.points[edge.points.len() - 2],
        )
    };
    let dx = p1.0 - p0.0;
    let dy = p1.1 - p0.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        return None;
    }
    let dir_x = dx / len;
    let dir_y = dy / len;
    let base_x = p0.0 + dir_x * offset * 1.4;
    let base_y = p0.1 + dir_y * offset * 1.4;
    let perp_x = -dir_y;
    let perp_y = dir_x;
    Some((base_x + perp_x * offset, base_y + perp_y * offset))
}

fn edge_endpoint_label_position_with_avoid(
    edge: &EdgeLayout,
    edge_idx: usize,
    start: bool,
    offset: f32,
    label_w: f32,
    label_h: f32,
    pad_x: f32,
    pad_y: f32,
    occupied: &[Rect],
    occupied_grid: &ObstacleGrid,
    node_obstacle_count: usize,
    edge_obstacles: &[EdgeObstacle],
    edge_grid: &ObstacleGrid,
    bounds: Option<(f32, f32)>,
) -> Option<(f32, f32)> {
    if edge.points.len() < 2 {
        return None;
    }
    let (p0, p1) = if start {
        (edge.points[0], edge.points[1])
    } else {
        (
            edge.points[edge.points.len() - 1],
            edge.points[edge.points.len() - 2],
        )
    };
    let dx = p1.0 - p0.0;
    let dy = p1.1 - p0.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        return None;
    }
    let dir_x = dx / len;
    let dir_y = dy / len;
    let perp_x = -dir_y;
    let perp_y = dir_x;
    let anchor_x = p0.0 + dir_x * offset * 1.4;
    let anchor_y = p0.1 + dir_y * offset * 1.4;
    let along_steps = [0.0, 0.8, -0.8, 1.6, -1.6];
    let perp_steps = [
        1.0, -1.0, 1.7, -1.7, 2.4, -2.4, 3.2, -3.2, 3.9, -3.9, 4.6, -4.6,
    ];
    let mut best_pos = (anchor_x, anchor_y);
    let mut best_penalty = (f32::INFINITY, f32::INFINITY);
    for along in along_steps {
        let base_x = p0.0 + dir_x * offset * (1.4 + along);
        let base_y = p0.1 + dir_y * offset * (1.4 + along);
        for step in perp_steps {
            let x = base_x + perp_x * offset * step;
            let y = base_y + perp_y * offset * step;
            let rect = (
                x - label_w / 2.0 - pad_x,
                y - label_h / 2.0 - pad_y,
                label_w + pad_x * 2.0,
                label_h + pad_y * 2.0,
            );
            let penalty = label_penalties(
                rect,
                (anchor_x, anchor_y),
                label_w,
                label_h,
                occupied,
                occupied_grid,
                node_obstacle_count,
                edge_obstacles,
                edge_grid,
                edge_idx,
                bounds,
            );
            if candidate_better(penalty, best_penalty) {
                best_penalty = penalty;
                best_pos = (x, y);
            }
        }
    }
    if let Some(bound) = bounds {
        let clamped = clamp_label_center_to_bounds(best_pos, label_w, label_h, pad_x, pad_y, bound);
        return Some(clamped);
    }
    Some(best_pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_area_no_overlap() {
        let a: Rect = (0.0, 0.0, 10.0, 10.0);
        let b: Rect = (20.0, 20.0, 10.0, 10.0);
        assert_eq!(overlap_area(&a, &b), 0.0);
    }

    #[test]
    fn overlap_area_partial_overlap() {
        let a: Rect = (0.0, 0.0, 10.0, 10.0);
        let b: Rect = (5.0, 5.0, 10.0, 10.0);
        assert_eq!(overlap_area(&a, &b), 25.0);
    }

    #[test]
    fn overlap_area_contained() {
        let a: Rect = (0.0, 0.0, 20.0, 20.0);
        let b: Rect = (5.0, 5.0, 5.0, 5.0);
        assert_eq!(overlap_area(&a, &b), 25.0);
    }

    #[test]
    fn outside_area_fully_inside() {
        let rect: Rect = (10.0, 10.0, 20.0, 20.0);
        assert_eq!(outside_area(&rect, (100.0, 100.0)), 0.0);
    }

    #[test]
    fn outside_area_partially_outside() {
        let rect: Rect = (90.0, 0.0, 20.0, 10.0);
        // 10 pixels overhang on x, so 10*10 = 100 pixels outside
        assert_eq!(outside_area(&rect, (100.0, 100.0)), 100.0);
    }

    #[test]
    fn outside_area_fully_outside() {
        let rect: Rect = (200.0, 200.0, 10.0, 10.0);
        assert_eq!(outside_area(&rect, (100.0, 100.0)), 100.0);
    }

    #[test]
    fn clamp_label_center_stays_inside() {
        // Label 20x10 with 2px padding, bounds 100x100
        let result = clamp_label_center_to_bounds((5.0, 5.0), 20.0, 10.0, 2.0, 2.0, (100.0, 100.0));
        assert!(result.0 >= 12.0, "x should be clamped away from left edge");
        assert!(result.1 >= 7.0, "y should be clamped away from top edge");
    }

    #[test]
    fn clamp_label_center_no_op_when_inside() {
        let result =
            clamp_label_center_to_bounds((50.0, 50.0), 20.0, 10.0, 2.0, 2.0, (100.0, 100.0));
        assert_eq!(result, (50.0, 50.0));
    }

    #[test]
    fn obstacle_grid_query_finds_nearby_rect() {
        let rects = vec![(10.0, 10.0, 30.0, 30.0)];
        let grid = ObstacleGrid::new(20.0, &rects);
        let hits: Vec<usize> = grid.query(&(15.0, 15.0, 5.0, 5.0)).collect();
        assert!(hits.contains(&0), "grid should find overlapping rect");
    }

    #[test]
    fn obstacle_grid_query_misses_distant_rect() {
        let rects = vec![(10.0, 10.0, 30.0, 30.0)];
        let grid = ObstacleGrid::new(20.0, &rects);
        let hits: Vec<usize> = grid.query(&(200.0, 200.0, 5.0, 5.0)).collect();
        assert!(hits.is_empty(), "grid should not find distant rect");
    }

    #[test]
    fn obstacle_grid_insert_finds_new_item() {
        let initial: Vec<Rect> = vec![];
        let mut grid = ObstacleGrid::new(20.0, &initial);
        let new_rect: Rect = (50.0, 50.0, 10.0, 10.0);
        grid.insert(0, &new_rect);
        let hits: Vec<usize> = grid.query(&(55.0, 55.0, 1.0, 1.0)).collect();
        assert!(hits.contains(&0));
    }

    #[test]
    fn edge_label_anchor_midpoint() {
        let edge = EdgeLayout {
            from: "A".into(),
            to: "B".into(),
            points: vec![(0.0, 0.0), (100.0, 0.0)],
            label: None,
            start_label: None,
            end_label: None,
            label_anchor: None,
            start_label_anchor: None,
            end_label_anchor: None,
            directed: true,
            arrow_end: true,
            arrow_start: false,
            arrow_end_kind: None,
            arrow_start_kind: None,
            end_decoration: None,
            start_decoration: None,
            style: crate::ir::EdgeStyle::Solid,
            override_style: crate::ir::EdgeStyleOverride::default(),
        };
        let (x, y, _dx, _dy) = edge_label_anchor(&edge);
        assert!(
            (x - 50.0).abs() < 1.0,
            "midpoint x should be ~50, got {}",
            x
        );
        assert!((y - 0.0).abs() < 1.0, "midpoint y should be ~0, got {}", y);
    }

    #[test]
    fn edge_label_anchor_from_point_uses_nearest_segment() {
        let edge = EdgeLayout {
            from: "A".into(),
            to: "B".into(),
            points: vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)],
            label: None,
            start_label: None,
            end_label: None,
            label_anchor: None,
            start_label_anchor: None,
            end_label_anchor: None,
            directed: true,
            arrow_end: true,
            arrow_start: false,
            arrow_end_kind: None,
            arrow_start_kind: None,
            end_decoration: None,
            start_decoration: None,
            style: crate::ir::EdgeStyle::Solid,
            override_style: crate::ir::EdgeStyleOverride::default(),
        };
        let (_x, _y, dx, dy) =
            edge_label_anchor_from_point(&edge, (100.0, 60.0)).expect("anchor should resolve");
        assert!(
            dx.abs() < 0.1,
            "dx should be ~0 for vertical segment, got {}",
            dx
        );
        assert!(
            dy > 0.9,
            "dy should be positive for vertical segment, got {}",
            dy
        );
    }
}
