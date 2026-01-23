use crate::config::LayoutConfig;
use crate::ir::{Direction, Graph};
use crate::theme::Theme;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct TextBlock {
    pub lines: Vec<String>,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct NodeLayout {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub label: TextBlock,
    pub shape: crate::ir::NodeShape,
    pub style: crate::ir::NodeStyle,
    pub anchor_subgraph: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct EdgeLayout {
    pub from: String,
    pub to: String,
    pub label: Option<TextBlock>,
    pub points: Vec<(f32, f32)>,
    pub directed: bool,
    pub arrow_start: bool,
    pub arrow_end: bool,
    pub start_decoration: Option<crate::ir::EdgeDecoration>,
    pub end_decoration: Option<crate::ir::EdgeDecoration>,
    pub style: crate::ir::EdgeStyle,
    pub override_style: crate::ir::EdgeStyleOverride,
}

#[derive(Debug, Clone)]
pub struct SubgraphLayout {
    pub label: String,
    pub label_block: TextBlock,
    pub nodes: Vec<String>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub style: crate::ir::NodeStyle,
}

#[derive(Debug, Clone)]
pub struct Lifeline {
    pub id: String,
    pub x: f32,
    pub y1: f32,
    pub y2: f32,
}

#[derive(Debug, Clone)]
pub struct SequenceLabel {
    pub x: f32,
    pub y: f32,
    pub text: TextBlock,
}

#[derive(Debug, Clone)]
pub struct SequenceFrameLayout {
    pub kind: crate::ir::SequenceFrameKind,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub label_box: (f32, f32, f32, f32),
    pub label: SequenceLabel,
    pub section_labels: Vec<SequenceLabel>,
    pub dividers: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub kind: crate::ir::DiagramKind,
    pub nodes: BTreeMap<String, NodeLayout>,
    pub edges: Vec<EdgeLayout>,
    pub subgraphs: Vec<SubgraphLayout>,
    pub lifelines: Vec<Lifeline>,
    pub sequence_footboxes: Vec<NodeLayout>,
    pub sequence_frames: Vec<SequenceFrameLayout>,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
struct Obstacle {
    id: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    members: Option<HashSet<String>>,
}

fn is_horizontal(direction: Direction) -> bool {
    matches!(direction, Direction::LeftRight | Direction::RightLeft)
}

fn is_region_subgraph(sub: &crate::ir::Subgraph) -> bool {
    sub.label.trim().is_empty()
        && sub
            .id
            .as_deref()
            .map(|id| id.starts_with("__region_"))
            .unwrap_or(false)
}

pub fn compute_layout(graph: &Graph, theme: &Theme, config: &LayoutConfig) -> Layout {
    match graph.kind {
        crate::ir::DiagramKind::Sequence => compute_sequence_layout(graph, theme, config),
        crate::ir::DiagramKind::Class
        | crate::ir::DiagramKind::State
        | crate::ir::DiagramKind::Flowchart => compute_flowchart_layout(graph, theme, config),
    }
}

fn compute_flowchart_layout(graph: &Graph, theme: &Theme, config: &LayoutConfig) -> Layout {
    let mut nodes = BTreeMap::new();

    for node in graph.nodes.values() {
        let label = measure_label(&node.label, theme, config);
        let label_empty = label.lines.len() == 1 && label.lines[0].trim().is_empty();
        let (mut width, mut height) = shape_size(node.shape, &label, config);
        if graph.kind == crate::ir::DiagramKind::State
            && label_empty
            && matches!(
                node.shape,
                crate::ir::NodeShape::Circle | crate::ir::NodeShape::DoubleCircle
            )
        {
            let size = (config.node_padding_y * 1.4).max(12.0);
            width = size;
            height = size;
        }
        let style = resolve_node_style(node.id.as_str(), graph);
        nodes.insert(
            node.id.clone(),
            NodeLayout {
                id: node.id.clone(),
                x: 0.0,
                y: 0.0,
                width,
                height,
                label,
                shape: node.shape,
                style,
                anchor_subgraph: None,
            },
        );
    }

    let ranks = compute_ranks(graph);
    let mut max_rank = 0usize;
    for rank in ranks.values() {
        max_rank = max_rank.max(*rank);
    }

    let mut rank_nodes: Vec<Vec<String>> = vec![Vec::new(); max_rank + 1];
    for (id, rank) in &ranks {
        if let Some(bucket) = rank_nodes.get_mut(*rank) {
            bucket.push(id.clone());
        }
    }

    for bucket in &mut rank_nodes {
        bucket.sort();
    }

    order_rank_nodes(&mut rank_nodes, &graph.edges);

    let mut main_cursor = 0.0;
    let mut _max_cross: f32 = 0.0;

    for (rank_idx, bucket) in rank_nodes.iter().enumerate() {
        let mut cross_cursor = 0.0;
        let mut max_main: f32 = 0.0;

        for node_id in bucket {
            if let Some(node_layout) = nodes.get_mut(node_id) {
                if is_horizontal(graph.direction) {
                    node_layout.x = main_cursor;
                    node_layout.y = cross_cursor;
                    cross_cursor += node_layout.height + config.node_spacing;
                    if node_layout.width > max_main {
                        max_main = node_layout.width;
                    }
                } else {
                    node_layout.x = cross_cursor;
                    node_layout.y = main_cursor;
                    cross_cursor += node_layout.width + config.node_spacing;
                    if node_layout.height > max_main {
                        max_main = node_layout.height;
                    }
                }
            }
        }

        main_cursor += max_main + config.rank_spacing;
        _max_cross = _max_cross.max(cross_cursor);

        if rank_idx == max_rank {
            // Ensure no trailing spacing
        }
    }

    if !graph.subgraphs.is_empty() {
        if graph.kind != crate::ir::DiagramKind::State {
            apply_subgraph_direction_overrides(graph, &mut nodes, config);
        } else {
            apply_state_subgraph_layouts(graph, &mut nodes, config);
        }
        apply_orthogonal_region_bands(graph, &mut nodes, config);
        if graph.kind != crate::ir::DiagramKind::State {
            apply_subgraph_bands(graph, &mut nodes, config);
        }
    }

    let mut subgraphs = build_subgraph_layouts(graph, &nodes, theme, config);
    apply_subgraph_anchors(graph, &subgraphs, &mut nodes);
    let obstacles = build_obstacles(&nodes, &subgraphs);
    let pair_counts = build_edge_pair_counts(&graph.edges);
    let mut pair_seen: HashMap<(String, String), usize> = HashMap::new();
    let mut edges = Vec::new();
    for (idx, edge) in graph.edges.iter().enumerate() {
        let key = edge_pair_key(edge);
        let total = *pair_counts.get(&key).unwrap_or(&1) as f32;
        let seen = pair_seen.entry(key).or_insert(0usize);
        let idx_in_pair = *seen as f32;
        *seen += 1;
        let base_offset = if total > 1.0 {
            (idx_in_pair - (total - 1.0) / 2.0) * (config.node_spacing * 0.35)
        } else {
            0.0
        };
        let from_layout = nodes.get(&edge.from).expect("from node missing");
        let to_layout = nodes.get(&edge.to).expect("to node missing");
        let temp_from = from_layout.anchor_subgraph.and_then(|idx| {
            subgraphs
                .get(idx)
                .map(|sub| anchor_layout_for_edge(from_layout, sub, graph.direction, true))
        });
        let temp_to = to_layout.anchor_subgraph.and_then(|idx| {
            subgraphs
                .get(idx)
                .map(|sub| anchor_layout_for_edge(to_layout, sub, graph.direction, false))
        });
        let from = temp_from.as_ref().unwrap_or(from_layout);
        let to = temp_to.as_ref().unwrap_or(to_layout);
        let label = edge.label.as_ref().map(|l| measure_label(l, theme, config));
        let override_style = resolve_edge_style(idx, graph);

        let route_ctx = RouteContext {
            from_id: &edge.from,
            to_id: &edge.to,
            from,
            to,
            direction: graph.direction,
            config,
            obstacles: &obstacles,
            base_offset,
        };
        let points = route_edge_with_avoidance(&route_ctx);
        edges.push(EdgeLayout {
            from: edge.from.clone(),
            to: edge.to.clone(),
            label,
            points,
            directed: edge.directed,
            arrow_start: edge.arrow_start,
            arrow_end: edge.arrow_end,
            start_decoration: edge.start_decoration,
            end_decoration: edge.end_decoration,
            style: edge.style,
            override_style,
        });
    }

    if matches!(graph.direction, Direction::RightLeft | Direction::BottomTop) {
        apply_direction_mirror(graph.direction, &mut nodes, &mut edges, &mut subgraphs);
    }

    normalize_layout(&mut nodes, &mut edges, &mut subgraphs);
    let (width, height) = bounds_from_layout(&nodes, &subgraphs);

    Layout {
        kind: graph.kind,
        nodes,
        edges,
        subgraphs,
        lifelines: Vec::new(),
        sequence_footboxes: Vec::new(),
        sequence_frames: Vec::new(),
        width,
        height,
    }
}

fn compute_sequence_layout(graph: &Graph, theme: &Theme, config: &LayoutConfig) -> Layout {
    let mut nodes = BTreeMap::new();
    let mut edges = Vec::new();
    let subgraphs = Vec::new();

    let mut participants = graph.sequence_participants.clone();
    for id in graph.nodes.keys() {
        if !participants.contains(id) {
            participants.push(id.clone());
        }
    }

    let mut label_blocks: HashMap<String, TextBlock> = HashMap::new();
    let mut max_label_width: f32 = 0.0;
    let mut max_label_height: f32 = 0.0;
    for id in &participants {
        let node = graph.nodes.get(id).expect("participant missing");
        let label = measure_label(&node.label, theme, config);
        max_label_width = max_label_width.max(label.width);
        max_label_height = max_label_height.max(label.height);
        label_blocks.insert(id.clone(), label);
    }

    let actor_width = (max_label_width + theme.font_size * 2.5).max(150.0);
    let actor_height = (max_label_height + theme.font_size * 2.5).max(65.0);
    let actor_gap = (theme.font_size * 3.1).max(40.0);

    let mut cursor_x = 0.0;
    for id in &participants {
        let label = label_blocks
            .get(id)
            .cloned()
            .unwrap_or_else(|| TextBlock {
                lines: vec![id.clone()],
                width: 0.0,
                height: 0.0,
            });
        nodes.insert(
            id.clone(),
            NodeLayout {
                id: id.clone(),
                x: cursor_x,
                y: 0.0,
                width: actor_width,
                height: actor_height,
                label,
                shape: crate::ir::NodeShape::RoundRect,
                style: resolve_node_style(id.as_str(), graph),
                anchor_subgraph: None,
            },
        );
        cursor_x += actor_width + actor_gap;
    }

    let base_spacing = (theme.font_size * 2.8).max(24.0);
    let mut extra_before = vec![0.0; graph.edges.len()];
    for frame in &graph.sequence_frames {
        if frame.start_idx < extra_before.len() {
            extra_before[frame.start_idx] += base_spacing;
        }
        for section in frame.sections.iter().skip(1) {
            if section.start_idx < extra_before.len() {
                extra_before[section.start_idx] += base_spacing;
            }
        }
    }

    let mut message_cursor = actor_height + theme.font_size * 2.9;
    let mut message_ys = Vec::new();
    for idx in 0..graph.edges.len() {
        message_cursor += extra_before[idx];
        message_ys.push(message_cursor);
        message_cursor += base_spacing;
    }

    for (idx, edge) in graph.edges.iter().enumerate() {
        let from = nodes.get(&edge.from).expect("from node missing");
        let to = nodes.get(&edge.to).expect("to node missing");
        let y = message_ys.get(idx).copied().unwrap_or(message_cursor);
        let label = edge.label.as_ref().map(|l| measure_label(l, theme, config));

        let points = if edge.from == edge.to {
            let pad = config.node_spacing.max(20.0) * 0.6;
            let x = from.x + from.width / 2.0;
            vec![(x, y), (x + pad, y), (x + pad, y + pad), (x, y + pad)]
        } else {
            let from_x = from.x + from.width / 2.0;
            let to_x = to.x + to.width / 2.0;
            vec![(from_x, y), (to_x, y)]
        };

        let mut override_style = resolve_edge_style(idx, graph);
        if edge.style == crate::ir::EdgeStyle::Dotted {
            if override_style.dasharray.is_none() {
                override_style.dasharray = Some("3 3".to_string());
            }
            if override_style.stroke_width.is_none() {
                override_style.stroke_width = Some(1.4);
            }
        }
        edges.push(EdgeLayout {
            from: edge.from.clone(),
            to: edge.to.clone(),
            label,
            points,
            directed: edge.directed,
            arrow_start: edge.arrow_start,
            arrow_end: edge.arrow_end,
            start_decoration: edge.start_decoration,
            end_decoration: edge.end_decoration,
            style: edge.style,
            override_style,
        });
    }

    let mut sequence_frames = Vec::new();
    if !graph.sequence_frames.is_empty() && !message_ys.is_empty() {
        let mut frames = graph.sequence_frames.clone();
        frames.sort_by(|a, b| {
            a.start_idx
                .cmp(&b.start_idx)
                .then_with(|| b.end_idx.cmp(&a.end_idx))
        });
        for frame in frames {
            if frame.start_idx >= frame.end_idx || frame.start_idx >= message_ys.len() {
                continue;
            }
            let mut xs = Vec::new();
            for edge in graph
                .edges
                .iter()
                .skip(frame.start_idx)
                .take(frame.end_idx.saturating_sub(frame.start_idx))
            {
                if let Some(node) = nodes.get(&edge.from) {
                    xs.push(node.x + node.width / 2.0);
                }
                if let Some(node) = nodes.get(&edge.to) {
                    xs.push(node.x + node.width / 2.0);
                }
            }
            if xs.is_empty() {
                for node in nodes.values() {
                    xs.push(node.x + node.width / 2.0);
                }
            }
            let (min_x, max_x) = xs.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |acc, x| {
                (acc.0.min(*x), acc.1.max(*x))
            });
            if !min_x.is_finite() || !max_x.is_finite() {
                continue;
            }
            let frame_pad_x = theme.font_size * 0.7;
            let frame_x = min_x - frame_pad_x;
            let frame_width = (max_x - min_x) + frame_pad_x * 2.0;

            let first_y = message_ys
                .get(frame.start_idx)
                .copied()
                .unwrap_or(message_cursor);
            let last_y = message_ys
                .get(frame.end_idx.saturating_sub(1))
                .copied()
                .unwrap_or(first_y);
            let header_offset = theme.font_size * 0.6;
            let top_offset = (2.0 * base_spacing - header_offset).max(base_spacing);
            let bottom_offset = header_offset;
            let frame_y = first_y - top_offset;
            let frame_height = (last_y - first_y).max(0.0) + top_offset + bottom_offset;

            let frame_label_text = match frame.kind {
                crate::ir::SequenceFrameKind::Alt => "alt",
                crate::ir::SequenceFrameKind::Opt => "opt",
                crate::ir::SequenceFrameKind::Loop => "loop",
                crate::ir::SequenceFrameKind::Par => "par",
                crate::ir::SequenceFrameKind::Rect => "rect",
            };
            let label_block = measure_label(frame_label_text, theme, config);
            let label_box_w = (label_block.width + theme.font_size * 2.0).max(theme.font_size * 3.0);
            let label_box_h = theme.font_size * 1.25;
            let label_box_x = frame_x;
            let label_box_y = frame_y;
            let label = SequenceLabel {
                x: label_box_x + label_box_w / 2.0,
                y: label_box_y + label_box_h / 2.0,
                text: label_block,
            };

            let mut dividers = Vec::new();
            let divider_offset = theme.font_size * 0.9;
            for window in frame.sections.windows(2) {
                let prev_end = window[0].end_idx;
                let base_y = message_ys
                    .get(prev_end.saturating_sub(1))
                    .copied()
                    .unwrap_or(first_y);
                dividers.push(base_y + divider_offset);
            }

            let mut section_labels = Vec::new();
            let label_offset = theme.font_size * 1.1;
            for (section_idx, section) in frame.sections.iter().enumerate() {
                if let Some(label) = &section.label {
                    let display = format!("[{}]", label);
                    let block = measure_label(&display, theme, config);
                    let label_y = if section_idx == 0 {
                        frame_y + label_offset
                    } else {
                        dividers
                            .get(section_idx - 1)
                            .copied()
                            .unwrap_or(frame_y + label_offset)
                            + label_offset
                    };
                    section_labels.push(SequenceLabel {
                        x: frame_x + frame_width / 2.0,
                        y: label_y,
                        text: block,
                    });
                }
            }

            sequence_frames.push(SequenceFrameLayout {
                kind: frame.kind,
                x: frame_x,
                y: frame_y,
                width: frame_width,
                height: frame_height,
                label_box: (label_box_x, label_box_y, label_box_w, label_box_h),
                label,
                section_labels,
                dividers,
            });
        }
    }

    let lifeline_start = actor_height;
    let last_message_y = message_ys
        .last()
        .copied()
        .unwrap_or(lifeline_start + base_spacing);
    let footbox_gap = (theme.font_size * 1.25).max(16.0);
    let lifeline_end = last_message_y + footbox_gap;
    let lifelines = participants
        .iter()
        .filter_map(|id| nodes.get(id))
        .map(|node| Lifeline {
            id: node.id.clone(),
            x: node.x + node.width / 2.0,
            y1: lifeline_start,
            y2: lifeline_end,
        })
        .collect::<Vec<_>>();

    let sequence_footboxes = participants
        .iter()
        .filter_map(|id| nodes.get(id))
        .map(|node| {
            let mut foot = node.clone();
            foot.y = lifeline_end;
            foot
        })
        .collect::<Vec<_>>();

    let (mut width, mut height) = bounds_from_layout(&nodes, &subgraphs);
    width = width.max(cursor_x + 40.0);
    let footbox_height = sequence_footboxes
        .iter()
        .map(|node| node.height)
        .fold(0.0, f32::max);
    height = height.max(lifeline_end + footbox_height + 40.0);

    Layout {
        kind: graph.kind,
        nodes,
        edges,
        subgraphs,
        lifelines,
        sequence_footboxes,
        sequence_frames,
        width,
        height,
    }
}

fn resolve_edge_style(idx: usize, graph: &Graph) -> crate::ir::EdgeStyleOverride {
    let mut style = graph.edge_style_default.clone().unwrap_or_default();
    if let Some(edge_style) = graph.edge_styles.get(&idx) {
        merge_edge_style(&mut style, edge_style);
    }
    style
}

fn merge_edge_style(
    target: &mut crate::ir::EdgeStyleOverride,
    source: &crate::ir::EdgeStyleOverride,
) {
    if source.stroke.is_some() {
        target.stroke = source.stroke.clone();
    }
    if source.stroke_width.is_some() {
        target.stroke_width = source.stroke_width;
    }
    if source.dasharray.is_some() {
        target.dasharray = source.dasharray.clone();
    }
    if source.label_color.is_some() {
        target.label_color = source.label_color.clone();
    }
}

fn order_rank_nodes(rank_nodes: &mut [Vec<String>], edges: &[crate::ir::Edge]) {
    if rank_nodes.len() <= 1 {
        return;
    }
    let mut incoming: HashMap<String, Vec<String>> = HashMap::new();
    let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();

    for edge in edges {
        outgoing
            .entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
        incoming
            .entry(edge.to.clone())
            .or_default()
            .push(edge.from.clone());
    }

    let mut positions: HashMap<String, usize> = HashMap::new();
    let update_positions = |rank_nodes: &mut [Vec<String>],
                            positions: &mut HashMap<String, usize>| {
        positions.clear();
        for bucket in rank_nodes.iter() {
            for (idx, node_id) in bucket.iter().enumerate() {
                positions.insert(node_id.clone(), idx);
            }
        }
    };

    update_positions(rank_nodes, &mut positions);

    let sort_bucket = |bucket: &mut Vec<String>,
                       neighbors: &HashMap<String, Vec<String>>,
                       positions: &HashMap<String, usize>| {
        let current_positions: HashMap<String, usize> = bucket
            .iter()
            .enumerate()
            .map(|(idx, id)| (id.clone(), idx))
            .collect();
        bucket.sort_by(|a, b| {
            let a_score = barycenter(a, neighbors, positions, &current_positions);
            let b_score = barycenter(b, neighbors, positions, &current_positions);
            match a_score.partial_cmp(&b_score) {
                Some(std::cmp::Ordering::Equal) | None => {
                    let a_pos = current_positions.get(a).copied().unwrap_or(0);
                    let b_pos = current_positions.get(b).copied().unwrap_or(0);
                    a_pos.cmp(&b_pos)
                }
                Some(ordering) => ordering,
            }
        });
    };

    for _ in 0..2 {
        for rank in 1..rank_nodes.len() {
            if rank_nodes[rank].len() <= 1 {
                continue;
            }
            sort_bucket(&mut rank_nodes[rank], &incoming, &positions);
            update_positions(rank_nodes, &mut positions);
        }
        for rank in (0..rank_nodes.len().saturating_sub(1)).rev() {
            if rank_nodes[rank].len() <= 1 {
                continue;
            }
            sort_bucket(&mut rank_nodes[rank], &outgoing, &positions);
            update_positions(rank_nodes, &mut positions);
        }
    }
}

fn barycenter(
    node_id: &str,
    neighbors: &HashMap<String, Vec<String>>,
    positions: &HashMap<String, usize>,
    current_positions: &HashMap<String, usize>,
) -> f32 {
    let Some(list) = neighbors.get(node_id) else {
        return *current_positions.get(node_id).unwrap_or(&0) as f32;
    };
    let mut total = 0.0;
    let mut count = 0.0;
    for neighbor in list {
        if let Some(pos) = positions.get(neighbor) {
            total += *pos as f32;
            count += 1.0;
        }
    }
    if count == 0.0 {
        *current_positions.get(node_id).unwrap_or(&0) as f32
    } else {
        total / count
    }
}

fn compute_ranks(graph: &Graph) -> HashMap<String, usize> {
    let mut indeg: HashMap<String, usize> = HashMap::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();

    for id in graph.nodes.keys() {
        indeg.insert(id.clone(), 0);
    }

    for edge in &graph.edges {
        adj.entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
        *indeg.entry(edge.to.clone()).or_insert(0) += 1;
    }

    let mut queue: VecDeque<String> = indeg
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut order = Vec::new();
    while let Some(node) = queue.pop_front() {
        order.push(node.clone());
        if let Some(nexts) = adj.get(&node) {
            for next in nexts {
                if let Some(deg) = indeg.get_mut(next) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(next.clone());
                    }
                }
            }
        }
    }

    if order.len() < graph.nodes.len() {
        let mut seen: HashSet<String> = order.iter().cloned().collect();
        for id in graph.nodes.keys() {
            if !seen.contains(id) {
                order.push(id.clone());
                seen.insert(id.clone());
            }
        }
    }

    let order_index: HashMap<String, usize> = order
        .iter()
        .enumerate()
        .map(|(idx, id)| (id.clone(), idx))
        .collect();

    let mut ranks: HashMap<String, usize> = HashMap::new();
    for node in &order {
        let rank = *ranks.get(node).unwrap_or(&0);
        ranks.entry(node.clone()).or_insert(rank);
        if let Some(nexts) = adj.get(node) {
            let from_idx = *order_index.get(node).unwrap_or(&0);
            for next in nexts {
                let to_idx = *order_index.get(next).unwrap_or(&from_idx);
                if to_idx <= from_idx {
                    continue;
                }
                let entry = ranks.entry(next.clone()).or_insert(0);
                *entry = (*entry).max(rank + 1);
            }
        }
    }

    ranks
}

