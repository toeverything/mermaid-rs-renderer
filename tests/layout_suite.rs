use std::path::Path;

use mermaid_rs_renderer::ir::SequenceFrameKind;
use mermaid_rs_renderer::layout::DiagramData;
use mermaid_rs_renderer::{LayoutConfig, Theme, parse_mermaid, render_svg};

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn read_fixture(rel: &str) -> String {
    std::fs::read_to_string(fixture_root().join(rel)).expect("fixture read failed")
}

fn assert_valid_svg(svg: &str, fixture: &str) {
    assert!(svg.contains("<svg"), "{fixture}: missing <svg tag");
    assert!(svg.contains("</svg>"), "{fixture}: missing </svg tag");
}

fn render_fixture(path: &Path) -> String {
    let input = std::fs::read_to_string(path).expect("fixture read failed");
    let parsed = parse_mermaid(&input).expect("parse failed");
    let theme = Theme::modern();
    let layout_config = LayoutConfig::default();
    let layout = mermaid_rs_renderer::compute_layout(&parsed.graph, &theme, &layout_config);
    render_svg(&layout, &theme, &layout_config)
}

fn layout_for_input(input: &str) -> mermaid_rs_renderer::Layout {
    let parsed = parse_mermaid(input).expect("parse failed");
    let theme = Theme::modern();
    let layout_config = LayoutConfig::default();
    mermaid_rs_renderer::compute_layout(&parsed.graph, &theme, &layout_config)
}

fn layout_for_fixture(rel: &str) -> mermaid_rs_renderer::Layout {
    layout_for_input(&read_fixture(rel))
}

fn rects_intersect(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    let ax2 = a.0 + a.2;
    let ay2 = a.1 + a.3;
    let bx2 = b.0 + b.2;
    let by2 = b.1 + b.3;
    a.0 < bx2 && ax2 > b.0 && a.1 < by2 && ay2 > b.1
}

fn rect_within_layout(rect: (f32, f32, f32, f32), layout: &mermaid_rs_renderer::Layout) -> bool {
    rect.0 >= -0.5
        && rect.1 >= -0.5
        && rect.0 + rect.2 <= layout.width + 0.5
        && rect.1 + rect.3 <= layout.height + 0.5
}

#[test]
fn render_all_fixtures() {
    let root = fixture_root();
    let mut fixtures: Vec<String> = Vec::new();

    // Keep this list explicit so new diagram types must be added intentionally.
    let candidates = [
        "architecture/basic.mmd",
        "block/basic.mmd",
        "c4/basic.mmd",
        "class/basic.mmd",
        "class/multiplicity.mmd",
        "er/basic.mmd",
        "flowchart/basic.mmd",
        "flowchart/complex.mmd",
        "flowchart/edges.mmd",
        "flowchart/dense.mmd",
        "flowchart/ports.mmd",
        "flowchart/styles.mmd",
        "flowchart/subgraph.mmd",
        "flowchart/subgraph_direction.mmd",
        "flowchart/cycles.mmd",
        "gantt/basic.mmd",
        "gitgraph/basic.mmd",
        "journey/basic.mmd",
        "kanban/basic.mmd",
        "mindmap/basic.mmd",
        "packet/basic.mmd",
        "pie/basic.mmd",
        "quadrant/basic.mmd",
        "radar/basic.mmd",
        "requirement/basic.mmd",
        "sankey/basic.mmd",
        "sequence/basic.mmd",
        "sequence/frames.mmd",
        "state/basic.mmd",
        "state/note.mmd",
        "timeline/basic.mmd",
        "treemap/basic.mmd",
        "xychart/basic.mmd",
        "zenuml/basic.mmd",
    ];

    for rel in candidates {
        fixtures.push(rel.to_string());
    }

    for rel in fixtures {
        let path = root.join(&rel);
        assert!(path.exists(), "fixture missing: {}", rel);
        let svg = render_fixture(&path);
        assert_valid_svg(&svg, &rel);
    }
}

#[test]
fn render_sequence_frame_with_long_section_label_and_note() {
    let input = read_fixture("sequence/long_section_note.mmd");
    let parsed = parse_mermaid(&input).expect("parse failed");
    let theme = Theme::modern();
    let layout_config = LayoutConfig::default();
    let layout = mermaid_rs_renderer::compute_layout(&parsed.graph, &theme, &layout_config);
    let svg = render_svg(&layout, &theme, &layout_config);

    assert_valid_svg(&svg, "sequence_long_section_label");
    assert!(svg.contains("HealthCheck"), "missing frame section label");
    assert!(svg.contains("Rational thoughts"), "missing note label");
}

