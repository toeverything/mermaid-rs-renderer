use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use crate::ir::Graph;

pub(super) fn rank_edges_for_manual_layout(
    graph: &Graph,
    layout_node_ids: &[String],
    layout_edges: &[crate::ir::Edge],
) -> Vec<crate::ir::Edge> {
    if graph.kind != crate::ir::DiagramKind::Flowchart || layout_edges.len() < 3 {
        return layout_edges.to_vec();
    }

    let primary: Vec<crate::ir::Edge> = layout_edges
        .iter()
        .filter(|edge| edge.style != crate::ir::EdgeStyle::Dotted)
        .cloned()
        .collect();
    if primary.is_empty() {
        return layout_edges.to_vec();
    }

    let mut covered: HashSet<&str> = HashSet::new();
    for edge in &primary {
        covered.insert(edge.from.as_str());
        covered.insert(edge.to.as_str());
    }
    let min_covered = layout_node_ids.len().div_ceil(2);
    if covered.len() >= min_covered {
        return primary;
    }

    layout_edges.to_vec()
}

pub(super) fn order_rank_nodes(
    rank_nodes: &mut [Vec<String>],
    edges: &[crate::ir::Edge],
    node_order: &HashMap<String, usize>,
    passes: usize,
) {
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
            let a_score = median_position(a, neighbors, positions, &current_positions);
            let b_score = median_position(b, neighbors, positions, &current_positions);
            match a_score.partial_cmp(&b_score) {
                Some(std::cmp::Ordering::Equal) | None => {
                    let a_pos = current_positions.get(a).copied().unwrap_or(0);
                    let b_pos = current_positions.get(b).copied().unwrap_or(0);
                    match a_pos.cmp(&b_pos) {
                        std::cmp::Ordering::Equal => node_order
                            .get(a)
                            .copied()
                            .unwrap_or(usize::MAX)
                            .cmp(&node_order.get(b).copied().unwrap_or(usize::MAX)),
                        other => other,
                    }
                }
                Some(ordering) => ordering,
            }
        });
    };

    let passes = passes.max(1);
    for _ in 0..passes {
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

pub(super) fn median_position(
    node_id: &str,
    neighbors: &HashMap<String, Vec<String>>,
    positions: &HashMap<String, usize>,
    current_positions: &HashMap<String, usize>,
) -> f32 {
    let Some(list) = neighbors.get(node_id) else {
        return *current_positions.get(node_id).unwrap_or(&0) as f32;
    };
    let mut values = Vec::new();
    for neighbor in list {
        if let Some(pos) = positions.get(neighbor) {
            values.push(*pos as f32);
        }
    }
    if values.is_empty() {
        return *current_positions.get(node_id).unwrap_or(&0) as f32;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) * 0.5
    }
}

