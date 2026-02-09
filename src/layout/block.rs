use super::*;

pub(super) fn compute_block_layout(
    graph: &Graph,
    theme: &Theme,
    config: &LayoutConfig,
) -> Layout {
    let mut nodes = build_graph_node_layouts(graph, theme, config);

    let node_gap = (theme.font_size * 0.4).max(4.0);
    let column_gap = (theme.font_size * 0.45).max(6.0);
    let origin_x = 6.0;
    let origin_y = 6.0;

    let mut edges: Vec<EdgeLayout> = Vec::new();

    let Some(block) = graph.block.as_ref() else {
        let mut subgraphs = build_subgraph_layouts(graph, &nodes, theme, config);
        normalize_layout(&mut nodes, edges.as_mut_slice(), &mut subgraphs);
        let (max_x, max_y) = bounds_without_padding(&nodes, &subgraphs);
        return Layout {
            kind: graph.kind,
            nodes,
            edges,
            subgraphs,
            width: max_x + 6.0,
            height: max_y + 6.0,
            diagram: DiagramData::Graph { state_notes: Vec::new() },
        };
    };

    let columns = block.columns.unwrap_or_else(|| block.nodes.len().max(1));
    let mut column_widths = vec![0.0f32; columns];
    let mut column_x = vec![0.0f32; columns];
    let mut row_y = Vec::<f32>::new();

    let mut row = 0usize;
    let mut col = 0usize;
    let mut row_heights: Vec<f32> = vec![0.0];

    for node in &block.nodes {
        if col >= columns {
            col = 0;
            row += 1;
            row_heights.push(0.0);
        }
        let span = node.span.max(1).min(columns);
        if col + span > columns {
            col = 0;
            row += 1;
            row_heights.push(0.0);
        }
        if !node.is_space
            && let Some(layout) = nodes.get(&node.id)
        {
            let per_col = layout.width / span as f32;
            for i in 0..span {
                let idx = col + i;
                if idx < columns {
                    column_widths[idx] = column_widths[idx].max(per_col);
                }
            }
            row_heights[row] = row_heights[row].max(layout.height);
        }
        col += span;
    }

    column_x[0] = origin_x;
    for i in 1..columns {
        column_x[i] = column_x[i - 1] + column_widths[i - 1] + column_gap;
    }

    let mut y_cursor = origin_y;
    for h in &row_heights {
        row_y.push(y_cursor);
        y_cursor += *h + node_gap;
    }

    row = 0;
    col = 0;
    for node in &block.nodes {
        if col >= columns {
            col = 0;
            row += 1;
        }
        let span = node.span.max(1).min(columns);
        if col + span > columns {
            col = 0;
            row += 1;
        }
        if !node.is_space
            && let Some(layout) = nodes.get_mut(&node.id)
        {
            let start_x = column_x[col];
            let mut span_width = 0.0;
            for i in 0..span {
                let idx = col + i;
                if idx < columns {
                    span_width += column_widths[idx];
                    if i + 1 < span {
                        span_width += column_gap;
                    }
                }
            }
            let x = start_x + (span_width - layout.width) / 2.0;
            let y = row_y[row] + (row_heights[row] - layout.height) / 2.0;
            layout.x = x;
            layout.y = y;
        }
        col += span;
    }

    for edge in &graph.edges {
        let Some(from_layout) = nodes.get(&edge.from) else {
            continue;
        };
        let Some(to_layout) = nodes.get(&edge.to) else {
            continue;
        };
        let from_center = (
            from_layout.x + from_layout.width / 2.0,
            from_layout.y + from_layout.height / 2.0,
        );
        let to_center = (
            to_layout.x + to_layout.width / 2.0,
            to_layout.y + to_layout.height / 2.0,
        );
        let label = edge.label.as_ref().map(|l| measure_label(l, theme, config));
        let start_label = edge
            .start_label
            .as_ref()
            .map(|l| measure_label(l, theme, config));
        let end_label = edge
            .end_label
            .as_ref()
            .map(|l| measure_label(l, theme, config));
        let mut override_style = resolve_edge_style(edges.len(), graph);
        if edge.style == crate::ir::EdgeStyle::Dotted && override_style.dasharray.is_none() {
            override_style.dasharray = Some("3 3".to_string());
        }
        edges.push(EdgeLayout {
            from: edge.from.clone(),
            to: edge.to.clone(),
            label,
            start_label,
            end_label,
            label_anchor: None,
            start_label_anchor: None,
            end_label_anchor: None,
            points: vec![from_center, to_center],
            directed: edge.directed,
            arrow_start: edge.arrow_start,
            arrow_end: edge.arrow_end,
            arrow_start_kind: edge.arrow_start_kind,
            arrow_end_kind: edge.arrow_end_kind,
            start_decoration: edge.start_decoration,
            end_decoration: edge.end_decoration,
            style: edge.style,
            override_style,
        });
    }

    let mut subgraphs = build_subgraph_layouts(graph, &nodes, theme, config);
    normalize_layout(&mut nodes, edges.as_mut_slice(), &mut subgraphs);

    let (max_x, max_y) = bounds_with_edges(&nodes, &subgraphs, &edges);
    let width = max_x + 6.0;
    let height = max_y + 6.0;

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
