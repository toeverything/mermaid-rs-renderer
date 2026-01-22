use crate::config::LayoutConfig;
use crate::ir::{Direction, Graph};
use crate::theme::Theme;
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
}

#[derive(Debug, Clone)]
pub struct EdgeLayout {
    pub from: String,
    pub to: String,
    pub label: Option<TextBlock>,
    pub points: Vec<(f32, f32)>,
    pub directed: bool,
}

#[derive(Debug, Clone)]
pub struct SubgraphLayout {
    pub label: String,
    pub nodes: Vec<String>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub nodes: BTreeMap<String, NodeLayout>,
    pub edges: Vec<EdgeLayout>,
    pub subgraphs: Vec<SubgraphLayout>,
    pub width: f32,
    pub height: f32,
}

pub fn compute_layout(graph: &Graph, theme: &Theme, config: &LayoutConfig) -> Layout {
    let mut nodes = BTreeMap::new();

    for node in graph.nodes.values() {
        let label = measure_label(&node.label, theme, config);
        let width = label.width + config.node_padding_x * 2.0;
        let height = label.height + config.node_padding_y * 2.0;
        nodes.insert(
            node.id.clone(),
            NodeLayout {
                id: node.id.clone(),
                x: 0.0,
                y: 0.0,
                width,
                height,
                label,
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

    let mut main_cursor = 0.0;
    let mut max_cross: f32 = 0.0;

    for (rank_idx, bucket) in rank_nodes.iter().enumerate() {
        let mut cross_cursor = 0.0;
        let mut max_main = 0.0;

        for node_id in bucket {
            if let Some(node_layout) = nodes.get_mut(node_id) {
                if graph.direction == Direction::LeftRight {
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
        max_cross = max_cross.max(cross_cursor);

        if rank_idx == max_rank {
            // Ensure no trailing spacing
        }
    }

    let mut edges = Vec::new();
    for edge in &graph.edges {
        let from = nodes.get(&edge.from).expect("from node missing");
        let to = nodes.get(&edge.to).expect("to node missing");
        let label = edge
            .label
            .as_ref()
            .map(|l| measure_label(l, theme, config));

        let points = route_edge(from, to, graph.direction);
        edges.push(EdgeLayout {
            from: edge.from.clone(),
            to: edge.to.clone(),
            label,
            points,
            directed: edge.directed,
        });
    }

    let mut subgraphs = Vec::new();
    for sub in &graph.subgraphs {
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

        if min_x != f32::MAX {
            let padding = 24.0;
            subgraphs.push(SubgraphLayout {
                label: sub.label.clone(),
                nodes: sub.nodes.clone(),
                x: min_x - padding,
                y: min_y - padding,
                width: (max_x - min_x) + padding * 2.0,
                height: (max_y - min_y) + padding * 2.0,
            });
        }
    }

    let width = if graph.direction == Direction::LeftRight {
        main_cursor + 60.0
    } else {
        max_cross + 60.0
    };

    let height = if graph.direction == Direction::LeftRight {
        max_cross + 60.0
    } else {
        main_cursor + 60.0
    };

    Layout {
        nodes,
        edges,
        subgraphs,
        width,
        height,
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

    let mut ranks: HashMap<String, usize> = HashMap::new();
    for node in &order {
        let rank = *ranks.get(node).unwrap_or(&0);
        if let Some(nexts) = adj.get(node) {
            for next in nexts {
                let entry = ranks.entry(next.clone()).or_insert(0);
                *entry = (*entry).max(rank + 1);
            }
        }
    }

    ranks
}

fn route_edge(from: &NodeLayout, to: &NodeLayout, direction: Direction) -> Vec<(f32, f32)> {
    if direction == Direction::LeftRight {
        let start = (from.x + from.width, from.y + from.height / 2.0);
        let end = (to.x, to.y + to.height / 2.0);
        let mid_x = (start.0 + end.0) / 2.0;
        vec![start, (mid_x, start.1), (mid_x, end.1), end]
    } else {
        let start = (from.x + from.width / 2.0, from.y + from.height);
        let end = (to.x + to.width / 2.0, to.y);
        let mid_y = (start.1 + end.1) / 2.0;
        vec![start, (start.0, mid_y), (end.0, mid_y), end]
    }
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

    TextBlock { lines, width, height }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Direction, Graph};

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
        graph.ensure_node("A", Some("Alpha".to_string()));
        graph.ensure_node("B", Some("Beta".to_string()));
        graph.edges.push(crate::ir::Edge {
            from: "A".to_string(),
            to: "B".to_string(),
            label: None,
            directed: true,
        });
        let layout = compute_layout(&graph, &Theme::modern(), &LayoutConfig::default());
        let a = layout.nodes.get("A").unwrap();
        let b = layout.nodes.get("B").unwrap();
        assert!(b.x >= a.x);
    }
}
