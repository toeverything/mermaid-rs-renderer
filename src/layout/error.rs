use std::collections::BTreeMap;

use crate::config::LayoutConfig;
use crate::ir::Graph;

use super::{ErrorLayout, Layout};

pub(super) fn compute_error_layout(graph: &Graph, config: &LayoutConfig) -> Layout {
    let viewbox_width = config.treemap.error_viewbox_width.max(1.0);
    let viewbox_height = config.treemap.error_viewbox_height.max(1.0);
    let render_width = config.treemap.error_render_width.max(1.0);
    let derived_height = render_width * viewbox_height / viewbox_width;
    let render_height = match config.treemap.error_render_height {
        Some(height) => height,
        None => derived_height.round(),
    }
    .max(1.0);
    Layout {
        kind: graph.kind,
        nodes: BTreeMap::new(),
        edges: Vec::new(),
        subgraphs: Vec::new(),
        lifelines: Vec::new(),
        sequence_footboxes: Vec::new(),
        sequence_boxes: Vec::new(),
        sequence_frames: Vec::new(),
        sequence_notes: Vec::new(),
        sequence_activations: Vec::new(),
        sequence_numbers: Vec::new(),
        state_notes: Vec::new(),
        pie_slices: Vec::new(),
        pie_legend: Vec::new(),
        pie_center: (0.0, 0.0),
        pie_radius: 0.0,
        pie_title: None,
        quadrant: None,
        gantt: None,
        sankey: None,
        gitgraph: None,
        c4: None,
        xychart: None,
        timeline: None,
        journey: None,
        error: Some(ErrorLayout {
            viewbox_width,
            viewbox_height,
            render_width,
            render_height,
            message: config.treemap.error_message.clone(),
            version: config.treemap.error_version.clone(),
            text_x: config.treemap.error_text_x,
            text_y: config.treemap.error_text_y,
            text_size: config.treemap.error_text_size,
            version_x: config.treemap.error_version_x,
            version_y: config.treemap.error_version_y,
            version_size: config.treemap.error_version_size,
            icon_scale: config.treemap.icon_scale,
            icon_tx: config.treemap.icon_tx,
            icon_ty: config.treemap.icon_ty,
        }),

        width: render_width,
        height: render_height,
    }
}

pub(super) fn compute_pie_error_layout(graph: &Graph, config: &LayoutConfig) -> Layout {
    let viewbox_width = config.pie.error_viewbox_width.max(1.0);
    let viewbox_height = config.pie.error_viewbox_height.max(1.0);
    let render_width = config.pie.error_render_width.max(1.0);
    let derived_height = render_width * viewbox_height / viewbox_width;
    let render_height = match config.pie.error_render_height {
        Some(height) => height,
        None => derived_height.round(),
    }
    .max(1.0);
    Layout {
        kind: graph.kind,
        nodes: BTreeMap::new(),
        edges: Vec::new(),
        subgraphs: Vec::new(),
        lifelines: Vec::new(),
        sequence_footboxes: Vec::new(),
        sequence_boxes: Vec::new(),
        sequence_frames: Vec::new(),
        sequence_notes: Vec::new(),
        sequence_activations: Vec::new(),
        sequence_numbers: Vec::new(),
        state_notes: Vec::new(),
        pie_slices: Vec::new(),
        pie_legend: Vec::new(),
        pie_center: (0.0, 0.0),
        pie_radius: 0.0,
        pie_title: None,
        quadrant: None,
        gantt: None,
        sankey: None,
        gitgraph: None,
        c4: None,
        xychart: None,
        timeline: None,
        journey: None,
        error: Some(ErrorLayout {
            viewbox_width,
            viewbox_height,
            render_width,
            render_height,
            message: config.pie.error_message.clone(),
            version: config.pie.error_version.clone(),
            text_x: config.pie.error_text_x,
            text_y: config.pie.error_text_y,
            text_size: config.pie.error_text_size,
            version_x: config.pie.error_version_x,
            version_y: config.pie.error_version_y,
            version_size: config.pie.error_version_size,
            icon_scale: config.pie.icon_scale,
            icon_tx: config.pie.icon_tx,
            icon_ty: config.pie.icon_ty,
        }),

        width: render_width,
        height: render_height,
    }
}