fn apply_subgraph_bands(
    graph: &Graph,
    nodes: &mut BTreeMap<String, NodeLayout>,
    config: &LayoutConfig,
) {
    let mut group_nodes: Vec<Vec<String>> = Vec::new();
    let mut node_group: HashMap<String, usize> = HashMap::new();

    // Group 0: nodes not in any subgraph.
    group_nodes.push(Vec::new());
    for node_id in graph.nodes.keys() {
        node_group.insert(node_id.clone(), 0);
    }

    let top_level = top_level_subgraph_indices(graph);
    for (pos, idx) in top_level.iter().enumerate() {
        let group_idx = pos + 1;
        let sub = &graph.subgraphs[*idx];
        group_nodes.push(Vec::new());
        for node_id in &sub.nodes {
            if nodes.contains_key(node_id) {
                node_group.insert(node_id.clone(), group_idx);
            }
        }
    }

    for (node_id, group_idx) in &node_group {
        if let Some(bucket) = group_nodes.get_mut(*group_idx) {
            bucket.push(node_id.clone());
        }
    }

    let mut groups: Vec<(usize, f32, f32, f32, f32)> = Vec::new();
    for (idx, bucket) in group_nodes.iter().enumerate() {
        if bucket.is_empty() {
            continue;
        }
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for node_id in bucket {
            if let Some(node) = nodes.get(node_id) {
                min_x = min_x.min(node.x);
                min_y = min_y.min(node.y);
                max_x = max_x.max(node.x + node.width);
                max_y = max_y.max(node.y + node.height);
            }
        }
        if min_x != f32::MAX {
            groups.push((idx, min_x, min_y, max_x, max_y));
        }
    }

    // Order groups by their current position to minimize crossing shifts.
    // Keep the non-subgraph group first to bias subgraphs after the main flow.
    if is_horizontal(graph.direction) {
        groups.sort_by(|a, b| {
            let a_primary = if a.0 == 0 { 0 } else { 1 };
            let b_primary = if b.0 == 0 { 0 } else { 1 };
            a_primary
                .cmp(&b_primary)
                .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        });
    } else {
        groups.sort_by(|a, b| {
            let a_primary = if a.0 == 0 { 0 } else { 1 };
            let b_primary = if b.0 == 0 { 0 } else { 1 };
            a_primary
                .cmp(&b_primary)
                .then_with(|| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
        });
    }

    let spacing = config.rank_spacing * 0.8;
    if is_horizontal(graph.direction) {
        let mut cursor = 0.0;
        for (group_idx, min_x, _min_y, max_x, _max_y) in groups {
            let width = max_x - min_x;
            let offset = cursor - min_x;
            for node_id in group_nodes[group_idx].iter() {
                if let Some(node) = nodes.get_mut(node_id) {
                    node.x += offset;
                }
            }
            cursor += width + spacing;
        }
    } else {
        let mut cursor = 0.0;
        for (group_idx, _min_x, min_y, _max_x, max_y) in groups {
            let height = max_y - min_y;
            let offset = cursor - min_y;
            for node_id in group_nodes[group_idx].iter() {
                if let Some(node) = nodes.get_mut(node_id) {
                    node.y += offset;
                }
            }
            cursor += height + spacing;
        }
    }
}

fn apply_orthogonal_region_bands(
    graph: &Graph,
    nodes: &mut BTreeMap<String, NodeLayout>,
    config: &LayoutConfig,
) {
    let mut region_indices = Vec::new();
    for (idx, sub) in graph.subgraphs.iter().enumerate() {
        if is_region_subgraph(sub) {
            region_indices.push(idx);
        }
    }
    if region_indices.is_empty() {
        return;
    }

    let sets: Vec<HashSet<String>> = graph
        .subgraphs
        .iter()
        .map(|sub| sub.nodes.iter().cloned().collect())
        .collect();

    let mut parent_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for region_idx in region_indices {
        let region_set = &sets[region_idx];
        let mut parent: Option<usize> = None;
        for (idx, set) in sets.iter().enumerate() {
            if idx == region_idx {
                continue;
            }
            if set.len() <= region_set.len() {
                continue;
            }
            if !region_set.is_subset(set) {
                continue;
            }
            if is_region_subgraph(&graph.subgraphs[idx]) {
                continue;
            }
            match parent {
                None => parent = Some(idx),
                Some(current) => {
                    if set.len() < sets[current].len() {
                        parent = Some(idx);
                    }
                }
            }
        }
        if let Some(parent_idx) = parent {
            parent_map.entry(parent_idx).or_default().push(region_idx);
        }
    }

    let spacing = config.rank_spacing * 0.6;
    let stack_along_x = is_horizontal(graph.direction);

    for region_list in parent_map.values() {
        let mut region_boxes: Vec<(usize, f32, f32, f32, f32)> = Vec::new();
        for &region_idx in region_list {
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX;
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN;
            for node_id in &graph.subgraphs[region_idx].nodes {
                if let Some(node) = nodes.get(node_id) {
                    min_x = min_x.min(node.x);
                    min_y = min_y.min(node.y);
                    max_x = max_x.max(node.x + node.width);
                    max_y = max_y.max(node.y + node.height);
                }
            }
            if min_x != f32::MAX {
                region_boxes.push((region_idx, min_x, min_y, max_x, max_y));
            }
        }
        if region_boxes.len() <= 1 {
            continue;
        }

        if stack_along_x {
            region_boxes.sort_by(|a, b| {
                a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
            });
            let mut cursor = region_boxes
                .first()
                .map(|entry| entry.1)
                .unwrap_or(0.0);
            for (region_idx, min_x, _min_y, max_x, _max_y) in region_boxes {
                let offset = cursor - min_x;
                for node_id in &graph.subgraphs[region_idx].nodes {
                    if let Some(node) = nodes.get_mut(node_id) {
                        node.x += offset;
                    }
                }
                cursor += (max_x - min_x) + spacing;
            }
        } else {
            region_boxes.sort_by(|a, b| {
                a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal)
            });
            let mut cursor = region_boxes
                .first()
                .map(|entry| entry.2)
                .unwrap_or(0.0);
            for (region_idx, _min_x, min_y, _max_x, max_y) in region_boxes {
                let offset = cursor - min_y;
                for node_id in &graph.subgraphs[region_idx].nodes {
                    if let Some(node) = nodes.get_mut(node_id) {
                        node.y += offset;
                    }
                }
                cursor += (max_y - min_y) + spacing;
            }
        }
    }
}

