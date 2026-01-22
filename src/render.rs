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

    let mut colors = Vec::new();
    colors.push(theme.line_color.clone());
    for edge in &layout.edges {
        if let Some(color) = &edge.override_style.stroke {
            if !colors.contains(color) {
                colors.push(color.clone());
            }
        }
    }
    let mut color_ids: HashMap<String, usize> = HashMap::new();
    for (idx, color) in colors.iter().enumerate() {
        color_ids.insert(color.clone(), idx);
    }

    svg.push_str("<defs>");
    for color in &colors {
        let idx = color_ids.get(color).copied().unwrap_or(0);
        svg.push_str(&format!(
            "<marker id=\"arrow-{idx}\" viewBox=\"0 0 10 10\" refX=\"10\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\"><path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"{}\"/></marker>",
            color
        ));
        svg.push_str(&format!(
            "<marker id=\"arrow-start-{idx}\" viewBox=\"0 0 10 10\" refX=\"0\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto\"><path d=\"M 10 0 L 0 5 L 10 10 z\" fill=\"{}\"/></marker>",
            color
        ));
    }
    svg.push_str("</defs>");

    for subgraph in &layout.subgraphs {
        let sub_fill = subgraph
            .style
            .fill
            .as_ref()
            .unwrap_or(&theme.cluster_background);
        let sub_stroke = subgraph
            .style
            .stroke
            .as_ref()
            .unwrap_or(&theme.cluster_border);
        let sub_dash = subgraph
            .style
            .stroke_dasharray
            .as_ref()
            .map(|value| format!(" stroke-dasharray=\"{}\"", value))
            .unwrap_or_else(|| " stroke-dasharray=\"6 4\"".to_string());
        let sub_stroke_width = subgraph.style.stroke_width.unwrap_or(1.2);
        svg.push_str(&format!(
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"10\" ry=\"10\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{} />",
            subgraph.x,
            subgraph.y,
            subgraph.width,
            subgraph.height,
            sub_fill,
            sub_stroke,
            sub_stroke_width,
            sub_dash
        ));
        let label_x = subgraph.x + 12.0;
        let label_y = subgraph.y + 6.0;
        let label_color = subgraph
            .style
            .text_color
            .as_ref()
            .unwrap_or(&theme.primary_text_color);
        svg.push_str(&text_block_svg_left(
            label_x,
            label_y,
            &subgraph.label_block,
            theme,
            config,
            label_color,
        ));
    }

    let label_positions = compute_edge_label_positions(&layout.edges, &layout.nodes, &layout.subgraphs);

    for (idx, edge) in layout.edges.iter().enumerate() {
        let d = points_to_path(&edge.points);
        let mut stroke = theme.line_color.clone();
        let (mut dash, mut stroke_width) = match edge.style {
            crate::ir::EdgeStyle::Solid => (String::new(), 1.4),
            crate::ir::EdgeStyle::Dotted => ("stroke-dasharray=\"3 5\"".to_string(), 1.2),
            crate::ir::EdgeStyle::Thick => (String::new(), 2.6),
        };

        if let Some(color) = &edge.override_style.stroke {
            stroke = color.clone();
        }
        let marker_id = color_ids.get(&stroke).copied().unwrap_or(0);
        let marker_end = if edge.arrow_end {
            format!("marker-end=\"url(#arrow-{marker_id})\"")
        } else {
            String::new()
        };
        let marker_start = if edge.arrow_start {
            format!("marker-start=\"url(#arrow-start-{marker_id})\"")
        } else {
            String::new()
        };
        if let Some(width) = edge.override_style.stroke_width {
            stroke_width = width;
        }
        if let Some(dash_override) = &edge.override_style.dasharray {
            dash = format!("stroke-dasharray=\"{}\"", dash_override);
        }
        svg.push_str(&format!(
            "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" {} {} {} />",
            d,
            stroke,
            stroke_width,
            marker_end,
            marker_start,
            dash
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
            svg.push_str(&text_block_svg(x, y, &label, theme, config, true, None));
        }
    }

    for node in layout.nodes.values() {
        svg.push_str(&shape_svg(node, theme));
        let center_x = node.x + node.width / 2.0;
        let center_y = node.y + node.height / 2.0;
        svg.push_str(&text_block_svg(center_x, center_y, &node.label, theme, config, false, node.style.text_color.as_deref()));
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

fn text_block_svg(
    x: f32,
    y: f32,
    label: &TextBlock,
    theme: &Theme,
    config: &LayoutConfig,
    edge: bool,
    override_color: Option<&str>,
) -> String {
    let total_height = label.lines.len() as f32 * theme.font_size * config.label_line_height;
    let start_y = y - total_height / 2.0 + theme.font_size;
    let mut text = String::new();
    let anchor = "middle";
    let default_fill = if edge {
        theme.primary_text_color.as_str()
    } else {
        theme.primary_text_color.as_str()
    };
    let fill = override_color.unwrap_or(default_fill);

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

fn text_block_svg_left(
    x: f32,
    y: f32,
    label: &TextBlock,
    theme: &Theme,
    config: &LayoutConfig,
    fill: &str,
) -> String {
    let start_y = y + theme.font_size;
    let mut text = String::new();
    text.push_str(&format!(
        "<text x=\"{x:.2}\" y=\"{start_y:.2}\" text-anchor=\"start\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\">",
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
    nodes: &std::collections::BTreeMap<String, crate::layout::NodeLayout>,
    subgraphs: &[crate::layout::SubgraphLayout],
) -> HashMap<usize, Option<(f32, f32, TextBlock)>> {
    let mut occupied: Vec<(f32, f32, f32, f32)> = Vec::new();
    let edge_obstacles = build_edge_obstacles(edges, 6.0);

    for node in nodes.values() {
        occupied.push((
            node.x - 6.0,
            node.y - 6.0,
            node.width + 12.0,
            node.height + 12.0,
        ));
    }

    for sub in subgraphs {
        if sub.label.trim().is_empty() {
            continue;
        }
        let width = sub.label_block.width;
        let height = sub.label_block.height;
        let x = sub.x + 12.0;
        let y = sub.y + 6.0;
        occupied.push((x - 4.0, y, width + 8.0, height + 4.0));
    }
    let mut positions = HashMap::new();

    for (idx, edge) in edges.iter().enumerate() {
        let Some(label) = edge.label.clone() else {
            positions.insert(idx, None);
            continue;
        };
        let (mid_x, mid_y, offset_axis) = edge_label_anchor(edge);
        let mut placed = None;

        let step = match offset_axis {
            OffsetAxis::X => (label.width / 2.0 + 12.0).max(18.0),
            OffsetAxis::Y => (label.height + 8.0).max(18.0),
        };

        for attempt in 0..6 {
            let dir = if attempt % 2 == 0 { 1.0 } else { -1.0 };
            let offset = if attempt == 0 {
                0.0
            } else {
                let step_mul = ((attempt + 1) / 2) as f32;
                dir * step_mul * step
            };

            let (x, y) = match offset_axis {
                OffsetAxis::X => (mid_x + offset, mid_y),
                OffsetAxis::Y => (mid_x, mid_y + offset),
            };
            let rect = (
                x - label.width / 2.0 - 6.0,
                y - label.height / 2.0 - 4.0,
                label.width + 12.0,
                label.height + 8.0,
            );
            if !collides(&rect, &occupied) && !collides_edges(&rect, &edge_obstacles, idx) {
                occupied.push(rect);
                placed = Some((x, y, label.clone()));
                break;
            }
        }

        if placed.is_none() {
            placed = Some((mid_x, mid_y, label));
        }

        positions.insert(idx, placed);
    }

    positions
}

#[derive(Debug, Clone, Copy)]
enum OffsetAxis {
    X,
    Y,
}

fn edge_label_anchor(edge: &EdgeLayout) -> (f32, f32, OffsetAxis) {
    if edge.points.len() >= 4 {
        let p1 = edge.points[1];
        let p2 = edge.points[2];
        let dx = (p2.0 - p1.0).abs();
        let dy = (p2.1 - p1.1).abs();
        let axis = if dx > dy { OffsetAxis::Y } else { OffsetAxis::X };
        return ((p1.0 + p2.0) / 2.0, (p1.1 + p2.1) / 2.0, axis);
    }
    if edge.points.len() >= 2 {
        let p1 = edge.points[0];
        let p2 = edge.points[edge.points.len() - 1];
        let dx = (p2.0 - p1.0).abs();
        let dy = (p2.1 - p1.1).abs();
        let axis = if dx > dy { OffsetAxis::Y } else { OffsetAxis::X };
        return ((p1.0 + p2.0) / 2.0, (p1.1 + p2.1) / 2.0, axis);
    }
    (0.0, 0.0, OffsetAxis::Y)
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

fn build_edge_obstacles(edges: &[EdgeLayout], pad: f32) -> Vec<(usize, (f32, f32, f32, f32))> {
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

fn collides_edges(
    rect: &(f32, f32, f32, f32),
    obstacles: &[(usize, (f32, f32, f32, f32))],
    edge_idx: usize,
) -> bool {
    for (idx, (x, y, w, h)) in obstacles {
        if *idx == edge_idx {
            continue;
        }
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

pub fn write_output_png(
    svg: &str,
    output: &Path,
    render_cfg: &RenderConfig,
    theme: &Theme,
) -> Result<()> {
    let mut opt = usvg::Options::default();
    opt.font_family = primary_font(&theme.font_family);
    opt.default_size = usvg::Size::from_wh(render_cfg.width, render_cfg.height)
        .unwrap_or(usvg::Size::from_wh(800.0, 600.0).unwrap());

    opt.fontdb_mut().load_system_fonts();

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

fn primary_font(fonts: &str) -> String {
    fonts
        .split(',')
        .map(|s| s.trim().trim_matches('"'))
        .find(|s| !s.is_empty())
        .unwrap_or("Inter")
        .to_string()
}

fn shape_svg(node: &crate::layout::NodeLayout, theme: &Theme) -> String {
    let stroke = node
        .style
        .stroke
        .as_ref()
        .unwrap_or(&theme.primary_border_color);
    let fill = node
        .style
        .fill
        .as_ref()
        .unwrap_or(&theme.primary_color);
    let dash = node
        .style
        .stroke_dasharray
        .as_ref()
        .map(|value| format!(" stroke-dasharray=\"{}\"", value))
        .unwrap_or_default();
    let x = node.x;
    let y = node.y;
    let w = node.width;
    let h = node.height;
    match node.shape {
        crate::ir::NodeShape::Diamond => {
            let cx = x + w / 2.0;
            let cy = y + h / 2.0;
            let points = format!(
                "{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2}",
                cx,
                y,
                x + w,
                cy,
                cx,
                y + h,
                x,
                cy
            );
            format!(
                "<polygon points=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{dash}/>",
                points,
                fill,
                stroke,
                node.style.stroke_width.unwrap_or(1.4)
            )
        }
        crate::ir::NodeShape::Circle | crate::ir::NodeShape::DoubleCircle => {
            let cx = x + w / 2.0;
            let cy = y + h / 2.0;
            let r = (w.min(h)) / 2.0;
            let mut svg = format!(
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{dash}/>",
                cx, cy, r, fill, stroke, node.style.stroke_width.unwrap_or(1.4)
            );
            if node.shape == crate::ir::NodeShape::DoubleCircle {
                let r2 = r - 4.0;
                if r2 > 0.0 {
                    svg.push_str(&format!(
                        "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.0\"/>",
                        cx, cy, r2, stroke
                    ));
                }
            }
            svg
        }
        crate::ir::NodeShape::Stadium => format!(
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{dash}/>",
            x,
            y,
            w,
            h,
            h / 2.0,
            h / 2.0,
            fill,
            stroke,
            node.style.stroke_width.unwrap_or(1.4)
        ),
        crate::ir::NodeShape::RoundRect => format!(
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"10\" ry=\"10\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{dash}/>",
            x,
            y,
            w,
            h,
            fill,
            stroke,
            node.style.stroke_width.unwrap_or(1.4)
        ),
        crate::ir::NodeShape::Cylinder => format!(
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"12\" ry=\"12\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{dash}/>",
            x,
            y,
            w,
            h,
            fill,
            stroke,
            node.style.stroke_width.unwrap_or(1.4)
        ),
        crate::ir::NodeShape::Hexagon => {
            let x1 = x + w * 0.25;
            let x2 = x + w * 0.75;
            let y_mid = y + h / 2.0;
            let points = format!(
                "{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2}",
                x1,
                y,
                x2,
                y,
                x + w,
                y_mid,
                x2,
                y + h,
                x1,
                y + h,
                x,
                y_mid
            );
            format!(
                "<polygon points=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{dash}/>",
                points,
                fill,
                stroke,
                node.style.stroke_width.unwrap_or(1.4)
            )
        }
        crate::ir::NodeShape::Parallelogram | crate::ir::NodeShape::ParallelogramAlt => {
            let offset = w * 0.18;
            let (p1, p2, p3, p4) = if node.shape == crate::ir::NodeShape::Parallelogram {
                (
                    (x + offset, y),
                    (x + w, y),
                    (x + w - offset, y + h),
                    (x, y + h),
                )
            } else {
                (
                    (x, y),
                    (x + w - offset, y),
                    (x + w, y + h),
                    (x + offset, y + h),
                )
            };
            let points = format!(
                "{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2}",
                p1.0, p1.1, p2.0, p2.1, p3.0, p3.1, p4.0, p4.1
            );
            format!(
                "<polygon points=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{dash}/>",
                points,
                fill,
                stroke,
                node.style.stroke_width.unwrap_or(1.4)
            )
        }
        crate::ir::NodeShape::Trapezoid | crate::ir::NodeShape::TrapezoidAlt => {
            let offset = w * 0.18;
            let (p1, p2, p3, p4) = if node.shape == crate::ir::NodeShape::Trapezoid {
                (
                    (x + offset, y),
                    (x + w - offset, y),
                    (x + w, y + h),
                    (x, y + h),
                )
            } else {
                (
                    (x, y),
                    (x + w, y),
                    (x + w - offset, y + h),
                    (x + offset, y + h),
                )
            };
            let points = format!(
                "{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2}",
                p1.0, p1.1, p2.0, p2.1, p3.0, p3.1, p4.0, p4.1
            );
            format!(
                "<polygon points=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{dash}/>",
                points,
                fill,
                stroke,
                node.style.stroke_width.unwrap_or(1.4)
            )
        }
        _ => format!(
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"6\" ry=\"6\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{dash}/>",
            x,
            y,
            w,
            h,
            fill,
            stroke,
            node.style.stroke_width.unwrap_or(1.4)
        ),
    }
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
        graph.ensure_node("A", Some("Alpha".to_string()), Some(crate::ir::NodeShape::Rectangle));
        graph.ensure_node("B", Some("Beta".to_string()), Some(crate::ir::NodeShape::Rectangle));
        graph.edges.push(crate::ir::Edge {
            from: "A".to_string(),
            to: "B".to_string(),
            label: Some("go".to_string()),
            directed: true,
            arrow_start: false,
            arrow_end: true,
            style: crate::ir::EdgeStyle::Solid,
        });
        let layout = compute_layout(&graph, &Theme::modern(), &LayoutConfig::default());
        let svg = render_svg(&layout, &Theme::modern(), &LayoutConfig::default());
        assert!(svg.contains("<svg"));
        assert!(svg.contains("Alpha"));
    }
}
