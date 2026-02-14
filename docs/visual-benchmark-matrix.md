# Visual Benchmark Matrix

Date: February 14, 2026

This matrix tracks what visual-readability behaviors are explicitly benchmarked.

| Category | What it measures | Fixtures |
|:--|:--|:--|
| Edge crossing pressure | Dense edge intersections and backedges | `flowchart_weave.mmd`, `flowchart_grid_feedback.mmd`, `flowchart_backedges_subgraphs.mmd`, `tests/fixtures/flowchart/dense.mmd` |
| Port congestion / fan patterns | Hub fan-out, fan-in, return links | `flowchart_ports_heavy.mmd`, `flowchart_fanout_returns.mmd`, `tests/fixtures/flowchart/ports.mmd` |
| Port side correctness | Whether port exits/entries align with the target direction and edge tangent | `flowchart_port_alignment_matrix.mmd`, `flowchart_ports_heavy.mmd`, `flowchart_fanout_returns.mmd` |
| Label readability | Label-label and label-edge collisions | `flowchart_label_collision.mmd`, `flowchart_lanes_crossfeed.mmd`, `flowchart_parallel_label_stack.mmd`, `tests/fixtures/flowchart/styles.mmd` |
| Label clipping / viewport fit | Out-of-bounds edge labels at canvas edges | `flowchart_direction_conflict.mmd`, `flowchart_long_edge_labels.mmd`, `flowchart_selfloop_bidi.mmd` |
| Endpoint multiplicity readability | Class endpoint labels and center labels competing for edge span | `tests/fixtures/class/multiplicity.mmd`, `docs/comparison_sources/class_multiplicity.mmd` |
| Nested structure readability | Subgraph nesting and region clarity | `flowchart_nested_clusters.mmd`, `tests/fixtures/flowchart/subgraph.mmd`, `tests/fixtures/flowchart/subgraph_direction.mmd` |
| Space efficiency / composition | Wasted space, fill ratio, margin imbalance | `flowchart_asymmetric_components.mmd`, `flowchart_sparse_components.mmd`, `tests/fixtures/flowchart/basic.mmd` |
| Extreme aspect robustness | Very wide/tall chain layouts and readability under large aspect stress | `benches/fixtures/flowchart_large.mmd` |
| Component packing | Gaps created by disconnected or weakly connected regions | `flowchart_component_packing.mmd`, `flowchart_lanes_crossfeed.mmd`, `flowchart_grid_feedback.mmd` |
| Loop/bidirectional readability | Self-loops and reciprocal routing clarity | `flowchart_selfloop_bidi.mmd`, `tests/fixtures/flowchart/cycles.mmd` |
| Path directness | Detour-heavy routes and long orthogonal paths | `flowchart_grid_feedback.mmd`, `flowchart_parallel_merges.mmd`, `tests/fixtures/flowchart/cycles.mmd` |
| Path occlusion severity | How much edge path length runs through non-endpoint nodes | `flowchart_path_occlusion_maze.mmd`, `flowchart_weave.mmd`, `flowchart_component_packing.mmd` |
| Large-system stress | Multi-hub, nested regions, and heavy cross-lane traffic | `flowchart_mega_multihub_control.mmd`, `flowchart_mega_crosslane_subgraphs.mmd`, `flowchart_mega_braid_feedback.mmd`, `flowchart_mega_event_mesh.mmd`, `flowchart_mega_nested_regions.mmd` |
| Speed under visual stress | Layout/render latency on readability-heavy cases | all `benches/fixtures/flowchart_*.mmd` stress fixtures |

## Scored Metrics

The benchmark pipeline now tracks:
- Structural readability: `edge_crossings`, `edge_node_crossings`, `node_overlap_count`, `edge_bends`, `port_congestion`, `edge_overlap_length`
- Port correctness: `port_target_side_mismatch_count`, `port_target_side_mismatch_ratio`, `port_direction_misalignment_count`, `port_direction_misalignment_ratio`, `endpoint_off_boundary_count`, `endpoint_off_boundary_ratio`, `endpoint_boundary_error_mean`
- Occlusion severity: `edge_node_crossing_length`, `edge_node_crossing_length_per_edge`
- Geometry readability: `crossing_angle_penalty`, `angular_resolution_penalty`, `edge_node_near_miss_count`, `node_spacing_violation_count`, `node_spacing_violation_severity`
- Space/composition: `content_fill_ratio`, `wasted_space_ratio`, `space_efficiency_penalty`, `margin_imbalance_ratio`, `component_gap_ratio`, `component_balance_penalty`, `content_center_offset_ratio`, `content_aspect_elongation`, `content_overflow_ratio`
- Path quality: `avg_edge_detour_ratio`, `edge_detour_penalty`, `edge_length_per_node`
- Text readability: `label_overlap_count`, `label_overlap_area`, `label_edge_overlap_count`, `label_edge_overlap_pairs`, `label_out_of_bounds_count`, `label_out_of_bounds_area`, `label_out_of_bounds_ratio`
- Edge-label attachment quality: `edge_label_alignment_mean`, `edge_label_alignment_p95`, `edge_label_alignment_bad_count`, `edge_label_path_gap_mean`, `edge_label_path_gap_p95`, `edge_label_path_touch_ratio`, `edge_label_path_gap_bad_count`
- Throughput: parse/layout/render/total timing from `--timing`

## Recent Engine Fixes Verified By Benchmarks

- Flowchart parser no longer splits pipe labels containing hyphens into phantom nodes
  (e.g. `|high-risk order|`).
- Edge-label placement now clamps to canvas bounds and uses overlap-first candidate
  selection, eliminating label clipping on the flowchart suite (`avg label out-of-bounds count: 0.000` in latest run).
- Edge routing now uses priority-aware ordering on larger flowcharts to reduce dense
  backedge crossings while preserving small-graph behavior.
- Edge labels now evaluate multiple anchor candidates along each path, improving
  readability on long-label and dense parallel-edge fixtures.
- A visual objective stage now runs after node placement:
  - class multiplicity edge-span relaxation to avoid endpoint/center label pileups
  - tiny cyclic-flow overlap resolution to remove node-label overlap in `tests/fixtures/flowchart/cycles.mmd`
  - chain-aware wrapping for large top-level subgraph chains (`flowchart_large.mmd`) to prevent unusable ultra-wide canvases.

## Weighting

Priority scoring uses `scripts/priority_bench.py --weight-mode auto` by default.
Weights are derived programmatically from fixture metrics (variance + inter-metric correlation),
so tuning does not depend on manually picked constants.