fn top_level_subgraph_indices(graph: &Graph) -> Vec<usize> {
    let mut sets: Vec<HashSet<String>> = Vec::new();
    for sub in &graph.subgraphs {
        sets.push(sub.nodes.iter().cloned().collect());
    }

    let mut top_level = Vec::new();
    for i in 0..graph.subgraphs.len() {
        let mut nested = false;
        for j in 0..graph.subgraphs.len() {
            if i == j {
                continue;
            }
            if sets[j].len() > sets[i].len() && sets[i].is_subset(&sets[j]) {
                nested = true;
                break;
            }
        }
        if !nested {
            top_level.push(i);
        }
    }
    top_level
}

fn apply_subgraph_direction_overrides(
    graph: &Graph,
    nodes: &mut BTreeMap<String, NodeLayout>,
    config: &LayoutConfig,
) {
    for sub in &graph.subgraphs {
        if is_region_subgraph(sub) {
            continue;
        }
        let direction = match sub.direction {
            Some(direction) => direction,
            None => {
                if sub.nodes.len() <= 1 {
                    continue;
                }
                match graph.direction {
                    Direction::TopDown | Direction::BottomTop => Direction::LeftRight,
                    Direction::LeftRight | Direction::RightLeft => Direction::TopDown,
                }
            }
        };
        if sub.nodes.is_empty() {
            continue;
        }
        if direction == graph.direction {
            continue;
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        for node_id in &sub.nodes {
            if let Some(node) = nodes.get(node_id) {
                min_x = min_x.min(node.x);
                min_y = min_y.min(node.y);
            }
        }
        if min_x == f32::MAX {
            continue;
        }

        let ranks = compute_ranks_subset(&sub.nodes, &graph.edges);
        assign_positions(&sub.nodes, &ranks, direction, config, nodes, min_x, min_y);

        if matches!(direction, Direction::RightLeft | Direction::BottomTop) {
            mirror_subgraph_nodes(&sub.nodes, nodes, direction);
        }
    }
}

fn apply_state_subgraph_layouts(
    graph: &Graph,
    nodes: &mut BTreeMap<String, NodeLayout>,
    config: &LayoutConfig,
) {
    for sub in &graph.subgraphs {
        if sub.nodes.len() <= 1 {
            continue;
        }
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        for node_id in &sub.nodes {
            if let Some(node) = nodes.get(node_id) {
                min_x = min_x.min(node.x);
                min_y = min_y.min(node.y);
            }
        }
        if min_x == f32::MAX {
            continue;
        }
        let ranks = compute_ranks_subset(&sub.nodes, &graph.edges);
        assign_positions(&sub.nodes, &ranks, graph.direction, config, nodes, min_x, min_y);
    }
}

fn apply_subgraph_anchors(
    graph: &Graph,
    subgraphs: &[SubgraphLayout],
    nodes: &mut BTreeMap<String, NodeLayout>,
) {
    if subgraphs.is_empty() {
        return;
    }

    let mut label_to_index: HashMap<&str, usize> = HashMap::new();
    for (idx, sub) in subgraphs.iter().enumerate() {
        label_to_index.insert(sub.label.as_str(), idx);
    }

    for sub in &graph.subgraphs {
        let Some(&layout_idx) = label_to_index.get(sub.label.as_str()) else {
            continue;
        };
        let layout = &subgraphs[layout_idx];
        let mut anchor_ids: HashSet<&str> = HashSet::new();
        if let Some(id) = &sub.id {
            anchor_ids.insert(id.as_str());
        }
        anchor_ids.insert(sub.label.as_str());

        for anchor_id in anchor_ids {
            if sub.nodes.iter().any(|node_id| node_id == anchor_id) {
                continue;
            }
            let Some(node) = nodes.get_mut(anchor_id) else {
                continue;
            };
            node.anchor_subgraph = Some(layout_idx);
            let size = 2.0;
            node.width = size;
            node.height = size;
            node.x = layout.x + layout.width / 2.0 - size / 2.0;
            node.y = layout.y + layout.height / 2.0 - size / 2.0;
        }
    }
}

fn anchor_layout_for_edge(
    anchor: &NodeLayout,
    subgraph: &SubgraphLayout,
    direction: Direction,
    is_from: bool,
) -> NodeLayout {
    let size = 2.0;
    let mut node = anchor.clone();
    node.width = size;
    node.height = size;

    if is_horizontal(direction) {
        let x = if is_from {
            subgraph.x + subgraph.width - size
        } else {
            subgraph.x
        };
        let y = subgraph.y + subgraph.height / 2.0 - size / 2.0;
        node.x = x;
        node.y = y;
    } else {
        let x = subgraph.x + subgraph.width / 2.0 - size / 2.0;
        let y = if is_from {
            subgraph.y + subgraph.height - size
        } else {
            subgraph.y
        };
        node.x = x;
        node.y = y;
    }

    node
}

fn mirror_subgraph_nodes(
    node_ids: &[String],
    nodes: &mut BTreeMap<String, NodeLayout>,
    direction: Direction,
) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for node_id in node_ids {
        if let Some(node) = nodes.get(node_id) {
            min_x = min_x.min(node.x);
            min_y = min_y.min(node.y);
            max_x = max_x.max(node.x + node.width);
            max_y = max_y.max(node.y + node.height);
        }
    }

    if min_x == f32::MAX {
        return;
    }

    if matches!(direction, Direction::RightLeft) {
        for node_id in node_ids {
            if let Some(node) = nodes.get_mut(node_id) {
                node.x = min_x + (max_x - (node.x + node.width));
            }
        }
    }
    if matches!(direction, Direction::BottomTop) {
        for node_id in node_ids {
            if let Some(node) = nodes.get_mut(node_id) {
                node.y = min_y + (max_y - (node.y + node.height));
            }
        }
    }
}

