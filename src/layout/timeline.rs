use super::*;

pub(super) fn compute_timeline_layout(
    graph: &Graph,
    theme: &Theme,
    config: &LayoutConfig,
) -> Layout {
    let data = &graph.timeline;
    let font_size = theme.font_size;
    let padding = 30.0;
    let event_width = 120.0;
    let event_height = 80.0;
    let event_spacing = 40.0;
    let title_height = if data.title.is_some() { 40.0 } else { 0.0 };
    let line_y = padding + title_height + 60.0;

    let num_events = data.events.len().max(1);
    let total_events_width =
        num_events as f32 * event_width + (num_events - 1).max(0) as f32 * event_spacing;

    let width = padding * 2.0 + total_events_width;
    let height = padding * 2.0 + title_height + event_height + 100.0;

    let title = data.title.as_ref().map(|t| measure_label(t, theme, config));

    let events: Vec<TimelineEventLayout> = data
        .events
        .iter()
        .enumerate()
        .map(|(i, event)| {
            let x = padding + i as f32 * (event_width + event_spacing);
            let y = line_y + 30.0;

            let time_block = measure_label(&event.time, theme, config);
            let event_blocks: Vec<TextBlock> = event
                .events
                .iter()
                .map(|e| measure_label(e, theme, config))
                .collect();

            TimelineEventLayout {
                time: time_block,
                events: event_blocks,
                x,
                y,
                width: event_width,
                height: event_height,
                circle_y: line_y,
            }
        })
        .collect();

    let line_start_x = padding;
    let line_end_x = width - padding;

    // Sections (simplified - just record them)
    let sections: Vec<TimelineSectionLayout> = data
        .sections
        .iter()
        .enumerate()
        .map(|(i, section)| {
            let label = measure_label(section, theme, config);
            TimelineSectionLayout {
                label,
                x: padding + i as f32 * 200.0,
                y: padding,
                width: 180.0,
                height: 30.0,
            }
        })
        .collect();

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
        timeline: Some(TimelineLayout {
            title,
            title_y: padding + font_size,
            events,
            sections,
            line_y,
            line_start_x,
            line_end_x,
            width,
            height,
        }),
        journey: None,
        error: None,

        width,
        height,
    }
}