pub(super) fn compute_ranks_subset(
    node_ids: &[String],
    edges: &[crate::ir::Edge],
    node_order: &HashMap<String, usize>,
) -> HashMap<String, usize> {
    let set: HashSet<String> = node_ids.iter().cloned().collect();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut rev: HashMap<String, Vec<String>> = HashMap::new();

    for edge in edges {
        if set.contains(&edge.from) && set.contains(&edge.to) {
            adj.entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
            rev.entry(edge.to.clone())
                .or_default()
                .push(edge.from.clone());
        }
    }

    let mut fallback_order: HashMap<&str, usize> = HashMap::new();
    for (idx, id) in node_ids.iter().enumerate() {
        fallback_order.insert(id.as_str(), idx);
    }
    let order_key = |id: &str| -> usize {
        node_order
            .get(id)
            .copied()
            .unwrap_or_else(|| fallback_order.get(id).copied().unwrap_or(usize::MAX))
    };

    let mut indeg: HashMap<String, usize> = HashMap::new();
    for id in &set {
        let count = rev.get(id).map(|v| v.len()).unwrap_or(0);
        indeg.insert(id.clone(), count);
    }

    let mut ready: BinaryHeap<Reverse<(usize, String)>> = BinaryHeap::new();
    for id in &set {
        if *indeg.get(id).unwrap_or(&0) == 0 {
            ready.push(Reverse((order_key(id.as_str()), id.clone())));
        }
    }

    let mut order = Vec::with_capacity(set.len());
    let mut processed: HashSet<String> = HashSet::new();
    loop {
        while let Some(Reverse((_key, id))) = ready.pop() {
            if processed.contains(&id) {
                continue;
            }
            order.push(id.clone());
            processed.insert(id.clone());
            if let Some(nexts) = adj.get(&id) {
                for next in nexts {
                    if processed.contains(next) {
                        continue;
                    }
                    if let Some(deg) = indeg.get_mut(next) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            ready.push(Reverse((order_key(next.as_str()), next.clone())));
                        }
                    }
                }
            }
        }

        if processed.len() >= set.len() {
            break;
        }

        // Cycle detected — pick the remaining node earliest in declaration
        // order as the next source, treating its incoming edges as back-edges.
        let mut best: Option<(usize, String)> = None;
        for id in &set {
            if !processed.contains(id) {
                let key = order_key(id.as_str());
                if best.as_ref().map_or(true, |(bk, _)| key < *bk) {
                    best = Some((key, id.clone()));
                }
            }
        }
        if let Some((key, id)) = best {
            ready.push(Reverse((key, id)));
        } else {
            break;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn edge(from: &str, to: &str) -> crate::ir::Edge {
        crate::ir::Edge {
            from: from.to_string(),
            to: to.to_string(),
            label: None,
            start_label: None,
            end_label: None,
            directed: true,
            arrow_start: false,
            arrow_end: true,
            arrow_start_kind: None,
            arrow_end_kind: None,
            start_decoration: None,
            end_decoration: None,
            style: crate::ir::EdgeStyle::Solid,
        }
    }

    #[test]
    fn compute_ranks_linear_chain() {
        let nodes = vec!["A".into(), "B".into(), "C".into()];
        let edges = vec![edge("A", "B"), edge("B", "C")];
        let ranks = compute_ranks_subset(&nodes, &edges, &HashMap::new());
        assert_eq!(ranks["A"], 0);
        assert_eq!(ranks["B"], 1);
        assert_eq!(ranks["C"], 2);
    }

    #[test]
    fn compute_ranks_diamond() {
        let nodes = vec!["A".into(), "B".into(), "C".into(), "D".into()];
        let edges = vec![edge("A", "B"), edge("A", "C"), edge("B", "D"), edge("C", "D")];
        let ranks = compute_ranks_subset(&nodes, &edges, &HashMap::new());
        assert_eq!(ranks["A"], 0);
        assert_eq!(ranks["B"], 1);
        assert_eq!(ranks["C"], 1);
        assert_eq!(ranks["D"], 2);
    }

    #[test]
    fn compute_ranks_handles_cycle() {
        let nodes = vec!["A".into(), "B".into(), "C".into()];
        let edges = vec![edge("A", "B"), edge("B", "C"), edge("C", "A")];
        let ranks = compute_ranks_subset(&nodes, &edges, &HashMap::new());
        // All nodes should get a rank (cycle doesn't cause infinite loop)
        assert_eq!(ranks.len(), 3);
    }

    #[test]
    fn compute_ranks_disconnected_nodes() {
        let nodes = vec!["A".into(), "B".into(), "C".into()];
        let edges = vec![edge("A", "B")];
        let ranks = compute_ranks_subset(&nodes, &edges, &HashMap::new());
        assert_eq!(ranks["A"], 0);
        assert_eq!(ranks["B"], 1);
        assert_eq!(ranks["C"], 0); // disconnected → rank 0
    }

    #[test]
    fn median_position_with_no_neighbors() {
        let neighbors: HashMap<String, Vec<String>> = HashMap::new();
        let positions: HashMap<String, usize> = HashMap::new();
        let current: HashMap<String, usize> = [("X".to_string(), 3)].into();
        assert_eq!(median_position("X", &neighbors, &positions, &current), 3.0);
    }

    #[test]
    fn median_position_odd_count() {
        let neighbors: HashMap<String, Vec<String>> =
            [("X".to_string(), vec!["A".into(), "B".into(), "C".into()])].into();
        let positions: HashMap<String, usize> =
            [("A".into(), 1), ("B".into(), 5), ("C".into(), 9)].into();
        let current: HashMap<String, usize> = [("X".to_string(), 0)].into();
        assert_eq!(median_position("X", &neighbors, &positions, &current), 5.0);
    }

    #[test]
    fn order_rank_nodes_reduces_crossings() {
        // A→D, B→E, C→F — rank1 starts in wrong order [F,D,E]
        // median-based ordering should move D before E before F
        let edges = vec![edge("A", "D"), edge("B", "E"), edge("C", "F")];
        let mut rank_nodes = vec![
            vec!["A".into(), "B".into(), "C".into()],
            vec!["F".into(), "D".into(), "E".into()],
        ];
        order_rank_nodes(&mut rank_nodes, &edges, &HashMap::new(), 3);
        // D should end up before E which should be before F
        let pos_d = rank_nodes[1].iter().position(|n| n == "D").unwrap();
        let pos_e = rank_nodes[1].iter().position(|n| n == "E").unwrap();
        let pos_f = rank_nodes[1].iter().position(|n| n == "F").unwrap();
        assert!(pos_d < pos_e, "D should precede E, got {:?}", rank_nodes[1]);
        assert!(pos_e < pos_f, "E should precede F, got {:?}", rank_nodes[1]);
    }
}
