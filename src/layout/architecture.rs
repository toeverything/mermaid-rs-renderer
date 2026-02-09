use super::*;

pub(super) fn compute_architecture_layout(
    graph: &Graph,
    theme: &Theme,
    config: &LayoutConfig,
) -> Layout {
    const MARGIN: f32 = 24.0;
    const SERVICE_SIZE: f32 = 64.0;
    const SERVICE_GAP: f32 = 72.0;
    const GROUP_PAD_X: f32 = 28.0;
    const GROUP_PAD_TOP: f32 = 32.0;
    const GROUP_PAD_BOTTOM: f32 = 44.0;
    const GROUP_GAP_Y: f32 = 48.0;
    const GROUP_STROKE: &str = "hsl(240, 60%, 86.2745098039%)";
    const ICON_FILL: &str = "#087ebf";

    let mut nodes = BTreeMap::new();

    for node in graph.nodes.values() {
        let label = measure_label(&node.label, theme, config);
        let mut style = resolve_node_style(node.id.as_str(), graph);
        if style.fill.is_none() {
            style.fill = Some(ICON_FILL.to_string());
        }
        if style.stroke.is_none() {
            style.stroke = Some("none".to_string());
        }
        if style.stroke_width.is_none() {
            style.stroke_width = Some(0.0);
        }
        let mut nl = build_node_layout(node, label, SERVICE_SIZE, SERVICE_SIZE, style, graph);
        nl.shape = crate::ir::NodeShape::Rectangle;
        nl.icon = node.icon.clone();
        nodes.insert(node.id.clone(), nl);
    }

    let mut assigned: HashSet<String> = HashSet::new();
    let mut subgraphs = Vec::new();
    let mut current_y = MARGIN;

    for sub in &graph.subgraphs {
        let mut group_nodes: Vec<String> = sub
            .nodes
            .iter()
            .filter(|id| nodes.contains_key(*id))
            .cloned()
            .collect();
        if group_nodes.is_empty() {
            continue;
        }
        group_nodes.sort_by(|a, b| {
            let order_a = graph.node_order.get(a).copied().unwrap_or(usize::MAX);
            let order_b = graph.node_order.get(b).copied().unwrap_or(usize::MAX);
            order_a.cmp(&order_b).then_with(|| a.cmp(b))
        });
        assigned.extend(group_nodes.iter().cloned());

        let count = group_nodes.len() as f32;
        let gaps = (count - 1.0).max(0.0);
        let group_width = GROUP_PAD_X * 2.0 + SERVICE_SIZE * count + SERVICE_GAP * gaps;
        let group_height = GROUP_PAD_TOP + SERVICE_SIZE + GROUP_PAD_BOTTOM;
        let group_x = MARGIN;
        let group_y = current_y;

        let mut x_cursor = group_x + GROUP_PAD_X;
        for node_id in &group_nodes {
            if let Some(node) = nodes.get_mut(node_id) {
                node.x = x_cursor;
                node.y = group_y + GROUP_PAD_TOP;
            }
            x_cursor += SERVICE_SIZE + SERVICE_GAP;
        }

        let label_block = measure_label(&sub.label, theme, config);
        let mut style = resolve_subgraph_style(sub, graph);
        style.fill = Some("none".to_string());
        style.stroke = Some(GROUP_STROKE.to_string());
        style.stroke_width = Some(2.0);
        style.stroke_dasharray = Some("8".to_string());
        if style.text_color.is_none() {
            style.text_color = Some(theme.primary_text_color.clone());
        }

        subgraphs.push(SubgraphLayout {
            label: sub.label.clone(),
            label_block,
            nodes: group_nodes,
            x: group_x,
            y: group_y,
            width: group_width,
            height: group_height,
            style,
            icon: sub.icon.clone(),
        });

        current_y += group_height + GROUP_GAP_Y;
    }

    let mut free_nodes: Vec<String> = nodes
        .keys()
        .filter(|id| !assigned.contains(*id))
        .cloned()
        .collect();
    free_nodes.sort_by(|a, b| {
        let order_a = graph.node_order.get(a).copied().unwrap_or(usize::MAX);
        let order_b = graph.node_order.get(b).copied().unwrap_or(usize::MAX);
        order_a.cmp(&order_b).then_with(|| a.cmp(b))
    });
    if !free_nodes.is_empty() {
        let row_y = current_y;
        let mut x_cursor = MARGIN + GROUP_PAD_X;
        for node_id in &free_nodes {
            if let Some(node) = nodes.get_mut(node_id) {
                node.x = x_cursor;
                node.y = row_y + GROUP_PAD_TOP;
            }
            x_cursor += SERVICE_SIZE + SERVICE_GAP;
        }
    }

    let mut edges = Vec::new();
    for (idx, edge) in graph.edges.iter().enumerate() {
        let Some(from) = nodes.get(&edge.from) else {
            continue;
        };
        let Some(to) = nodes.get(&edge.to) else {
            continue;
        };
        let start_x = from.x + SERVICE_SIZE;
        let start_y = from.y + SERVICE_SIZE / 2.0;
        let end_x = to.x;
        let end_y = to.y + SERVICE_SIZE / 2.0;
        let mut override_style = resolve_edge_style(idx, graph);
        if override_style.stroke.is_none() {
            override_style.stroke = Some(theme.line_color.clone());
        }
        override_style.stroke_width = Some(override_style.stroke_width.unwrap_or(3.0));

        edges.push(EdgeLayout {
            from: edge.from.clone(),
            to: edge.to.clone(),
            label: None,
            start_label: None,
            end_label: None,
            label_anchor: None,
            start_label_anchor: None,
            end_label_anchor: None,
            points: vec![(start_x, start_y), (end_x, end_y)],
            directed: true,
            arrow_start: false,
            arrow_end: true,
            arrow_start_kind: None,
            arrow_end_kind: None,
            start_decoration: None,
            end_decoration: None,
            style: edge.style,
            override_style,
        });
    }

    let (max_x, max_y) = bounds_with_edges(&nodes, &subgraphs, &edges);
    let width = (max_x + MARGIN).max(200.0);
    let height = (max_y + MARGIN).max(200.0);

    Layout {
        kind: graph.kind,
        nodes,
        edges,
        subgraphs,
        width,
        height,
        diagram: DiagramData::Graph { state_notes: Vec::new() },
    }
}
