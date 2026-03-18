use std::collections::BTreeMap;

use crate::config::LayoutConfig;
use crate::ir::Graph;
use crate::theme::Theme;

use super::text::measure_label;
use super::{DiagramData, Layout, QuadrantLayout, QuadrantPointLayout, TextBlock};
use super::{PlacementBounds, place_anchored_rect, rect_from_center};

fn quadrant_palette(_theme: &Theme) -> Vec<String> {
    vec![
        "#6366f1".to_string(), // indigo
        "#f59e0b".to_string(), // amber
        "#10b981".to_string(), // emerald
        "#ef4444".to_string(), // red
        "#8b5cf6".to_string(), // violet
        "#06b6d4".to_string(), // cyan
    ]
}

pub(super) fn compute_quadrant_layout(
    graph: &Graph,
    theme: &Theme,
    config: &LayoutConfig,
) -> Layout {
    let padding = theme.font_size * 1.6;
    let grid_size = 360.0;
    // Measure title
    let title = graph
        .quadrant
        .title
        .as_ref()
        .map(|t| measure_label(t, theme, config));
    let title_height = title.as_ref().map(|t| t.height + padding).unwrap_or(0.0);

    // Measure axis labels
    let x_left = graph
        .quadrant
        .x_axis_left
        .as_ref()
        .map(|t| measure_label(t, theme, config));
    let x_right = graph
        .quadrant
        .x_axis_right
        .as_ref()
        .map(|t| measure_label(t, theme, config));
    let y_bottom = graph
        .quadrant
        .y_axis_bottom
        .as_ref()
        .map(|t| measure_label(t, theme, config));
    let y_top = graph
        .quadrant
        .y_axis_top
        .as_ref()
        .map(|t| measure_label(t, theme, config));

    // Measure quadrant labels
    let q_labels: [Option<TextBlock>; 4] = [
        graph.quadrant.quadrant_labels[0]
            .as_ref()
            .map(|t| measure_label(t, theme, config)),
        graph.quadrant.quadrant_labels[1]
            .as_ref()
            .map(|t| measure_label(t, theme, config)),
        graph.quadrant.quadrant_labels[2]
            .as_ref()
            .map(|t| measure_label(t, theme, config)),
        graph.quadrant.quadrant_labels[3]
            .as_ref()
            .map(|t| measure_label(t, theme, config)),
    ];

    let y_axis_label_width = y_bottom
        .as_ref()
        .map(|t| t.width)
        .unwrap_or(0.0)
        .max(y_top.as_ref().map(|t| t.width).unwrap_or(0.0));
    let y_axis_width = if y_axis_label_width > 0.0 {
        y_axis_label_width + padding
    } else {
        padding
    };
    let x_axis_height = x_left
        .as_ref()
        .map(|t| t.height + padding)
        .unwrap_or(padding);

    let grid_x = y_axis_width + padding;
    let grid_y = title_height + padding;

    let provisional_width = grid_x + grid_size + padding * 2.0;
    let provisional_height = grid_y + grid_size + x_axis_height + padding;
    let bounds = PlacementBounds {
        min_x: padding * 0.5,
        min_y: padding * 0.5,
        max_x: provisional_width - padding * 0.5,
        max_y: provisional_height - padding * 0.5,
    };
    let title_center_x = grid_x + grid_size / 2.0;
    let title_center_y = title_height / 2.0;
    let quadrant_label_centers = [
        (grid_x + grid_size * 0.75, grid_y + 15.0),
        (grid_x + grid_size * 0.25, grid_y + 15.0),
        (grid_x + grid_size * 0.25, grid_y + grid_size * 0.5 + 15.0),
        (grid_x + grid_size * 0.75, grid_y + grid_size * 0.5 + 15.0),
    ];
    let x_axis_centers = [
        (grid_x + grid_size * 0.25, grid_y + grid_size + 20.0),
        (grid_x + grid_size * 0.75, grid_y + grid_size + 20.0),
    ];
    let y_axis_centers = [
        (grid_x - theme.font_size * 2.2, grid_y + grid_size * 0.75),
        (grid_x - theme.font_size * 2.2, grid_y + grid_size * 0.25),
    ];
    let mut occupied = Vec::new();
    if let Some(block) = title.as_ref() {
        occupied.push(rect_from_center(
            (title_center_x, title_center_y),
            block.width,
            block.height,
        ));
    }
    for (center, label) in quadrant_label_centers.iter().zip(q_labels.iter()) {
        if let Some(block) = label.as_ref() {
            occupied.push(rect_from_center(*center, block.width, block.height));
        }
    }
    for (center, label) in x_axis_centers
        .iter()
        .zip([x_left.as_ref(), x_right.as_ref()])
    {
        if let Some(block) = label {
            occupied.push(rect_from_center(*center, block.width, block.height));
        }
    }
    for (center, label) in y_axis_centers
        .iter()
        .zip([y_bottom.as_ref(), y_top.as_ref()])
    {
        if let Some(block) = label {
            occupied.push(rect_from_center(*center, block.width, block.height));
        }
    }
    let point_radius = 5.0f32;
    let point_gap = (theme.font_size * 0.45).max(8.0);

    // Layout points
    let palette = quadrant_palette(theme);
    let points: Vec<QuadrantPointLayout> = graph
        .quadrant
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let px = grid_x + p.x.clamp(0.0, 1.0) * grid_size;
            let py = grid_y + (1.0 - p.y.clamp(0.0, 1.0)) * grid_size; // Invert Y
            let label = measure_label(&p.label, theme, config);
            occupied.push(rect_from_center(
                (px, py),
                point_radius * 2.0 + 2.0,
                point_radius * 2.0 + 2.0,
            ));
            let base_y = py + point_radius + point_gap + label.height / 2.0;
            let base_x = px;
            let horizontal_gap = point_radius + point_gap + label.width / 2.0;
            let diagonal_x = horizontal_gap * 0.72;
            let diagonal_y = point_radius + point_gap + label.height * 0.4;
            let step = theme.font_size.max(10.0);
            let mut candidates = Vec::new();
            for extra in [0.0, step, step * 2.0, step * 3.0] {
                candidates.push((base_x, base_y + extra));
                candidates.push((
                    base_x,
                    py - point_radius - point_gap - label.height / 2.0 - extra,
                ));
                candidates.push((px + horizontal_gap + extra, py));
                candidates.push((px - horizontal_gap - extra, py));
                candidates.push((px + diagonal_x + extra, py + diagonal_y + extra * 0.6));
                candidates.push((px - diagonal_x - extra, py + diagonal_y + extra * 0.6));
                candidates.push((px + diagonal_x + extra, py - diagonal_y - extra * 0.6));
                candidates.push((px - diagonal_x - extra, py - diagonal_y - extra * 0.6));
            }
            let placement = place_anchored_rect(
                label.width,
                label.height,
                &candidates,
                &occupied,
                Some(bounds),
                (px, base_y),
                0.35,
                0.08,
            );
            occupied.push(placement.rect);
            QuadrantPointLayout {
                label,
                x: px,
                y: py,
                label_x: placement.center.0,
                label_y: placement.center.1,
                color: palette[i % palette.len()].clone(),
            }
        })
        .collect();

    let mut width = provisional_width;
    let mut height = provisional_height;
    for rect in &occupied {
        width = width.max(rect.0 + rect.2 + padding * 0.4);
        height = height.max(rect.1 + rect.3 + padding * 0.4);
    }

    Layout {
        kind: graph.kind,
        nodes: BTreeMap::new(),
        edges: Vec::new(),
        subgraphs: Vec::new(),
        width,
        height,
        diagram: DiagramData::Quadrant(QuadrantLayout {
            title,
            title_y: title_height / 2.0,
            x_axis_left: x_left,
            x_axis_right: x_right,
            y_axis_bottom: y_bottom,
            y_axis_top: y_top,
            quadrant_labels: q_labels,
            points,
            grid_x,
            grid_y,
            grid_width: grid_size,
            grid_height: grid_size,
        }),
    }
}
