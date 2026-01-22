use crate::config::{LayoutConfig, RenderConfig};
use crate::layout::{EdgeLayout, Layout, TextBlock};
use crate::theme::Theme;
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

pub fn render_svg(layout: &Layout, theme: &Theme, config: &LayoutConfig) -> String {
    let mut svg = String::new();
    let width = layout.width.max(200.0);
    let height = layout.height.max(200.0);

    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">",
    ));

    svg.push_str(&format!(
        "<rect width=\"100%\" height=\"100%\" fill=\"{}\"/>",
        theme.background
    ));

    svg.push_str("<defs>");
    svg.push_str(&format!(
        "<marker id=\"arrow\" viewBox=\"0 0 10 10\" refX=\"10\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\"><path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"{}\"/></marker>",
        theme.line_color
    ));
    svg.push_str("</defs>");

    for subgraph in &layout.subgraphs {
        svg.push_str(&format!(
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"10\" ry=\"10\" fill=\"{}\" stroke=\"{}\" stroke-dasharray=\"6 4\" stroke-width=\"1.2\"/>",
            subgraph.x,
            subgraph.y,
            subgraph.width,
            subgraph.height,
            theme.cluster_background,
            theme.cluster_border
        ));
        let label_x = subgraph.x + 12.0;
        let label_y = subgraph.y + 20.0;
        svg.push_str(&format!(
            "<text x=\"{label_x:.2}\" y=\"{label_y:.2}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
            theme.font_family,
            theme.font_size,
            theme.primary_text_color,
            escape_xml(&subgraph.label)
        ));
    }

    let label_positions = compute_edge_label_positions(&layout.edges, theme, config);

    for (idx, edge) in layout.edges.iter().enumerate() {
        let d = points_to_path(&edge.points);
        let marker = if edge.directed { "marker-end=\"url(#arrow)\"" } else { "" };
        svg.push_str(&format!(
            "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.4\" {} />",
            d,
            theme.line_color,
            marker
        ));

        if let Some((x, y, label)) = label_positions.get(&idx).and_then(|v| v.clone()) {
            let rect_x = x - label.width / 2.0 - 6.0;
            let rect_y = y - label.height / 2.0 - 4.0;
            let rect_w = label.width + 12.0;
            let rect_h = label.height + 8.0;
            svg.push_str(&format!(
                "<rect x=\"{rect_x:.2}\" y=\"{rect_y:.2}\" width=\"{rect_w:.2}\" height=\"{rect_h:.2}\" rx=\"6\" ry=\"6\" fill=\"{}\" stroke=\"{}\" stroke-width=\"0.8\"/>",
                theme.edge_label_background,
                theme.primary_border_color
            ));
            svg.push_str(&text_block_svg(x, y, &label, theme, config, true));
        }
    }

    for node in layout.nodes.values() {
        svg.push_str(&format!(
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"10\" ry=\"10\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1.4\"/>",
            node.x,
            node.y,
            node.width,
            node.height,
            theme.primary_color,
            theme.primary_border_color
        ));
        let center_x = node.x + node.width / 2.0;
        let center_y = node.y + node.height / 2.0;
        svg.push_str(&text_block_svg(center_x, center_y, &node.label, theme, config, false));
    }

    svg.push_str("</svg>");
    svg
}

fn points_to_path(points: &[(f32, f32)]) -> String {
    if points.is_empty() {
        return String::new();
    }
    let mut d = String::new();
    d.push_str(&format!("M {:.2} {:.2}", points[0].0, points[0].1));
    for point in points.iter().skip(1) {
        d.push_str(&format!(" L {:.2} {:.2}", point.0, point.1));
    }
    d
}

fn text_block_svg(x: f32, y: f32, label: &TextBlock, theme: &Theme, config: &LayoutConfig, edge: bool) -> String {
    let total_height = label.lines.len() as f32 * theme.font_size * config.label_line_height;
    let start_y = y - total_height / 2.0 + theme.font_size;
    let mut text = String::new();
    let anchor = "middle";
    let fill = if edge { theme.primary_text_color.as_str() } else { theme.primary_text_color.as_str() };

    text.push_str(&format!(
        "<text x=\"{x:.2}\" y=\"{start_y:.2}\" text-anchor=\"{anchor}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\">",
        theme.font_family,
        theme.font_size,
        fill
    ));

    for (idx, line) in label.lines.iter().enumerate() {
        if idx == 0 {
            text.push_str(&format!("<tspan x=\"{x:.2}\" dy=\"0\">{}", escape_xml(line)));
        } else {
            let dy = theme.font_size * config.label_line_height;
            text.push_str(&format!("<tspan x=\"{x:.2}\" dy=\"{dy:.2}\">{}", escape_xml(line)));
        }
        text.push_str("</tspan>");
    }

    text.push_str("</text>");
    text
}

