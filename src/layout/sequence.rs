use super::*;

pub(super) fn compute_sequence_layout(graph: &Graph, theme: &Theme, config: &LayoutConfig) -> Layout {
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

    let participant_count = participants.len();
    let base_actor_width = max_label_width + theme.font_size * 1.2;
    let min_actor_width = (theme.font_size * 4.0).max(80.0);
    let actor_width = base_actor_width.max(min_actor_width);
    let actor_height = (max_label_height + theme.font_size * 1.6).max(48.0);
    let mut actor_gap = (theme.font_size * 1.4).max(16.0);
    if participant_count >= 7 {
        actor_gap *= 0.75;
    } else if participant_count >= 5 {
        actor_gap *= 0.82;
    }

    // Add consistent margins to center the diagram
    let margin = 8.0;
    let mut cursor_x = margin;
    for id in &participants {
        let node = graph.nodes.get(id).expect("participant missing");
        let label = label_blocks.get(id).cloned().unwrap_or_else(|| TextBlock {
            lines: vec![id.clone()],
            width: 0.0,
            height: 0.0,
        });
        nodes.insert(
            id.clone(),
            NodeLayout {
                id: id.clone(),
                x: cursor_x,
                y: margin,
                width: actor_width,
                height: actor_height,
                label,
                shape: node.shape,
                style: resolve_node_style(id.as_str(), graph),
                link: graph.node_links.get(id).cloned(),
                anchor_subgraph: None,
                hidden: false,
                icon: None,
            },
        );
        cursor_x += actor_width + actor_gap;
    }

    let base_spacing = (theme.font_size * 2.1).max(18.0);
    let note_gap_y = (theme.font_size * 0.55).max(5.0);
    let note_gap_x = (theme.font_size * 0.65).max(7.0);
    let note_padding_x = (theme.font_size * 0.75).max(7.0);
    let note_padding_y = (theme.font_size * 0.45).max(4.0);
    let mut extra_before = vec![0.0; graph.edges.len()];
    let frame_end_pad = base_spacing * 0.25;
    for frame in &graph.sequence_frames {
        if frame.start_idx < extra_before.len() {
            extra_before[frame.start_idx] += base_spacing;
        }
        for section in frame.sections.iter().skip(1) {
            if section.start_idx < extra_before.len() {
                extra_before[section.start_idx] += base_spacing;
            }
        }
        if frame.end_idx < extra_before.len() {
            extra_before[frame.end_idx] += frame_end_pad;
        }
    }

    let mut notes_by_index = vec![Vec::new(); graph.edges.len().saturating_add(1)];
    for note in &graph.sequence_notes {
        let idx = note.index.min(graph.edges.len());
        notes_by_index[idx].push(note);
    }

    let mut message_cursor = margin + actor_height + theme.font_size * 2.2;
    let mut message_ys = Vec::new();
    let mut sequence_notes = Vec::new();
    for idx in 0..=graph.edges.len() {
        if let Some(bucket) = notes_by_index.get(idx) {
            for note in bucket {
                message_cursor += note_gap_y;
                let label = measure_label(&note.label, theme, config);
                let mut width = label.width + note_padding_x * 2.0;
                let height = label.height + note_padding_y * 2.0;
                let mut lifeline_xs = note
                    .participants
                    .iter()
                    .filter_map(|id| nodes.get(id))
                    .map(|node| node.x + node.width / 2.0)
                    .collect::<Vec<_>>();
                if lifeline_xs.is_empty() {
                    lifeline_xs.push(0.0);
                }
                let base_x = lifeline_xs[0];
                let min_x = lifeline_xs.iter().copied().fold(f32::INFINITY, f32::min);
                let max_x = lifeline_xs
                    .iter()
                    .copied()
                    .fold(f32::NEG_INFINITY, f32::max);
                if note.position == crate::ir::SequenceNotePosition::Over
                    && note.participants.len() > 1
                {
                    let span = (max_x - min_x).abs();
                    width = width.max(span + note_gap_x * 2.0);
                }
                let x = match note.position {
                    crate::ir::SequenceNotePosition::LeftOf => base_x - note_gap_x - width,
                    crate::ir::SequenceNotePosition::RightOf => base_x + note_gap_x,
                    crate::ir::SequenceNotePosition::Over => (min_x + max_x) / 2.0 - width / 2.0,
                };
                let y = message_cursor;
                sequence_notes.push(SequenceNoteLayout {
                    x,
                    y,
                    width,
                    height,
                    label,
                    position: note.position,
                    participants: note.participants.clone(),
                    index: note.index,
                });
                message_cursor += height + note_gap_y;
            }
        }
        if idx < graph.edges.len() {
            message_cursor += extra_before[idx];
            message_ys.push(message_cursor);
            message_cursor += base_spacing;
        }
    }

    for (idx, edge) in graph.edges.iter().enumerate() {
        let from = nodes.get(&edge.from).expect("from node missing");
        let to = nodes.get(&edge.to).expect("to node missing");
        let y = message_ys.get(idx).copied().unwrap_or(message_cursor);
        let label = edge.label.as_ref().map(|l| measure_label(l, theme, config));
        let start_label = edge
            .start_label
            .as_ref()
            .map(|l| measure_label(l, theme, config));
        let end_label = edge
            .end_label
            .as_ref()
            .map(|l| measure_label(l, theme, config));

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
            points,
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
            let (min_x, max_x) = xs
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |acc, x| {
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
            let mut min_y = first_y;
            let mut max_y = last_y;
            for note in &sequence_notes {
                if note.index >= frame.start_idx && note.index <= frame.end_idx {
                    min_y = min_y.min(note.y);
                    max_y = max_y.max(note.y + note.height);
                }
            }
            let header_offset = theme.font_size * 0.6;
            let top_offset = (2.0 * base_spacing - header_offset).max(base_spacing);
            let bottom_offset = header_offset;
            let frame_y = min_y - top_offset;
            let frame_height = (max_y - min_y).max(0.0) + top_offset + bottom_offset;

            let frame_label_text = match frame.kind {
                crate::ir::SequenceFrameKind::Alt => "alt",
                crate::ir::SequenceFrameKind::Opt => "opt",
                crate::ir::SequenceFrameKind::Loop => "loop",
                crate::ir::SequenceFrameKind::Par => "par",
                crate::ir::SequenceFrameKind::Rect => "rect",
                crate::ir::SequenceFrameKind::Critical => "critical",
                crate::ir::SequenceFrameKind::Break => "break",
            };
            let label_block = measure_label(frame_label_text, theme, config);
            let label_box_w =
                (label_block.width + theme.font_size * 2.0).max(theme.font_size * 3.0);
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
            let label_offset = theme.font_size * 0.7;
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

    let lifeline_start = margin + actor_height;
    let mut last_message_y = message_ys
        .last()
        .copied()
        .unwrap_or(lifeline_start + base_spacing);
    for note in &sequence_notes {
        last_message_y = last_message_y.max(note.y + note.height);
    }
    let footbox_gap = (theme.font_size * 1.25).max(16.0);
    let lifeline_end = last_message_y + footbox_gap;
    let mut lifelines = participants
        .iter()
        .filter_map(|id| nodes.get(id))
        .map(|node| Lifeline {
            id: node.id.clone(),
            x: node.x + node.width / 2.0,
            y1: lifeline_start,
            y2: lifeline_end,
        })
        .collect::<Vec<_>>();

    let mut sequence_footboxes = participants
        .iter()
        .filter_map(|id| nodes.get(id))
        .map(|node| {
            let mut foot = node.clone();
            foot.y = lifeline_end;
            foot
        })
        .collect::<Vec<_>>();

    let mut sequence_boxes = Vec::new();
    if !graph.sequence_boxes.is_empty() {
        let pad_x = theme.font_size * 0.8;
        let pad_y = theme.font_size * 0.6;
        let bottom = sequence_footboxes
            .iter()
            .map(|foot| foot.y + foot.height)
            .fold(lifeline_end, f32::max);
        for seq_box in &graph.sequence_boxes {
            let mut min_x = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            for participant in &seq_box.participants {
                if let Some(node) = nodes.get(participant) {
                    min_x = min_x.min(node.x);
                    max_x = max_x.max(node.x + node.width);
                }
            }
            if !min_x.is_finite() || !max_x.is_finite() {
                continue;
            }
            let x = min_x - pad_x;
            let y = 0.0;
            let width = (max_x - min_x) + pad_x * 2.0;
            let height = bottom + pad_y;
            let label = seq_box
                .label
                .as_ref()
                .map(|text| measure_label(text, theme, config));
            sequence_boxes.push(SequenceBoxLayout {
                x,
                y,
                width,
                height,
                label,
                color: seq_box.color.clone(),
            });
        }
    }
    let activation_width = (theme.font_size * 0.75).max(10.0);
    let activation_offset = (activation_width * 0.6).max(4.0);
    let activation_end_default = message_ys
        .last()
        .copied()
        .unwrap_or(lifeline_start + base_spacing * 0.5)
        + base_spacing * 0.6;
    let mut sequence_activations = Vec::new();
    let mut activation_stacks: HashMap<String, Vec<(f32, usize)>> = HashMap::new();
    let mut events = graph
        .sequence_activations
        .iter()
        .cloned()
        .enumerate()
        .map(|(order, event)| (event.index, order, event))
        .collect::<Vec<_>>();
    events.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let activation_y_for = |idx: usize| {
        if idx < message_ys.len() {
            message_ys[idx]
        } else {
            activation_end_default
        }
    };
    for (_, _, event) in events {
        let y = activation_y_for(event.index);
        let stack = activation_stacks
            .entry(event.participant.clone())
            .or_default();
        match event.kind {
            crate::ir::SequenceActivationKind::Activate => {
                let depth = stack.len();
                stack.push((y, depth));
            }
            crate::ir::SequenceActivationKind::Deactivate => {
                if let Some((start_y, depth)) = stack.pop()
                    && let Some(node) = nodes.get(&event.participant)
                {
                    let base_x = node.x + node.width / 2.0 - activation_width / 2.0;
                    let x = base_x + depth as f32 * activation_offset;
                    let mut y0 = start_y.min(y);
                    let mut height = (y - start_y).abs();
                    if height < base_spacing * 0.6 {
                        height = base_spacing * 0.6;
                    }
                    if y0 < lifeline_start {
                        y0 = lifeline_start;
                    }
                    sequence_activations.push(SequenceActivationLayout {
                        x,
                        y: y0,
                        width: activation_width,
                        height,
                        participant: event.participant.clone(),
                        depth,
                    });
                }
            }
        }
    }
    for (participant, stack) in activation_stacks {
        for (start_y, depth) in stack {
            if let Some(node) = nodes.get(&participant) {
                let base_x = node.x + node.width / 2.0 - activation_width / 2.0;
                let x = base_x + depth as f32 * activation_offset;
                let mut y0 = start_y.min(activation_end_default);
                let mut height = (activation_end_default - start_y).abs();
                if height < base_spacing * 0.6 {
                    height = base_spacing * 0.6;
                }
                if y0 < lifeline_start {
                    y0 = lifeline_start;
                }
                sequence_activations.push(SequenceActivationLayout {
                    x,
                    y: y0,
                    width: activation_width,
                    height,
                    participant: participant.clone(),
                    depth,
                });
            }
        }
    }

    let mut sequence_numbers = Vec::new();
    if let Some(start) = graph.sequence_autonumber {
        let mut value = start;
        for (idx, edge) in graph.edges.iter().enumerate() {
            if let (Some(from), Some(y)) = (nodes.get(&edge.from), message_ys.get(idx).copied()) {
                let from_x = from.x + from.width / 2.0;
                let to_x = nodes
                    .get(&edge.to)
                    .map(|node| node.x + node.width / 2.0)
                    .unwrap_or(from_x);
                let offset = if to_x >= from_x { 16.0 } else { -16.0 };
                sequence_numbers.push(SequenceNumberLayout {
                    x: from_x + offset,
                    y,
                    value,
                });
                value += 1;
            }
        }
    }

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut update_rect = |x: f32, y: f32, w: f32, h: f32| {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
    };
    for node in nodes.values() {
        update_rect(node.x, node.y, node.width, node.height);
    }
    for footbox in &sequence_footboxes {
        update_rect(footbox.x, footbox.y, footbox.width, footbox.height);
    }
    for seq_box in &sequence_boxes {
        update_rect(seq_box.x, seq_box.y, seq_box.width, seq_box.height);
    }
    for frame in &sequence_frames {
        update_rect(frame.x, frame.y, frame.width, frame.height);
    }
    for note in &sequence_notes {
        update_rect(note.x, note.y, note.width, note.height);
    }
    for activation in &sequence_activations {
        update_rect(activation.x, activation.y, activation.width, activation.height);
    }
    for number in &sequence_numbers {
        update_rect(number.x, number.y, 0.0, 0.0);
    }
    for edge in &edges {
        for point in &edge.points {
            min_x = min_x.min(point.0);
            min_y = min_y.min(point.1);
            max_x = max_x.max(point.0);
            max_y = max_y.max(point.1);
        }
    }
    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        min_x = 0.0;
        min_y = 0.0;
        max_x = 1.0;
        max_y = 1.0;
    }

    let margin = 8.0;
    let shift_x = margin - min_x;
    let shift_y = margin - min_y;
    if shift_x.abs() > 1e-3 || shift_y.abs() > 1e-3 {
        for node in nodes.values_mut() {
            node.x += shift_x;
            node.y += shift_y;
        }
        for edge in &mut edges {
            for point in &mut edge.points {
                point.0 += shift_x;
                point.1 += shift_y;
            }
        }
        for lifeline in &mut lifelines {
            lifeline.x += shift_x;
            lifeline.y1 += shift_y;
            lifeline.y2 += shift_y;
        }
        for footbox in &mut sequence_footboxes {
            footbox.x += shift_x;
            footbox.y += shift_y;
        }
        for seq_box in &mut sequence_boxes {
            seq_box.x += shift_x;
            seq_box.y += shift_y;
        }
        for frame in &mut sequence_frames {
            frame.x += shift_x;
            frame.y += shift_y;
            frame.label_box.0 += shift_x;
            frame.label_box.1 += shift_y;
            frame.label.x += shift_x;
            frame.label.y += shift_y;
            for label in &mut frame.section_labels {
                label.x += shift_x;
                label.y += shift_y;
            }
            for divider in &mut frame.dividers {
                *divider += shift_y;
            }
        }
        for note in &mut sequence_notes {
            note.x += shift_x;
            note.y += shift_y;
        }
        for activation in &mut sequence_activations {
            activation.x += shift_x;
            activation.y += shift_y;
        }
        for number in &mut sequence_numbers {
            number.x += shift_x;
            number.y += shift_y;
        }
        max_x += shift_x;
        max_y += shift_y;
    }

    let width = (max_x - min_x + margin * 2.0).max(1.0);
    let height = (max_y - min_y + margin * 2.0).max(1.0);

    Layout {
        kind: graph.kind,
        nodes,
        edges,
        subgraphs,
        width,
        height,
        diagram: DiagramData::Sequence(SequenceData {
            lifelines,
            footboxes: sequence_footboxes,
            boxes: sequence_boxes,
            frames: sequence_frames,
            notes: sequence_notes,
            activations: sequence_activations,
            numbers: sequence_numbers,
        }),
    }
}
