// Label placement and collision avoidance for edge labels.
// Moved from render.rs — all functions here work with pure geometry,
// no SVG dependency.

use super::{EdgeLayout, NodeLayout, SubgraphLayout};
use crate::config::LayoutConfig;
use crate::ir::DiagramKind;
use crate::theme::Theme;
use std::collections::{BTreeMap, HashMap, HashSet};

const LABEL_OVERLAP_WIDE_THRESHOLD: f32 = 1e-4;
const LABEL_ANCHOR_FRACTIONS: [f32; 5] = [0.5, 0.35, 0.65, 0.2, 0.8];
const LABEL_ANCHOR_POS_EPS: f32 = 1.0;
const LABEL_ANCHOR_DIR_EPS: f32 = 0.02;
const LABEL_EXTRA_SEGMENT_ANCHORS: usize = 6;
const FLOWCHART_LABEL_CLEARANCE_PAD: f32 = 1.5;

type Rect = (f32, f32, f32, f32);
type EdgeObstacle = (usize, Rect);

#[derive(Clone)]
struct FlowchartCenterLabelEntry {
    edge_idx: usize,
    label_w: f32,
    label_h: f32,
    initial_center: (f32, f32),
    current_center: (f32, f32),
    edge_points: Vec<(f32, f32)>,
    candidates: Vec<(f32, f32)>,
}

fn edge_distance_weight(kind: DiagramKind, overlap_pressure: f32) -> f32 {
    let base = match kind {
        DiagramKind::Flowchart => 0.42,
        DiagramKind::Class | DiagramKind::State => 0.20,
        _ => 0.16,
    };
    if overlap_pressure <= 0.025 {
        base
    } else if overlap_pressure <= 0.10 {
        if kind == DiagramKind::Flowchart {
            base * 0.82
        } else {
            base * 0.55
        }
    } else {
        if kind == DiagramKind::Flowchart {
            base * 0.58
        } else {
            base * 0.2
        }
    }
}

fn edge_target_distance(kind: DiagramKind, label_h: f32, label_pad_y: f32) -> f32 {
    match kind {
        // For flowcharts we want labels visually attached to the carrying edge.
        // Keep them close, but with enough clearance to avoid path contact.
        DiagramKind::Flowchart => (label_h * 0.52 + label_pad_y * 0.65 + 0.4).max(4.8),
        _ => (label_h * 0.65 + label_pad_y).max(6.0),
    }
}

