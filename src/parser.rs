use crate::ir::{Direction, Graph, Subgraph};
use anyhow::Result;
use regex::Regex;

#[derive(Debug, Default)]
pub struct ParseOutput {
    pub graph: Graph,
    pub init_config: Option<serde_json::Value>,
}

pub fn parse_mermaid(input: &str) -> Result<ParseOutput> {
    let mut graph = Graph::new();
    let mut current_subgraph: Option<usize> = None;
    let mut init_config: Option<serde_json::Value> = None;

    let header_re = Regex::new(r"^(flowchart|graph)\s+(\w+)")?;
    let subgraph_re = Regex::new(r"^subgraph\s+(.*)$")?;
    let init_re = Regex::new(r"^%%\{\s*init\s*:\s*(\{.*\})\s*\}%%")?;

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(caps) = init_re.captures(line) {
            if let Some(json_str) = caps.get(1).map(|m| m.as_str()) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) {
                    init_config = Some(value);
                }
            }
            continue;
        }

        if line.starts_with("%%") {
            continue;
        }

        if let Some(caps) = header_re.captures(line) {
            if let Some(dir) = caps.get(2).and_then(|m| Direction::from_token(m.as_str())) {
                graph.direction = dir;
            }
            continue;
        }

        if line == "end" {
            current_subgraph = None;
            continue;
        }

        if let Some(caps) = subgraph_re.captures(line) {
            let rest = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let (id, label) = parse_subgraph_header(rest);
            graph.subgraphs.push(Subgraph {
                id,
                label,
                nodes: Vec::new(),
            });
            current_subgraph = Some(graph.subgraphs.len() - 1);
            continue;
        }

        if let Some((left, label, right, directed)) = parse_edge_line(line) {
            let (left_id, left_label) = parse_node_token(&left);
            let (right_id, right_label) = parse_node_token(&right);
            graph.ensure_node(&left_id, left_label);
            graph.ensure_node(&right_id, right_label);
            graph.edges.push(crate::ir::Edge {
                from: left_id.clone(),
                to: right_id.clone(),
                label,
                directed,
            });
            if let Some(idx) = current_subgraph {
                add_node_to_subgraph(&mut graph, idx, &left_id);
                add_node_to_subgraph(&mut graph, idx, &right_id);
            }
            continue;
        }

        if let Some((node_id, node_label)) = parse_node_only(line) {
            graph.ensure_node(&node_id, node_label);
            if let Some(idx) = current_subgraph {
                add_node_to_subgraph(&mut graph, idx, &node_id);
            }
        }
    }

    Ok(ParseOutput { graph, init_config })
}

fn add_node_to_subgraph(graph: &mut Graph, idx: usize, node_id: &str) {
    if let Some(subgraph) = graph.subgraphs.get_mut(idx) {
        if !subgraph.nodes.contains(&node_id.to_string()) {
            subgraph.nodes.push(node_id.to_string());
        }
    }
}

fn parse_subgraph_header(input: &str) -> (Option<String>, String) {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return (None, "Subgraph".to_string());
    }

    if let Some((id, label)) = split_id_label(trimmed) {
        return (Some(id.to_string()), label);
    }

    (None, strip_quotes(trimmed))
}

fn parse_node_only(line: &str) -> Option<(String, Option<String>)> {
    if line.contains("--") {
        return None;
    }
    let (id, label) = parse_node_token(line);
    if id.is_empty() {
        None
    } else {
        Some((id, label))
    }
}

fn parse_edge_line(line: &str) -> Option<(String, Option<String>, String, bool)> {
    let (left, arrow, right) = if let Some(pos) = line.find("-->") {
        (line[..pos].trim(), "-->", line[pos + 3..].trim())
    } else if let Some(pos) = line.find("---") {
        (line[..pos].trim(), "---", line[pos + 3..].trim())
    } else {
        return None;
    };

    if left.is_empty() || right.is_empty() || arrow.is_empty() {
        return None;
    }

    let (label, right_token) = if right.starts_with('|') {
        if let Some(end) = right[1..].find('|') {
            let label = right[1..=end].trim_matches('|').trim().to_string();
            let rest = right[end + 2..].trim();
            (Some(label), rest)
        } else {
            (None, right)
        }
    } else {
        (None, right)
    };

    if right_token.is_empty() {
        return None;
    }

    let directed = arrow == "-->";
    Some((left.to_string(), label, right_token.to_string(), directed))
}

fn parse_node_token(token: &str) -> (String, Option<String>) {
    let trimmed = token.trim();
    if let Some((id, label)) = split_id_label(trimmed) {
        return (id.to_string(), Some(label));
    }

    let id = trimmed
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    (id, None)
}

fn split_id_label(token: &str) -> Option<(&str, String)> {
    let bracket_re = Regex::new(r"^([A-Za-z0-9_\-]+)\s*\[(.*)\]$").ok()?;
    if let Some(caps) = bracket_re.captures(token) {
        let id = caps.get(1)?.as_str();
        let label = strip_quotes(caps.get(2)?.as_str());
        return Some((id, label));
    }

    let paren_re = Regex::new(r"^([A-Za-z0-9_\-]+)\s*\((.*)\)$").ok()?;
    if let Some(caps) = paren_re.captures(token) {
        let id = caps.get(1)?.as_str();
        let label = strip_quotes(caps.get(2)?.as_str());
        return Some((id, label));
    }

    None
}

fn strip_quotes(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_flowchart() {
        let input = "flowchart LR\nA[Start] -->|go| B(End)";
        let parsed = parse_mermaid(input).unwrap();
        assert_eq!(parsed.graph.nodes.len(), 2);
        assert_eq!(parsed.graph.edges.len(), 1);
        assert_eq!(parsed.graph.edges[0].label.as_deref(), Some("go"));
        assert_eq!(parsed.graph.direction, Direction::LeftRight);
    }

    #[test]
    fn parse_subgraph() {
        let input = "flowchart TD\nsubgraph Group[\"My Group\"]\nA --> B\nend";
        let parsed = parse_mermaid(input).unwrap();
        assert_eq!(parsed.graph.subgraphs.len(), 1);
        let sg = &parsed.graph.subgraphs[0];
        assert_eq!(sg.label, "My Group");
        assert_eq!(sg.nodes.len(), 2);
    }
}