fn compute_ranks_subset(node_ids: &[String], edges: &[crate::ir::Edge]) -> HashMap<String, usize> {
    let mut indeg: HashMap<String, usize> = HashMap::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let set: HashSet<String> = node_ids.iter().cloned().collect();

    for id in &set {
        indeg.insert(id.clone(), 0);
    }

    for edge in edges {
        if set.contains(&edge.from) && set.contains(&edge.to) {
            adj.entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
            *indeg.entry(edge.to.clone()).or_insert(0) += 1;
        }
    }

    let mut queue: VecDeque<String> = indeg
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut order = Vec::new();
    while let Some(node) = queue.pop_front() {
        order.push(node.clone());
        if let Some(nexts) = adj.get(&node) {
            for next in nexts {
                if let Some(deg) = indeg.get_mut(next) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(next.clone());
                    }
                }
            }
        }
    }

    if order.len() < set.len() {
        for id in &set {
            if !order.contains(id) {
                order.push(id.clone());
            }
        }
    }

    let order_index: HashMap<String, usize> = order
        .iter()
        .enumerate()
        .map(|(idx, id)| (id.clone(), idx))
        .collect();

    let mut ranks: HashMap<String, usize> = HashMap::new();
    for node in &order {
        let rank = *ranks.get(node).unwrap_or(&0);
        ranks.entry(node.clone()).or_insert(rank);
        if let Some(nexts) = adj.get(node) {
            let from_idx = *order_index.get(node).unwrap_or(&0);
            for next in nexts {
                let to_idx = *order_index.get(next).unwrap_or(&from_idx);
                if to_idx <= from_idx {
                    continue;
                }
                let entry = ranks.entry(next.clone()).or_insert(0);
                *entry = (*entry).max(rank + 1);
            }
        }
    }

    ranks
}