fn compute_edge_label_positions(
    edges: &[EdgeLayout],
    _theme: &Theme,
    _config: &LayoutConfig,
) -> HashMap<usize, Option<(f32, f32, TextBlock)>> {
    let mut occupied: Vec<(f32, f32, f32, f32)> = Vec::new();
    let mut positions = HashMap::new();

    for (idx, edge) in edges.iter().enumerate() {
        let Some(label) = edge.label.clone() else {
            positions.insert(idx, None);
            continue;
        };
        let (mid_x, mid_y) = edge_midpoint(edge);
        let mut offset = 0.0;
        let mut placed = None;

        for _ in 0..6 {
            let x = mid_x;
            let y = mid_y + offset;
            let rect = (
                x - label.width / 2.0 - 6.0,
                y - label.height / 2.0 - 4.0,
                label.width + 12.0,
                label.height + 8.0,
            );
            if !collides(&rect, &occupied) {
                occupied.push(rect);
                placed = Some((x, y, label.clone()));
                break;
            }
            offset += label.height + 6.0;
        }

        if placed.is_none() {
            placed = Some((mid_x, mid_y, label));
        }

        positions.insert(idx, placed);
    }

    positions
}

fn edge_midpoint(edge: &EdgeLayout) -> (f32, f32) {
    if edge.points.len() >= 4 {
        let p1 = edge.points[1];
        let p2 = edge.points[2];
        ((p1.0 + p2.0) / 2.0, (p1.1 + p2.1) / 2.0)
    } else if edge.points.len() >= 2 {
        let p1 = edge.points[0];
        let p2 = edge.points[edge.points.len() - 1];
        ((p1.0 + p2.0) / 2.0, (p1.1 + p2.1) / 2.0)
    } else {
        (0.0, 0.0)
    }
}

fn collides(rect: &(f32, f32, f32, f32), occupied: &[(f32, f32, f32, f32)]) -> bool {
    for (x, y, w, h) in occupied {
        if rect.0 < x + w
            && rect.0 + rect.2 > *x
            && rect.1 < y + h
            && rect.1 + rect.3 > *y
        {
            return true;
        }
    }
    false
}

pub fn write_output_svg(svg: &str, output: Option<&Path>) -> Result<()> {
    match output {
        Some(path) => {
            std::fs::write(path, svg)?;
        }
        None => {
            print!("{}", svg);
        }
    }
    Ok(())
}

pub fn write_output_png(svg: &str, output: &Path, render_cfg: &RenderConfig) -> Result<()> {
    let mut opt = usvg::Options::default();
    opt.font_family = "Inter".to_string();
    opt.default_size = usvg::Size::from_wh(render_cfg.width, render_cfg.height)
        .unwrap_or(usvg::Size::from_wh(800.0, 600.0).unwrap());

    let tree = usvg::Tree::from_str(svg, &opt)?;
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| anyhow::anyhow!("Failed to allocate pixmap"))?;

    let mut pixmap_mut = pixmap.as_mut();
    resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap_mut);
    pixmap.save_png(output)?;
    Ok(())
}

fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LayoutConfig;
    use crate::ir::{Direction, Graph};
    use crate::layout::compute_layout;

    #[test]
    fn render_svg_basic() {
        let mut graph = Graph::new();
        graph.direction = Direction::LeftRight;
        graph.ensure_node("A", Some("Alpha".to_string()));
        graph.ensure_node("B", Some("Beta".to_string()));
        graph.edges.push(crate::ir::Edge {
            from: "A".to_string(),
            to: "B".to_string(),
            label: Some("go".to_string()),
            directed: true,
        });
        let layout = compute_layout(&graph, &Theme::modern(), &LayoutConfig::default());
        let svg = render_svg(&layout, &Theme::modern(), &LayoutConfig::default());
        assert!(svg.contains("<svg"));
        assert!(svg.contains("Alpha"));
    }
}