#[test]
fn sequence_frame_excludes_note_after_end_and_contains_self_loop_label() {
    let layout = layout_for_fixture("sequence/long_section_note.mmd");
    let DiagramData::Sequence(seq) = &layout.diagram else {
        panic!("expected sequence layout");
    };
    let frame = seq.frames.first().expect("missing loop frame");
    let note = seq.notes.first().expect("missing sequence note");
    let self_loop = &layout.edges[1];
    let self_loop_label = self_loop.label.as_ref().expect("missing self-loop label");
    let (label_x, label_y) = self_loop
        .label_anchor
        .expect("missing self-loop label anchor");
    let label_rect = (
        label_x - self_loop_label.width / 2.0 - 3.0,
        label_y - self_loop_label.height / 2.0 - 2.0,
        self_loop_label.width + 6.0,
        self_loop_label.height + 4.0,
    );
    let frame_rect = (frame.x, frame.y, frame.width, frame.height);
    let note_rect = (note.x, note.y, note.width, note.height);

    assert!(
        !rects_intersect(frame_rect, note_rect),
        "note after frame end should not be enclosed by the loop frame"
    );
    assert!(
        label_rect.0 >= frame.x - 0.5
            && label_rect.0 + label_rect.2 <= frame.x + frame.width + 0.5
            && label_rect.1 >= frame.y - 0.5
            && label_rect.1 + label_rect.3 <= frame.y + frame.height + 0.5,
        "self-loop label should be contained within the loop frame"
    );
    assert!(
        label_rect.1 + label_rect.3 <= self_loop.points[0].1 - 4.0,
        "self-loop label should reserve vertical space above the loop path"
    );
}

#[test]
fn sequence_center_labels_reserve_space_above_message_lines() {
    let layout = layout_for_fixture("sequence/center_labels_clearance.mmd");

    for edge in &layout.edges {
        let Some(label) = edge.label.as_ref() else {
            continue;
        };
        let (label_x, label_y) = edge.label_anchor.expect("missing center label anchor");
        let label_bottom = label_y + label.height / 2.0 + 2.0;
        let line_y = edge.points.first().expect("edge points missing").1;
        assert!(
            label_bottom <= line_y - 4.0,
            "center label at x={label_x:.2} should stay above message line (label bottom {label_bottom:.2}, line y {line_y:.2})"
        );
    }
}