fn assign_positions(
    node_ids: &[String],
    ranks: &HashMap<String, usize>,
    direction: Direction,
    config: &LayoutConfig,
    nodes: &mut BTreeMap<String, NodeLayout>,
    origin_x: f32,
    origin_y: f32,
) {
    let mut max_rank = 0usize;
    for rank in ranks.values() {
        max_rank = max_rank.max(*rank);
    }

    let mut rank_nodes: Vec<Vec<String>> = vec![Vec::new(); max_rank + 1];
    for node_id in node_ids {
        let rank = *ranks.get(node_id).unwrap_or(&0);
        if let Some(bucket) = rank_nodes.get_mut(rank) {
            bucket.push(node_id.clone());
        }
    }
    for bucket in &mut rank_nodes {
        bucket.sort();
    }

    let mut main_cursor = 0.0;
    for bucket in rank_nodes {
        let mut cross_cursor = 0.0;
        let mut max_main: f32 = 0.0;
        for node_id in bucket {
            if let Some(node) = nodes.get_mut(&node_id) {
                if is_horizontal(direction) {
                    node.x = origin_x + main_cursor;
                    node.y = origin_y + cross_cursor;
                    cross_cursor += node.height + config.node_spacing;
                    max_main = max_main.max(node.width);
                } else {
                    node.x = origin_x + cross_cursor;
                    node.y = origin_y + main_cursor;
                    cross_cursor += node.width + config.node_spacing;
                    max_main = max_main.max(node.height);
                }
            }
        }
        main_cursor += max_main + config.rank_spacing;
    }
}