fn sweep_bias(kind: DiagramKind, tangent_step: f32, normal_step: f32) -> f32 {
    let (normal_w, tangent_w) = match kind {
        DiagramKind::Flowchart => (0.018, 0.004),
        _ => (0.010, 0.003),
    };
    normal_step.abs() * normal_w + tangent_step.abs() * tangent_w
}

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
    let node_obstacle_pad = if kind == DiagramKind::Flowchart {
        (theme.font_size * 0.55).max(label_pad_x.max(label_pad_y + FLOWCHART_LABEL_CLEARANCE_PAD))
    } else {
        (theme.font_size * 0.45).max(label_pad_x.max(label_pad_y))
    };
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
    if kind == DiagramKind::Flowchart {
        occupied.extend(build_node_text_obstacles(
            nodes,
            (theme.font_size * 0.2).max(2.0),
        ));
    }
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
        // Larger labels are harder to place; give them first pick.
        let area_a = edges[a]
            .label
            .as_ref()
            .map(|label| label.width * label.height)
            .unwrap_or(0.0);
        let area_b = edges[b]
            .label
            .as_ref()
            .map(|label| label.width * label.height)
            .unwrap_or(0.0);
        if (area_a - area_b).abs() > 1e-3 {
            return area_b
                .partial_cmp(&area_a)
                .unwrap_or(std::cmp::Ordering::Equal);
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
        for candidate in edge_segment_anchors(edge, LABEL_EXTRA_SEGMENT_ANCHORS) {
            push_anchor_unique(&mut anchors, candidate);
        }
        if anchors.is_empty() {
            anchors.push(edge_label_anchor(edge));
        } else {
            push_anchor_unique(&mut anchors, edge_label_anchor(edge));
        }
        let (normal_steps, tangent_steps): (&[f32], &[f32]) = if kind == DiagramKind::Flowchart {
            // For flowcharts, prioritize candidate bands that keep labels clear of
            // their own edge while spreading along the edge before collapsing to
            // touching placements.
            (
                &[
                    0.6, -0.6, 1.0, -1.0, 1.4, -1.4, 0.35, -0.35, 2.0, -2.0, 2.8, -2.8, 0.0,
                ],
                &[0.0, 0.3, -0.3, 0.8, -0.8, 1.4, -1.4, 2.2, -2.2, 3.2, -3.2],
            )
        } else {
            (
                &[
                    0.0, 0.15, -0.15, 0.35, -0.35, 0.6, -0.6, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0,
                ],
                &[0.0, 0.2, -0.2, 0.6, -0.6, 1.2, -1.2, 2.0, -2.0, 3.0, -3.0],
            )
        };
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
                    let center = if let Some(bound) = bounds {
                        clamp_label_center_to_bounds(
                            (x, y),
                            label.width,
                            label.height,
                            label_pad_x,
                            label_pad_y,
                            bound,
                        )
                    } else {
                        (x, y)
                    };
                    let rect = (
                        center.0 - label.width / 2.0 - label_pad_x,
                        center.1 - label.height / 2.0 - label_pad_y,
                        pad_w,
                        pad_h,
                    );
                    let penalty = label_penalties(
                        rect,
                        (anchor_x, anchor_y),
                        label.width,
                        label.height,
                        kind,
                        &occupied,
                        &occupied_grid,
                        node_obstacle_count,
                        &edge_obstacles,
                        &edge_grid,
                        idx,
                        &edge.points,
                        bounds,
                    );
                    let overlap_pressure = penalty.0;
                    let edge_dist = point_polyline_distance(center, &edge.points);
                    let edge_target = edge_target_distance(kind, label.height, label_pad_y);
                    let edge_dist_weight = edge_distance_weight(kind, overlap_pressure);
                    let edge_dist_penalty =
                        ((edge_dist - edge_target).max(0.0) / edge_target) * edge_dist_weight;
                    let sweep_penalty = sweep_bias(kind, *t, *n);
                    let penalty = (penalty.0 + edge_dist_penalty + sweep_penalty, penalty.1);
                    if candidate_better(penalty, *best_penalty) {
                        *best_penalty = penalty;
                        *best_pos = center;
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
            let (normal_steps_wide, tangent_steps_wide): (&[f32], &[f32]) =
                if kind == DiagramKind::Flowchart {
                    (
                        &[
                            0.6, -0.6, 1.2, -1.2, 2.0, -2.0, 3.0, -3.0, 4.0, -4.0, 5.2, -5.2, 6.5,
                            -6.5, 0.0,
                        ],
                        &[
                            0.0, 0.8, -0.8, 1.6, -1.6, 2.6, -2.6, 3.8, -3.8, 5.2, -5.2, 6.6, -6.6,
                            8.0, -8.0, 10.0, -10.0,
                        ],
                    )
                } else {
                    (
                        &[0.0, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0, 4.0, -4.0, 5.0, -5.0],
                        &[
                            0.0, 0.8, -0.8, 1.6, -1.6, 2.4, -2.4, 3.2, -3.2, 4.2, -4.2, 5.4, -5.4,
                        ],
                    )
                };
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
        let occupied_rect = if kind == DiagramKind::Flowchart {
            inflate_rect(rect, FLOWCHART_LABEL_CLEARANCE_PAD)
        } else {
            rect
        };
        occupied_grid.insert(occupied.len(), &occupied_rect);
        occupied.push(occupied_rect);
        edges[idx].label_anchor = Some(clamped_pos);
    }

    if kind == DiagramKind::Flowchart {
        deoverlap_flowchart_center_labels(
            edges,
            nodes,
            subgraphs,
            bounds,
            theme,
            label_pad_x,
            label_pad_y,
        );
    }
}