#[test]
fn sequence_nested_frames_contain_children_and_center_section_labels() {
    let layout = layout_for_input(
        r#"sequenceDiagram
    loop Daily query
        Alice->>Bob: Hello Bob, how are you?
        alt is sick
            Bob->>Alice: Not so good :(
        else is well
            Bob->>Alice: Feeling fresh like a daisy
        end

        opt Extra response
            Bob->>Alice: Thanks for asking
        end
    end"#,
    );
    let DiagramData::Sequence(seq) = &layout.diagram else {
        panic!("expected sequence layout");
    };
    assert_eq!(seq.frames.len(), 3, "expected loop/alt/opt frames");

    let loop_frame = seq
        .frames
        .iter()
        .find(|frame| frame.kind == SequenceFrameKind::Loop)
        .expect("missing loop frame");
    let alt_frame = seq
        .frames
        .iter()
        .find(|frame| frame.kind == SequenceFrameKind::Alt)
        .expect("missing alt frame");
    let opt_frame = seq
        .frames
        .iter()
        .find(|frame| frame.kind == SequenceFrameKind::Opt)
        .expect("missing opt frame");

    let loop_right = loop_frame.x + loop_frame.width;
    let loop_bottom = loop_frame.y + loop_frame.height;
    for child in [alt_frame, opt_frame] {
        assert!(
            child.x >= loop_frame.x - 0.5
                && child.y >= loop_frame.y - 0.5
                && child.x + child.width <= loop_right + 0.5
                && child.y + child.height <= loop_bottom + 0.5,
            "loop frame should enclose nested {:?} frame",
            child.kind
        );
    }
    assert!(
        alt_frame.x - loop_frame.x >= 8.0
            && alt_frame.y - loop_frame.y >= 8.0
            && loop_right - (alt_frame.x + alt_frame.width) >= 8.0,
        "loop frame should leave visible gutter around nested alt frame"
    );
    assert!(
        opt_frame.x - loop_frame.x >= 8.0
            && opt_frame.y - loop_frame.y >= 8.0
            && loop_right - (opt_frame.x + opt_frame.width) >= 8.0,
        "loop frame should leave visible gutter around nested opt frame"
    );

    let else_label = alt_frame
        .section_labels
        .get(1)
        .expect("missing else section label");
    let alt_mid_x = alt_frame.x + alt_frame.width / 2.0;
    assert!(
        (else_label.x - alt_mid_x).abs() <= alt_frame.width * 0.12,
        "else label should be centered within the alt fragment (label x {:.2}, frame mid {:.2})",
        else_label.x,
        alt_mid_x
    );
}

#[test]
fn state_notes_avoid_neighbor_nodes_on_requested_side() {
    let layout = layout_for_input(
        r#"stateDiagram-v2
    direction LR
    [*] --> Idle
    Idle --> Active
    Active --> Review
    Review --> Done
    note right of Active: Running for a prolonged period
    Done --> [*]"#,
    );
    let DiagramData::Graph { state_notes } = &layout.diagram else {
        panic!("expected graph layout");
    };
    let note = state_notes.first().expect("missing state note");
    let target = layout.nodes.get("Active").expect("missing Active node");
    let note_rect = (note.x, note.y, note.width, note.height);
    let right_side_nodes: Vec<_> = layout
        .nodes
        .values()
        .filter(|node| node.id != target.id && node.x >= target.x + target.width - 0.5)
        .collect();
    assert!(
        !right_side_nodes.is_empty(),
        "fixture should place at least one node on the requested note side"
    );
    assert!(
        note.x >= target.x + target.width - 0.5,
        "right-of note should remain on the target's right side"
    );
    for node in right_side_nodes {
        let node_rect = (node.x, node.y, node.width, node.height);
        assert!(
            !rects_intersect(note_rect, node_rect),
            "state note should avoid node {} on the requested side",
            node.id
        );
    }
}

#[test]
fn quadrant_point_labels_avoid_each_other_and_stay_on_canvas() {
    let layout = layout_for_input(
        r#"quadrantChart
    title Dense Labels
    x-axis Low Reach --> High Reach
    y-axis Low Engagement --> High Engagement
    quadrant-1 Execute
    quadrant-2 Expand
    quadrant-3 Monitor
    quadrant-4 Re-evaluate
    Alpha deployment track : [0.50, 0.52]
    Beta release candidate : [0.52, 0.50]
    Gamma adoption stream : [0.48, 0.51]"#,
    );
    let DiagramData::Quadrant(quadrant) = &layout.diagram else {
        panic!("expected quadrant layout");
    };
    let label_rects: Vec<_> = quadrant
        .points
        .iter()
        .map(|point| {
            (
                point.label_x - point.label.width / 2.0,
                point.label_y - point.label.height / 2.0,
                point.label.width,
                point.label.height,
            )
        })
        .collect();
    for rect in &label_rects {
        assert!(
            rect_within_layout(*rect, &layout),
            "quadrant label should stay within the layout bounds"
        );
    }
    for i in 0..label_rects.len() {
        for j in (i + 1)..label_rects.len() {
            assert!(
                !rects_intersect(label_rects[i], label_rects[j]),
                "quadrant point labels {i} and {j} should not overlap"
            );
        }
    }
}

#[test]
fn render_kanban_with_frontmatter_and_duplicate_task_ids() {
    let input = read_fixture("kanban/frontmatter_duplicate_ids.mmd");
    let parsed = parse_mermaid(&input).expect("parse failed");
    assert_eq!(
        parsed.graph.subgraphs.len(),
        6,
        "unexpected kanban column count"
    );
    let ticket_base_url = parsed
        .init_config
        .as_ref()
        .and_then(|cfg| cfg.get("kanban"))
        .and_then(|kanban| kanban.get("ticketBaseUrl"))
        .and_then(|value| value.as_str());
    assert_eq!(
        ticket_base_url,
        Some("https://issues.example.test/browse/#TICKET#")
    );
    let theme = Theme::modern();
    let layout_config = LayoutConfig::default();
    let layout = mermaid_rs_renderer::compute_layout(&parsed.graph, &theme, &layout_config);
    let svg = render_svg(&layout, &theme, &layout_config);

    assert_valid_svg(&svg, "kanban_duplicate_task_ids");
    assert!(
        svg.contains("Weird flickering in Firefox"),
        "missing duplicate-id task"
    );
    assert!(
        svg.contains("Can&apos;t reproduce"),
        "missing final column label"
    );
    assert!(
        !svg.contains("ticketBaseUrl"),
        "frontmatter leaked into rendered output"
    );
}