fn bounds_from_layout(
    nodes: &BTreeMap<String, NodeLayout>,
    subgraphs: &[SubgraphLayout],
) -> (f32, f32) {
    let (max_x, max_y) = bounds_without_padding(nodes, subgraphs);
    (max_x + 60.0, max_y + 60.0)
}

fn bounds_without_padding(
    nodes: &BTreeMap<String, NodeLayout>,
    subgraphs: &[SubgraphLayout],
) -> (f32, f32) {
    let mut max_x: f32 = 0.0;
    let mut max_y: f32 = 0.0;
    for node in nodes.values() {
        max_x = max_x.max(node.x + node.width);
        max_y = max_y.max(node.y + node.height);
    }
    for sub in subgraphs {
        let invisible_region = sub.label.trim().is_empty()
            && sub.style.stroke.as_deref() == Some("none")
            && sub.style.fill.as_deref() == Some("none");
        if invisible_region {
            continue;
        }
        max_x = max_x.max(sub.x + sub.width);
        max_y = max_y.max(sub.y + sub.height);
    }
    (max_x, max_y)
}

fn apply_direction_mirror(
    direction: Direction,
    nodes: &mut BTreeMap<String, NodeLayout>,
    edges: &mut [EdgeLayout],
    subgraphs: &mut [SubgraphLayout],
) {
    let (max_x, max_y) = bounds_without_padding(nodes, subgraphs);
    if matches!(direction, Direction::RightLeft) {
        for node in nodes.values_mut() {
            node.x = max_x - node.x - node.width;
        }
        for edge in edges.iter_mut() {
            for point in edge.points.iter_mut() {
                point.0 = max_x - point.0;
            }
        }
        for sub in subgraphs.iter_mut() {
            sub.x = max_x - sub.x - sub.width;
        }
    }
    if matches!(direction, Direction::BottomTop) {
        for node in nodes.values_mut() {
            node.y = max_y - node.y - node.height;
        }
        for edge in edges.iter_mut() {
            for point in edge.points.iter_mut() {
                point.1 = max_y - point.1;
            }
        }
        for sub in subgraphs.iter_mut() {
            sub.y = max_y - sub.y - sub.height;
        }
    }
}

fn normalize_layout(
    nodes: &mut BTreeMap<String, NodeLayout>,
    edges: &mut [EdgeLayout],
    subgraphs: &mut [SubgraphLayout],
) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    for node in nodes.values() {
        min_x = min_x.min(node.x);
        min_y = min_y.min(node.y);
    }
    for sub in subgraphs.iter() {
        min_x = min_x.min(sub.x);
        min_y = min_y.min(sub.y);
    }

    let padding = 24.0;
    let shift_x = if min_x < padding {
        padding - min_x
    } else {
        0.0
    };
    let shift_y = if min_y < padding {
        padding - min_y
    } else {
        0.0
    };

    if shift_x == 0.0 && shift_y == 0.0 {
        return;
    }

    for node in nodes.values_mut() {
        node.x += shift_x;
        node.y += shift_y;
    }
    for edge in edges.iter_mut() {
        for point in edge.points.iter_mut() {
            point.0 += shift_x;
            point.1 += shift_y;
        }
    }
    for sub in subgraphs.iter_mut() {
        sub.x += shift_x;
        sub.y += shift_y;
    }
}

struct RouteContext<'a> {
    from_id: &'a str,
    to_id: &'a str,
    from: &'a NodeLayout,
    to: &'a NodeLayout,
    direction: Direction,
    config: &'a LayoutConfig,
    obstacles: &'a [Obstacle],
    base_offset: f32,
}

fn route_edge_with_avoidance(ctx: &RouteContext<'_>) -> Vec<(f32, f32)> {
    if ctx.from_id == ctx.to_id {
        return route_self_loop(ctx.from, ctx.direction, ctx.config);
    }

    let (start, end) = if is_horizontal(ctx.direction) {
        (
            (
                ctx.from.x + ctx.from.width,
                ctx.from.y + ctx.from.height / 2.0,
            ),
            (ctx.to.x, ctx.to.y + ctx.to.height / 2.0),
        )
    } else {
        (
            (
                ctx.from.x + ctx.from.width / 2.0,
                ctx.from.y + ctx.from.height,
            ),
            (ctx.to.x + ctx.to.width / 2.0, ctx.to.y),
        )
    };

    let step = ctx.config.node_spacing.max(16.0) * 0.6;
    let mut offsets = vec![ctx.base_offset];
    for i in 1..=4 {
        let delta = step * i as f32;
        offsets.push(ctx.base_offset + delta);
        offsets.push(ctx.base_offset - delta);
    }

    for offset in offsets {
        let points = if is_horizontal(ctx.direction) {
            let mid_x = (start.0 + end.0) / 2.0 + offset;
            vec![start, (mid_x, start.1), (mid_x, end.1), end]
        } else {
            let mid_y = (start.1 + end.1) / 2.0 + offset;
            vec![start, (start.0, mid_y), (end.0, mid_y), end]
        };

        if !path_intersects_obstacles(&points, ctx.obstacles, ctx.from_id, ctx.to_id) {
            return points;
        }
    }

    if is_horizontal(ctx.direction) {
        let mid_x = (start.0 + end.0) / 2.0;
        vec![start, (mid_x, start.1), (mid_x, end.1), end]
    } else {
        let mid_y = (start.1 + end.1) / 2.0;
        vec![start, (start.0, mid_y), (end.0, mid_y), end]
    }
}