fn deoverlap_flowchart_center_labels(
    edges: &mut [EdgeLayout],
    nodes: &BTreeMap<String, NodeLayout>,
    subgraphs: &[SubgraphLayout],
    bounds: Option<(f32, f32)>,
    theme: &Theme,
    label_pad_x: f32,
    label_pad_y: f32,
) {
    let step_normal_pad = (theme.font_size * 0.25).max(label_pad_y);
    let step_tangent_pad = (theme.font_size * 0.35).max(label_pad_x);
    let mut entries: Vec<FlowchartCenterLabelEntry> = Vec::new();
    for (idx, edge) in edges.iter().enumerate() {
        let (Some(label), Some(anchor)) = (&edge.label, edge.label_anchor) else {
            continue;
        };
        let initial_center = if let Some(bound) = bounds {
            clamp_label_center_to_bounds(
                anchor,
                label.width,
                label.height,
                label_pad_x,
                label_pad_y,
                bound,
            )
        } else {
            anchor
        };
        let candidates = flowchart_center_label_candidates(
            edge,
            initial_center,
            label.width,
            label.height,
            label_pad_x,
            label_pad_y,
            step_normal_pad,
            step_tangent_pad,
            bounds,
        );
        entries.push(FlowchartCenterLabelEntry {
            edge_idx: idx,
            label_w: label.width,
            label_h: label.height,
            initial_center,
            current_center: initial_center,
            edge_points: edge.points.clone(),
            candidates,
        });
    }
    if entries.len() < 2 {
        return;
    }

    let node_obstacle_pad =
        (theme.font_size * 0.55).max(label_pad_x.max(label_pad_y + FLOWCHART_LABEL_CLEARANCE_PAD));
    let subgraph_label_pad = (theme.font_size * 0.35).max(3.0);
    let mut fixed_obstacles = build_label_obstacles(
        nodes,
        subgraphs,
        DiagramKind::Flowchart,
        theme,
        node_obstacle_pad,
        subgraph_label_pad,
    );
    fixed_obstacles.extend(build_node_text_obstacles(
        nodes,
        (theme.font_size * 0.2).max(2.0),
    ));

    // Iterative global refinement: resolve the most conflicted labels first and
    // re-score against all other current placements.
    for _ in 0..10 {
        let current_rects: Vec<Rect> = entries
            .iter()
            .map(|entry| {
                flowchart_center_label_rect(
                    entry.current_center,
                    entry.label_w,
                    entry.label_h,
                    label_pad_x,
                    label_pad_y,
                )
            })
            .collect();
        let mut conflict_order: Vec<(f32, usize)> = Vec::new();
        for (i, rect) in current_rects.iter().enumerate() {
            let mut conflict_score = 0.0;
            for (j, other) in current_rects.iter().enumerate() {
                if i == j {
                    continue;
                }
                let ov = overlap_area(rect, other);
                if ov > 0.0 {
                    conflict_score += ov + 1.0;
                }
            }
            for obstacle in &fixed_obstacles {
                let ov = overlap_area(rect, obstacle);
                if ov > 0.0 {
                    conflict_score += ov * 1.6 + 1.0;
                }
            }
            if conflict_score > 0.0 {
                conflict_order.push((conflict_score, i));
            }
        }
        if conflict_order.is_empty() {
            break;
        }
        conflict_order.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let mut moved = false;
        for (_, entry_idx) in conflict_order {
            let entry_snapshot = entries[entry_idx].clone();
            let others: Vec<Rect> = entries
                .iter()
                .enumerate()
                .filter_map(|(i, entry)| {
                    if i == entry_idx {
                        None
                    } else {
                        Some(flowchart_center_label_rect(
                            entry.current_center,
                            entry.label_w,
                            entry.label_h,
                            label_pad_x,
                            label_pad_y,
                        ))
                    }
                })
                .collect();
            let mut best_center = entry_snapshot.current_center;
            let mut best_cost = flowchart_center_label_refine_cost(
                &entry_snapshot,
                entry_snapshot.current_center,
                label_pad_x,
                label_pad_y,
                &others,
                &fixed_obstacles,
            );
            for candidate in entry_snapshot.candidates.iter().copied() {
                if (candidate.0 - entry_snapshot.current_center.0).abs() <= 0.2
                    && (candidate.1 - entry_snapshot.current_center.1).abs() <= 0.2
                {
                    continue;
                }
                let cost = flowchart_center_label_refine_cost(
                    &entry_snapshot,
                    candidate,
                    label_pad_x,
                    label_pad_y,
                    &others,
                    &fixed_obstacles,
                );
                if candidate_better(cost, best_cost) {
                    best_cost = cost;
                    best_center = candidate;
                }
            }
            if (best_center.0 - entries[entry_idx].current_center.0).abs() > 0.2
                || (best_center.1 - entries[entry_idx].current_center.1).abs() > 0.2
            {
                entries[entry_idx].current_center = best_center;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }

    // If overlaps still remain, force-separate conflicted pairs by selecting a
    // non-overlapping candidate for one side of the pair.
    for _ in 0..6 {
        let current_rects: Vec<Rect> = entries
            .iter()
            .map(|entry| {
                flowchart_center_label_rect(
                    entry.current_center,
                    entry.label_w,
                    entry.label_h,
                    label_pad_x,
                    label_pad_y,
                )
            })
            .collect();
        let mut adjusted = false;
        'pair_search: for i in 0..entries.len() {
            for j in (i + 1)..entries.len() {
                if overlap_area(&current_rects[i], &current_rects[j]) <= LABEL_OVERLAP_WIDE_THRESHOLD
                {
                    continue;
                }
                for &move_idx in &[i, j] {
                    let entry_snapshot = entries[move_idx].clone();
                    let others: Vec<Rect> = current_rects
                        .iter()
                        .enumerate()
                        .filter_map(|(k, rect)| if k == move_idx { None } else { Some(*rect) })
                        .collect();
                    let mut best_center: Option<(f32, f32)> = None;
                    let mut best_cost = (f32::INFINITY, f32::INFINITY);
                    for candidate in entry_snapshot.candidates.iter().copied() {
                        let rect = flowchart_center_label_rect(
                            candidate,
                            entry_snapshot.label_w,
                            entry_snapshot.label_h,
                            label_pad_x,
                            label_pad_y,
                        );
                        if others.iter().any(|other| {
                            overlap_area(&rect, other) > LABEL_OVERLAP_WIDE_THRESHOLD
                        }) {
                            continue;
                        }
                        let cost = flowchart_center_label_refine_cost(
                            &entry_snapshot,
                            candidate,
                            label_pad_x,
                            label_pad_y,
                            &others,
                            &fixed_obstacles,
                        );
                        if best_center.is_none() || candidate_better(cost, best_cost) {
                            best_center = Some(candidate);
                            best_cost = cost;
                        }
                    }
                    if let Some(center) = best_center
                        && ((center.0 - entries[move_idx].current_center.0).abs() > 0.2
                            || (center.1 - entries[move_idx].current_center.1).abs() > 0.2)
                    {
                        entries[move_idx].current_center = center;
                        adjusted = true;
                        break 'pair_search;
                    }
                }
            }
        }
        if !adjusted {
            break;
        }
    }

    for entry in entries {
        edges[entry.edge_idx].label_anchor = Some(entry.current_center);
    }
}

fn flowchart_center_label_rect(
    center: (f32, f32),
    label_w: f32,
    label_h: f32,
    label_pad_x: f32,
    label_pad_y: f32,
) -> Rect {
    let base = (
        center.0 - label_w / 2.0 - label_pad_x,
        center.1 - label_h / 2.0 - label_pad_y,
        label_w + 2.0 * label_pad_x,
        label_h + 2.0 * label_pad_y,
    );
    inflate_rect(base, FLOWCHART_LABEL_CLEARANCE_PAD)
}

fn push_center_unique(centers: &mut Vec<(f32, f32)>, candidate: (f32, f32)) {
    let duplicate = centers.iter().any(|center| {
        (center.0 - candidate.0).abs() <= 0.35 && (center.1 - candidate.1).abs() <= 0.35
    });
    if !duplicate {
        centers.push(candidate);
    }
}

fn flowchart_center_label_candidates(
    edge: &EdgeLayout,
    initial_center: (f32, f32),
    label_w: f32,
    label_h: f32,
    label_pad_x: f32,
    label_pad_y: f32,
    step_normal_pad: f32,
    step_tangent_pad: f32,
    bounds: Option<(f32, f32)>,
) -> Vec<(f32, f32)> {
    let mut candidates = Vec::new();
    let mut push_candidate = |mut center: (f32, f32)| {
        if let Some(bound) = bounds {
            center = clamp_label_center_to_bounds(
                center,
                label_w,
                label_h,
                label_pad_x,
                label_pad_y,
                bound,
            );
        }
        push_center_unique(&mut candidates, center);
    };
    push_candidate(initial_center);

    let mut anchors: Vec<(f32, f32, f32, f32)> = Vec::new();
    if let Some(anchor) = edge_label_anchor_from_point(edge, initial_center) {
        push_anchor_unique(&mut anchors, anchor);
    }
    for frac in LABEL_ANCHOR_FRACTIONS {
        if let Some(anchor) = edge_label_anchor_at_fraction(edge, frac) {
            push_anchor_unique(&mut anchors, anchor);
        }
    }
    for anchor in edge_segment_anchors(edge, LABEL_EXTRA_SEGMENT_ANCHORS) {
        push_anchor_unique(&mut anchors, anchor);
    }
    if anchors.is_empty() {
        anchors.push(edge_label_anchor(edge));
    } else {
        push_anchor_unique(&mut anchors, edge_label_anchor(edge));
    }

    let normal_steps: &[f32] = &[
        0.6, -0.6, 1.0, -1.0, 1.4, -1.4, 0.35, -0.35, 2.0, -2.0, 2.8, -2.8, 4.0, -4.0, 5.2, -5.2,
        6.5, -6.5, 8.2, -8.2, 10.0, -10.0, 0.0,
    ];
    let tangent_steps: &[f32] = &[
        0.0, 0.3, -0.3, 0.8, -0.8, 1.4, -1.4, 2.2, -2.2, 3.2, -3.2, 4.6, -4.6, 6.0, -6.0, 8.0,
        -8.0, 10.0, -10.0, 12.5, -12.5, 15.0, -15.0, 18.0, -18.0,
    ];
    for (anchor_x, anchor_y, dir_x, dir_y) in anchors {
        let normal_x = -dir_y;
        let normal_y = dir_x;
        let step_n = if normal_x.abs() > normal_y.abs() {
            label_w + label_pad_x + step_normal_pad
        } else {
            label_h + label_pad_y + step_normal_pad
        };
        let step_t = if dir_x.abs() > dir_y.abs() {
            label_w + label_pad_x + step_tangent_pad
        } else {
            label_h + label_pad_y + step_tangent_pad
        };
        for t in tangent_steps {
            let base_x = anchor_x + dir_x * step_t * *t;
            let base_y = anchor_y + dir_y * step_t * *t;
            for n in normal_steps {
                let center = (
                    base_x + normal_x * step_n * *n,
                    base_y + normal_y * step_n * *n,
                );
                push_candidate(center);
            }
        }
    }
    candidates
}

fn flowchart_center_label_refine_cost(
    entry: &FlowchartCenterLabelEntry,
    center: (f32, f32),
    label_pad_x: f32,
    label_pad_y: f32,
    others: &[Rect],
    fixed_obstacles: &[Rect],
) -> (f32, f32) {
    let rect = flowchart_center_label_rect(
        center,
        entry.label_w,
        entry.label_h,
        label_pad_x,
        label_pad_y,
    );
    let area = (entry.label_w * entry.label_h).max(1.0);

    let mut overlap_area_sum = 0.0f32;
    let mut overlap_count = 0u32;
    for other in others {
        let ov = overlap_area(&rect, other);
        if ov > 0.0 {
            overlap_area_sum += ov;
            overlap_count += 1;
        }
    }

    let mut fixed_overlap_area = 0.0f32;
    let mut fixed_overlap_count = 0u32;
    for obstacle in fixed_obstacles {
        let ov = overlap_area(&rect, obstacle);
        if ov > 0.0 {
            fixed_overlap_area += ov;
            fixed_overlap_count += 1;
        }
    }

    let own_edge_dist = polyline_rect_distance(&entry.edge_points, &rect);
    let mut own_edge_penalty = 0.0f32;
    if own_edge_dist.is_finite() {
        let target_gap = OWN_EDGE_GAP_TARGET_FLOWCHART.max(1e-3);
        if own_edge_dist < target_gap {
            let shortage = (target_gap - own_edge_dist) / target_gap;
            own_edge_penalty += shortage * shortage * 3.5;
        } else {
            let excess = (own_edge_dist - target_gap) / target_gap;
            own_edge_penalty += excess * excess * 0.12;
        }
        if own_edge_dist <= 0.35 {
            own_edge_penalty += 12.0;
        }
    }
    let edge_center_dist = point_polyline_distance(center, &entry.edge_points);
    let edge_target = edge_target_distance(DiagramKind::Flowchart, entry.label_h, label_pad_y);
    let edge_center_penalty =
        ((edge_center_dist - edge_target).max(0.0) / edge_target.max(1e-3)) * 0.45;
    let primary = fixed_overlap_count as f32 * 110.0
        + (fixed_overlap_area / area) * 40.0
        + overlap_count as f32 * 80.0
        + (overlap_area_sum / area) * 30.0
        + own_edge_penalty
        + edge_center_penalty;
    let dx = center.0 - entry.initial_center.0;
    let dy = center.1 - entry.initial_center.1;
    let drift = (dx * dx + dy * dy).sqrt() / (entry.label_w + entry.label_h + 1.0);
    (primary, drift)
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
            let rect = (
                ax - label.width / 2.0 - center_pad_x,
                ay - label.height / 2.0 - center_pad_y,
                label.width + 2.0 * center_pad_x,
                label.height + 2.0 * center_pad_y,
            );
            let occupied_rect = if kind == DiagramKind::Flowchart {
                inflate_rect(rect, FLOWCHART_LABEL_CLEARANCE_PAD)
            } else {
                rect
            };
            endpoint_occupied.push(occupied_rect);
        }
    }

    let end_label_offset = match kind {
        DiagramKind::Class => (theme.font_size * 1.05).max(12.0),
        DiagramKind::Flowchart => (theme.font_size * 0.75).max(9.0),
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
                kind,
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
                let occupied_rect = if kind == DiagramKind::Flowchart {
                    inflate_rect(rect, FLOWCHART_LABEL_CLEARANCE_PAD)
                } else {
                    rect
                };
                endpoint_grid.insert(endpoint_occupied.len(), &occupied_rect);
                endpoint_occupied.push(occupied_rect);
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
                kind,
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
                let occupied_rect = if kind == DiagramKind::Flowchart {
                    inflate_rect(rect, FLOWCHART_LABEL_CLEARANCE_PAD)
                } else {
                    rect
                };
                endpoint_grid.insert(endpoint_occupied.len(), &occupied_rect);
                endpoint_occupied.push(occupied_rect);
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

fn point_segment_distance(point: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let vx = b.0 - a.0;
    let vy = b.1 - a.1;
    let len2 = vx * vx + vy * vy;
    if len2 <= 1e-6 {
        let dx = point.0 - a.0;
        let dy = point.1 - a.1;
        return (dx * dx + dy * dy).sqrt();
    }
    let t = ((point.0 - a.0) * vx + (point.1 - a.1) * vy) / len2;
    let t = t.clamp(0.0, 1.0);
    let proj_x = a.0 + vx * t;
    let proj_y = a.1 + vy * t;
    let dx = point.0 - proj_x;
    let dy = point.1 - proj_y;
    (dx * dx + dy * dy).sqrt()
}

fn point_polyline_distance(point: (f32, f32), points: &[(f32, f32)]) -> f32 {
    if points.len() < 2 {
        return 0.0;
    }
    let mut best = f32::INFINITY;
    for seg in points.windows(2) {
        let dist = point_segment_distance(point, seg[0], seg[1]);
        if dist < best {
            best = dist;
        }
    }
    if best.is_finite() { best } else { 0.0 }
}

fn point_rect_distance(point: (f32, f32), rect: &Rect) -> f32 {
    let min_x = rect.0;
    let min_y = rect.1;
    let max_x = rect.0 + rect.2;
    let max_y = rect.1 + rect.3;
    let dx = if point.0 < min_x {
        min_x - point.0
    } else if point.0 > max_x {
        point.0 - max_x
    } else {
        0.0
    };
    let dy = if point.1 < min_y {
        min_y - point.1
    } else if point.1 > max_y {
        point.1 - max_y
    } else {
        0.0
    };
    (dx * dx + dy * dy).sqrt()
}

fn point_inside_rect(point: (f32, f32), rect: &Rect) -> bool {
    point.0 >= rect.0
        && point.0 <= rect.0 + rect.2
        && point.1 >= rect.1
        && point.1 <= rect.1 + rect.3
}

fn orientation(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn point_on_segment(point: (f32, f32), a: (f32, f32), b: (f32, f32), eps: f32) -> bool {
    point.0 >= a.0.min(b.0) - eps
        && point.0 <= a.0.max(b.0) + eps
        && point.1 >= a.1.min(b.1) - eps
        && point.1 <= a.1.max(b.1) + eps
}

fn segments_intersect(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) -> bool {
    let eps = 1e-4;
    let o1 = orientation(a, b, c);
    let o2 = orientation(a, b, d);
    let o3 = orientation(c, d, a);
    let o4 = orientation(c, d, b);
    let crosses = ((o1 > eps && o2 < -eps) || (o1 < -eps && o2 > eps))
        && ((o3 > eps && o4 < -eps) || (o3 < -eps && o4 > eps));
    if crosses {
        return true;
    }
    if o1.abs() <= eps && point_on_segment(c, a, b, eps) {
        return true;
    }
    if o2.abs() <= eps && point_on_segment(d, a, b, eps) {
        return true;
    }
    if o3.abs() <= eps && point_on_segment(a, c, d, eps) {
        return true;
    }
    if o4.abs() <= eps && point_on_segment(b, c, d, eps) {
        return true;
    }
    false
}

fn segment_intersects_rect(a: (f32, f32), b: (f32, f32), rect: &Rect) -> bool {
    if point_inside_rect(a, rect) || point_inside_rect(b, rect) {
        return true;
    }
    let x0 = rect.0;
    let y0 = rect.1;
    let x1 = rect.0 + rect.2;
    let y1 = rect.1 + rect.3;
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    corners
        .iter()
        .zip(corners.iter().cycle().skip(1))
        .take(4)
        .any(|(c0, c1)| segments_intersect(a, b, *c0, *c1))
}

fn segment_rect_distance(a: (f32, f32), b: (f32, f32), rect: &Rect) -> f32 {
    if segment_intersects_rect(a, b, rect) {
        return 0.0;
    }
    let mut best = point_rect_distance(a, rect).min(point_rect_distance(b, rect));
    let x0 = rect.0;
    let y0 = rect.1;
    let x1 = rect.0 + rect.2;
    let y1 = rect.1 + rect.3;
    for corner in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
        best = best.min(point_segment_distance(corner, a, b));
    }
    best
}

fn polyline_rect_distance(points: &[(f32, f32)], rect: &Rect) -> f32 {
    if points.len() < 2 {
        return f32::INFINITY;
    }
    let mut best = f32::INFINITY;
    for seg in points.windows(2) {
        let dist = segment_rect_distance(seg[0], seg[1], rect);
        if dist < best {
            best = dist;
        }
        if best <= 0.0 {
            break;
        }
    }
    best
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

fn build_node_text_obstacles(nodes: &BTreeMap<String, NodeLayout>, pad: f32) -> Vec<Rect> {
    let mut occupied = Vec::new();
    for node in nodes.values() {
        if node.anchor_subgraph.is_some() || node.hidden {
            continue;
        }
        if node.label.width <= 0.0 || node.label.height <= 0.0 {
            continue;
        }
        let cx = node.x + node.width * 0.5;
        let cy = node.y + node.height * 0.5;
        occupied.push((
            cx - node.label.width * 0.5 - pad,
            cy - node.label.height * 0.5 - pad,
            node.label.width + pad * 2.0,
            node.label.height + pad * 2.0,
        ));
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
    let mut best_proj: Option<(f32, f32)> = None;
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
            best_proj = Some((proj_x, proj_y));
            best_dir = Some((dx, dy));
        }
    }
    let (proj_x, proj_y) = best_proj?;
    let (dx, dy) = best_dir?;
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    Some((proj_x, proj_y, dx / len, dy / len))
}

fn edge_segment_anchors(edge: &EdgeLayout, max_count: usize) -> Vec<(f32, f32, f32, f32)> {
    if edge.points.len() < 2 || max_count == 0 {
        return Vec::new();
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
    let mut scored: Vec<(f32, (f32, f32, f32, f32))> = Vec::new();
    for idx in start_idx..end_idx {
        let p1 = edge.points[idx];
        let p2 = edge.points[idx + 1];
        let dx = p2.0 - p1.0;
        let dy = p2.1 - p1.1;
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 1.0 {
            continue;
        }
        let dir_x = dx / len;
        let dir_y = dy / len;
        scored.push((
            len,
            ((p1.0 + p2.0) * 0.5, (p1.1 + p2.1) * 0.5, dir_x, dir_y),
        ));
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored
        .into_iter()
        .take(max_count)
        .map(|(_, anchor)| anchor)
        .collect()
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

fn inflate_rect(rect: Rect, pad: f32) -> Rect {
    if pad <= 0.0 {
        return rect;
    }
    (
        rect.0 - pad,
        rect.1 - pad,
        rect.2 + pad * 2.0,
        rect.3 + pad * 2.0,
    )
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
const WEIGHT_NODE_OVERLAP: f32 = 1.6;
const WEIGHT_NODE_OVERLAP_FLOWCHART: f32 = 2.6;
const WEIGHT_LABEL_OVERLAP: f32 = 1.0;
const WEIGHT_FLOWCHART_LABEL_OVERLAP: f32 = 1.5;
const WEIGHT_EDGE_OVERLAP: f32 = 0.45;
const WEIGHT_FLOWCHART_EDGE_OVERLAP: f32 = 0.25;
const WEIGHT_OUTSIDE: f32 = 1.2;
const OWN_EDGE_GAP_TARGET: f32 = 1.2;
const OWN_EDGE_GAP_TARGET_FLOWCHART: f32 = 1.8;
const OWN_EDGE_GAP_UNDER_WEIGHT: f32 = 0.7;
const OWN_EDGE_GAP_UNDER_WEIGHT_FLOWCHART: f32 = 1.6;
const OWN_EDGE_GAP_OVER_WEIGHT: f32 = 0.06;
const OWN_EDGE_GAP_OVER_WEIGHT_FLOWCHART: f32 = 0.05;
const OWN_EDGE_TOUCH_HARD_PENALTY: f32 = 0.25;
const OWN_EDGE_TOUCH_HARD_PENALTY_FLOWCHART: f32 = 1.25;

fn label_penalties(
    rect: Rect,
    anchor: (f32, f32),
    label_w: f32,
    label_h: f32,
    kind: DiagramKind,
    occupied: &[Rect],
    occupied_grid: &ObstacleGrid,
    node_obstacle_count: usize,
    edge_obstacles: &[EdgeObstacle],
    edge_grid: &ObstacleGrid,
    edge_idx: usize,
    own_edge_points: &[(f32, f32)],
    bounds: Option<(f32, f32)>,
) -> (f32, f32) {
    let area = (label_w * label_h).max(1.0);
    let mut overlap = 0.0;
    let label_weight = if kind == DiagramKind::Flowchart {
        WEIGHT_FLOWCHART_LABEL_OVERLAP
    } else {
        WEIGHT_LABEL_OVERLAP
    };
    let edge_weight = if kind == DiagramKind::Flowchart {
        WEIGHT_FLOWCHART_EDGE_OVERLAP
    } else {
        WEIGHT_EDGE_OVERLAP
    };
    for i in occupied_grid.query(&rect) {
        let ov = overlap_area(&rect, &occupied[i]);
        if ov > 0.0 {
            let weight = if i < node_obstacle_count {
                if kind == DiagramKind::Flowchart {
                    WEIGHT_NODE_OVERLAP_FLOWCHART
                } else {
                    WEIGHT_NODE_OVERLAP
                }
            } else {
                label_weight
            };
            overlap += ov * weight;
        }
    }
    for i in edge_grid.query(&rect) {
        let (idx, ref obs) = edge_obstacles[i];
        if idx == edge_idx {
            continue;
        }
        overlap += overlap_area(&rect, obs) * edge_weight;
    }
    if let Some(bound) = bounds {
        overlap += outside_area(&rect, bound) * WEIGHT_OUTSIDE;
    }
    let own_edge_dist = polyline_rect_distance(own_edge_points, &rect);
    if own_edge_dist.is_finite() {
        let (target_gap, under_weight, over_weight, hard_penalty) =
            if kind == DiagramKind::Flowchart {
                (
                    OWN_EDGE_GAP_TARGET_FLOWCHART,
                    OWN_EDGE_GAP_UNDER_WEIGHT_FLOWCHART,
                    OWN_EDGE_GAP_OVER_WEIGHT_FLOWCHART,
                    OWN_EDGE_TOUCH_HARD_PENALTY_FLOWCHART,
                )
            } else {
                (
                    OWN_EDGE_GAP_TARGET,
                    OWN_EDGE_GAP_UNDER_WEIGHT,
                    OWN_EDGE_GAP_OVER_WEIGHT,
                    OWN_EDGE_TOUCH_HARD_PENALTY,
                )
            };
        if own_edge_dist < target_gap {
            let shortage = (target_gap - own_edge_dist) / target_gap.max(1e-3);
            overlap += area * (shortage * shortage * under_weight);
        }
        if own_edge_dist > target_gap {
            let excess = (own_edge_dist - target_gap) / target_gap.max(1e-3);
            overlap += area * (excess * excess * over_weight);
        }
        if own_edge_dist <= 0.35 {
            overlap += area * hard_penalty;
        }
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
    kind: DiagramKind,
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
                kind,
                occupied,
                occupied_grid,
                node_obstacle_count,
                edge_obstacles,
                edge_grid,
                edge_idx,
                &edge.points,
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
    fn polyline_rect_distance_zero_when_segment_crosses_rect() {
        let rect: Rect = (10.0, 10.0, 20.0, 20.0);
        let points = vec![(0.0, 20.0), (40.0, 20.0)];
        let dist = polyline_rect_distance(&points, &rect);
        assert!(
            dist <= 1e-4,
            "expected intersection distance ~0, got {dist}"
        );
    }

    #[test]
    fn polyline_rect_distance_positive_when_clear() {
        let rect: Rect = (10.0, 10.0, 20.0, 20.0);
        let points = vec![(0.0, 40.0), (40.0, 40.0)];
        let dist = polyline_rect_distance(&points, &rect);
        assert!(
            (dist - 10.0).abs() < 1e-3,
            "expected 10px gap below rectangle, got {dist}"
        );
    }

    #[test]
    fn label_penalties_increase_when_touching_own_edge() {
        let rect_touch: Rect = (10.0, 10.0, 20.0, 10.0);
        let rect_clear: Rect = (10.0, 16.0, 20.0, 10.0);
        let edge_points = vec![(0.0, 15.0), (40.0, 15.0)];
        let occupied: Vec<Rect> = Vec::new();
        let occupied_grid = ObstacleGrid::new(20.0, &occupied);
        let edge_obstacles: Vec<EdgeObstacle> = Vec::new();
        let edge_rects: Vec<Rect> = Vec::new();
        let edge_grid = ObstacleGrid::new(20.0, &edge_rects);

        let touch = label_penalties(
            rect_touch,
            (20.0, 15.0),
            20.0,
            10.0,
            DiagramKind::Flowchart,
            &occupied,
            &occupied_grid,
            0,
            &edge_obstacles,
            &edge_grid,
            0,
            &edge_points,
            None,
        );
        let clear = label_penalties(
            rect_clear,
            (20.0, 15.0),
            20.0,
            10.0,
            DiagramKind::Flowchart,
            &occupied,
            &occupied_grid,
            0,
            &edge_obstacles,
            &edge_grid,
            0,
            &edge_points,
            None,
        );

        assert!(
            touch.0 > clear.0,
            "touching own edge should cost more than clear placement"
        );
    }

    #[test]
    fn label_penalties_increase_when_too_far_from_own_edge() {
        let rect_near: Rect = (10.0, 14.0, 20.0, 10.0);
        let rect_far: Rect = (10.0, 44.0, 20.0, 10.0);
        let edge_points = vec![(0.0, 15.0), (40.0, 15.0)];
        let occupied: Vec<Rect> = Vec::new();
        let occupied_grid = ObstacleGrid::new(20.0, &occupied);
        let edge_obstacles: Vec<EdgeObstacle> = Vec::new();
        let edge_rects: Vec<Rect> = Vec::new();
        let edge_grid = ObstacleGrid::new(20.0, &edge_rects);

        let near = label_penalties(
            rect_near,
            (20.0, 15.0),
            20.0,
            10.0,
            DiagramKind::Flowchart,
            &occupied,
            &occupied_grid,
            0,
            &edge_obstacles,
            &edge_grid,
            0,
            &edge_points,
            None,
        );
        let far = label_penalties(
            rect_far,
            (20.0, 15.0),
            20.0,
            10.0,
            DiagramKind::Flowchart,
            &occupied,
            &occupied_grid,
            0,
            &edge_obstacles,
            &edge_grid,
            0,
            &edge_points,
            None,
        );

        assert!(
            far.0 > near.0,
            "large own-edge gap should cost more than near-target placement"
        );
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