fn route_self_loop(
    node: &NodeLayout,
    direction: Direction,
    config: &LayoutConfig,
) -> Vec<(f32, f32)> {
    let pad = config.node_spacing.max(20.0) * 0.6;
    if is_horizontal(direction) {
        let start = (node.x + node.width, node.y + node.height / 2.0);
        let p1 = (node.x + node.width + pad, node.y + node.height / 2.0);
        let p2 = (node.x + node.width + pad, node.y - pad);
        let p3 = (node.x + node.width / 2.0, node.y - pad);
        let end = (node.x + node.width / 2.0, node.y);
        vec![start, p1, p2, p3, end]
    } else {
        let start = (node.x + node.width / 2.0, node.y + node.height);
        let p1 = (node.x + node.width / 2.0, node.y + node.height + pad);
        let p2 = (node.x + node.width + pad, node.y + node.height + pad);
        let p3 = (node.x + node.width + pad, node.y + node.height / 2.0);
        let end = (node.x + node.width, node.y + node.height / 2.0);
        vec![start, p1, p2, p3, end]
    }
}

fn build_obstacles(
    nodes: &BTreeMap<String, NodeLayout>,
    subgraphs: &[SubgraphLayout],
) -> Vec<Obstacle> {
    let mut obstacles = Vec::new();
    for node in nodes.values() {
        if node.anchor_subgraph.is_some() {
            continue;
        }
        obstacles.push(Obstacle {
            id: node.id.clone(),
            x: node.x - 6.0,
            y: node.y - 6.0,
            width: node.width + 12.0,
            height: node.height + 12.0,
            members: None,
        });
    }

    for (idx, sub) in subgraphs.iter().enumerate() {
        let invisible_region = sub.label.trim().is_empty()
            && sub.style.stroke.as_deref() == Some("none")
            && sub.style.fill.as_deref() == Some("none");
        if invisible_region {
            continue;
        }
        let mut members: HashSet<String> = sub.nodes.iter().cloned().collect();
        for node in nodes.values() {
            if node.anchor_subgraph == Some(idx) {
                members.insert(node.id.clone());
            }
        }
        let pad = 6.0;
        obstacles.push(Obstacle {
            id: format!("subgraph:{}", sub.label),
            x: sub.x - pad,
            y: sub.y - pad,
            width: sub.width + pad * 2.0,
            height: sub.height + pad * 2.0,
            members: Some(members),
        });
    }
    obstacles
}

fn edge_pair_key(edge: &crate::ir::Edge) -> (String, String) {
    if edge.from <= edge.to {
        (edge.from.clone(), edge.to.clone())
    } else {
        (edge.to.clone(), edge.from.clone())
    }
}

fn build_edge_pair_counts(edges: &[crate::ir::Edge]) -> HashMap<(String, String), usize> {
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for edge in edges {
        let key = edge_pair_key(edge);
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn path_intersects_obstacles(
    points: &[(f32, f32)],
    obstacles: &[Obstacle],
    from_id: &str,
    to_id: &str,
) -> bool {
    if points.len() < 2 {
        return false;
    }

    for segment in points.windows(2) {
        let (a, b) = (segment[0], segment[1]);
        for obstacle in obstacles {
            if obstacle.id == from_id || obstacle.id == to_id {
                continue;
            }
            if let Some(members) = &obstacle.members
                && (members.contains(from_id) || members.contains(to_id))
            {
                continue;
            }
            if segment_intersects_rect(a, b, obstacle) {
                return true;
            }
        }
    }
    false
}

fn segment_intersects_rect(a: (f32, f32), b: (f32, f32), rect: &Obstacle) -> bool {
    let (x1, y1) = a;
    let (x2, y2) = b;
    if (x1 - x2).abs() < f32::EPSILON {
        let x = x1;
        if x >= rect.x && x <= rect.x + rect.width {
            let min_y = y1.min(y2);
            let max_y = y1.max(y2);
            return max_y >= rect.y && min_y <= rect.y + rect.height;
        }
    } else if (y1 - y2).abs() < f32::EPSILON {
        let y = y1;
        if y >= rect.y && y <= rect.y + rect.height {
            let min_x = x1.min(x2);
            let max_x = x1.max(x2);
            return max_x >= rect.x && min_x <= rect.x + rect.width;
        }
    }
    false
}

fn measure_label(text: &str, theme: &Theme, config: &LayoutConfig) -> TextBlock {
    let raw_lines = split_lines(text);
    let mut lines = Vec::new();
    for line in raw_lines {
        let wrapped = wrap_line(&line, config.max_label_width_chars);
        lines.extend(wrapped);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    let max_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1);
    let approx_char_width = theme.font_size * 0.6;
    let width = max_len as f32 * approx_char_width;
    let height = lines.len() as f32 * theme.font_size * config.label_line_height;

    TextBlock {
        lines,
        width,
        height,
    }
}

fn split_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = text.replace("<br/>", "\n").replace("<br>", "\n");
    current = current.replace("\\n", "\n");
    for line in current.split('\n') {
        lines.push(line.trim().to_string());
    }
    lines
}

fn wrap_line(line: &str, max_chars: usize) -> Vec<String> {
    if line.chars().count() <= max_chars {
        return vec![line.to_string()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    for word in line.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", current, word)
        };
        if candidate.chars().count() > max_chars {
            if !current.is_empty() {
                lines.push(current.clone());
                current.clear();
            }
            current.push_str(word);
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn resolve_node_style(node_id: &str, graph: &Graph) -> crate::ir::NodeStyle {
    let mut style = crate::ir::NodeStyle::default();

    if let Some(classes) = graph.node_classes.get(node_id) {
        for class_name in classes {
            if let Some(class_style) = graph.class_defs.get(class_name) {
                merge_node_style(&mut style, class_style);
            }
        }
    }

    if let Some(node_style) = graph.node_styles.get(node_id) {
        merge_node_style(&mut style, node_style);
    }

    style
}

fn resolve_subgraph_style(sub: &crate::ir::Subgraph, graph: &Graph) -> crate::ir::NodeStyle {
    let mut style = crate::ir::NodeStyle::default();
    let Some(id) = sub.id.as_ref() else {
        return style;
    };

    if let Some(classes) = graph.subgraph_classes.get(id) {
        for class_name in classes {
            if let Some(class_style) = graph.class_defs.get(class_name) {
                merge_node_style(&mut style, class_style);
            }
        }
    }

    if let Some(sub_style) = graph.subgraph_styles.get(id) {
        merge_node_style(&mut style, sub_style);
    }

    style
}

fn build_subgraph_layouts(
    graph: &Graph,
    nodes: &BTreeMap<String, NodeLayout>,
    theme: &Theme,
    config: &LayoutConfig,
) -> Vec<SubgraphLayout> {
    let mut subgraphs = Vec::new();
    for sub in &graph.subgraphs {
        let is_region = is_region_subgraph(sub);
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for node_id in &sub.nodes {
            if let Some(node) = nodes.get(node_id) {
                min_x = min_x.min(node.x);
                min_y = min_y.min(node.y);
                max_x = max_x.max(node.x + node.width);
                max_y = max_y.max(node.y + node.height);
            }
        }

        if min_x == f32::MAX {
            continue;
        }

        let style = resolve_subgraph_style(sub, graph);
        let mut label_block = measure_label(&sub.label, theme, config);
        let base_padding = if graph.kind == crate::ir::DiagramKind::State {
            16.0
        } else {
            24.0
        };
        let padding = if is_region { 0.0 } else { base_padding };
        let label_empty = sub.label.trim().is_empty();
        if label_empty {
            label_block.width = 0.0;
            label_block.height = 0.0;
        }
        let label_height = if label_empty { 0.0 } else { label_block.height };
        let top_padding = if label_empty {
            padding
        } else {
            if graph.kind == crate::ir::DiagramKind::State {
                (label_height + theme.font_size * 0.4).max(18.0)
            } else {
                padding + label_height + 8.0
            }
        };

        subgraphs.push(SubgraphLayout {
            label: sub.label.clone(),
            label_block,
            nodes: sub.nodes.clone(),
            x: min_x - padding,
            y: min_y - top_padding,
            width: (max_x - min_x) + padding * 2.0,
            height: (max_y - min_y) + padding + top_padding,
            style,
        });
    }

    if subgraphs.len() > 1 {
        let sets: Vec<HashSet<String>> = graph
            .subgraphs
            .iter()
            .map(|sub| sub.nodes.iter().cloned().collect())
            .collect();

        let mut order: Vec<usize> = (0..subgraphs.len()).collect();
        order.sort_by_key(|i| sets[*i].len());

        for &i in &order {
            for &j in &order {
                if i == j {
                    continue;
                }
                if is_region_subgraph(&graph.subgraphs[j]) {
                    continue;
                }
                if sets[j].len() >= sets[i].len() {
                    continue;
                }
                if !sets[j].is_subset(&sets[i]) {
                    continue;
                }
                let pad = 12.0;
                let (child_x, child_y, child_w, child_h) = {
                    let child = &subgraphs[j];
                    (child.x, child.y, child.width, child.height)
                };
                let parent = &mut subgraphs[i];
                let min_x = parent.x.min(child_x - pad);
                let min_y = parent.y.min(child_y - pad);
                let max_x = (parent.x + parent.width).max(child_x + child_w + pad);
                let max_y = (parent.y + parent.height).max(child_y + child_h + pad);
                parent.x = min_x;
                parent.y = min_y;
                parent.width = max_x - min_x;
                parent.height = max_y - min_y;
            }
        }
    }

    subgraphs.sort_by(|a, b| {
        let area_a = a.width * a.height;
        let area_b = b.width * b.height;
        area_b.partial_cmp(&area_a).unwrap_or(Ordering::Equal)
    });
    subgraphs
}

fn merge_node_style(target: &mut crate::ir::NodeStyle, source: &crate::ir::NodeStyle) {
    if source.fill.is_some() {
        target.fill = source.fill.clone();
    }
    if source.stroke.is_some() {
        target.stroke = source.stroke.clone();
    }
    if source.text_color.is_some() {
        target.text_color = source.text_color.clone();
    }
    if source.stroke_width.is_some() {
        target.stroke_width = source.stroke_width;
    }
    if source.stroke_dasharray.is_some() {
        target.stroke_dasharray = source.stroke_dasharray.clone();
    }
}

fn shape_size(shape: crate::ir::NodeShape, label: &TextBlock, config: &LayoutConfig) -> (f32, f32) {
    let mut width = label.width + config.node_padding_x * 2.0;
    let mut height = label.height + config.node_padding_y * 2.0;
    let label_empty = label.lines.len() == 1 && label.lines[0].trim().is_empty();

    match shape {
        crate::ir::NodeShape::Diamond => {
            width *= 1.4;
            height *= 1.4;
        }
        crate::ir::NodeShape::Circle | crate::ir::NodeShape::DoubleCircle => {
            let size = if label_empty {
                (config.node_padding_y * 1.4).max(14.0)
            } else {
                width.max(height)
            };
            width = size;
            height = size;
        }
        crate::ir::NodeShape::Stadium | crate::ir::NodeShape::RoundRect => {
            width *= 1.1;
            height *= 1.05;
        }
        crate::ir::NodeShape::Cylinder => {
            width *= 1.1;
            height *= 1.1;
        }
        crate::ir::NodeShape::Hexagon => {
            width *= 1.2;
            height *= 1.1;
        }
        crate::ir::NodeShape::Parallelogram
        | crate::ir::NodeShape::ParallelogramAlt
        | crate::ir::NodeShape::Trapezoid
        | crate::ir::NodeShape::TrapezoidAlt
        | crate::ir::NodeShape::Asymmetric => {
            width *= 1.2;
        }
        crate::ir::NodeShape::Subroutine => {
            width *= 1.1;
        }
        _ => {}
    }

    (width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Direction, Graph, NodeShape};

    #[test]
    fn wraps_long_labels() {
        let theme = Theme::modern();
        let mut config = LayoutConfig::default();
        config.max_label_width_chars = 8;
        let block = measure_label("this is a long label", &theme, &config);
        assert!(block.lines.len() > 1);
    }

    #[test]
    fn layout_places_nodes() {
        let mut graph = Graph::new();
        graph.direction = Direction::LeftRight;
        graph.ensure_node("A", Some("Alpha".to_string()), Some(NodeShape::Rectangle));
        graph.ensure_node("B", Some("Beta".to_string()), Some(NodeShape::Rectangle));
        graph.edges.push(crate::ir::Edge {
            from: "A".to_string(),
            to: "B".to_string(),
            label: None,
            directed: true,
            arrow_start: false,
            arrow_end: true,
            start_decoration: None,
            end_decoration: None,
            style: crate::ir::EdgeStyle::Solid,
        });
        let layout = compute_layout(&graph, &Theme::modern(), &LayoutConfig::default());
        let a = layout.nodes.get("A").unwrap();
        let b = layout.nodes.get("B").unwrap();
        assert!(b.x >= a.x);
    }

    #[test]
    fn edge_style_merges_default_and_override() {
        let mut graph = Graph::new();
        graph.ensure_node("A", Some("Alpha".to_string()), Some(NodeShape::Rectangle));
        graph.ensure_node("B", Some("Beta".to_string()), Some(NodeShape::Rectangle));
        graph.edges.push(crate::ir::Edge {
            from: "A".to_string(),
            to: "B".to_string(),
            label: None,
            directed: true,
            arrow_start: false,
            arrow_end: true,
            start_decoration: None,
            end_decoration: None,
            style: crate::ir::EdgeStyle::Solid,
        });

        graph.edge_style_default = Some(crate::ir::EdgeStyleOverride {
            stroke: Some("#111111".to_string()),
            stroke_width: None,
            dasharray: None,
            label_color: Some("#222222".to_string()),
        });
        graph.edge_styles.insert(
            0,
            crate::ir::EdgeStyleOverride {
                stroke: None,
                stroke_width: Some(4.0),
                dasharray: None,
                label_color: None,
            },
        );

        let layout = compute_layout(&graph, &Theme::modern(), &LayoutConfig::default());
        let edge = &layout.edges[0];
        assert_eq!(edge.override_style.stroke.as_deref(), Some("#111111"));
        assert_eq!(edge.override_style.stroke_width, Some(4.0));
        assert_eq!(edge.override_style.label_color.as_deref(), Some("#222222"));
    }
}
