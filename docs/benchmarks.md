# Benchmarks

Date: February 7, 2026 (v0.2.0)

## Environment
- Rust: `rustc 1.93.0 (254b59607 2026-01-19)`
- Node: `v25.2.1`
- Mermaid CLI: `mermaid-cli 11.4.2 via Puppeteer/Chromium`
- CPU: Intel Core Ultra 7 265V

## Core diagram speedups

| Diagram | mmdr | mermaid-cli | Speedup |
|:--------|-----:|------------:|--------:|
| Flowchart | 4.49 ms | 1,971 ms | **439x** |
| Class | 4.67 ms | 1,907 ms | **408x** |
| State | 3.97 ms | 1,968 ms | **496x** |
| Sequence | 2.71 ms | 1,906 ms | **704x** |

## All diagram types

| Diagram | mmdr | mermaid-cli | Speedup |
|:--------|-----:|------------:|--------:|
| Flowchart (small) | 3.38 ms | 1,910 ms | 565x |
| Flowchart (medium) | 8.71 ms | 2,018 ms | 232x |
| Flowchart (large) | 47.00 ms | 2,276 ms | 48x |
| Class | 3.61 ms | 2,000 ms | 554x |
| State | 5.05 ms | 2,227 ms | 441x |
| Sequence | 4.07 ms | 1,969 ms | 484x |
| ER | 5.84 ms | 2,012 ms | 344x |
| Pie | 4.51 ms | 1,952 ms | 433x |
| Gantt | 4.03 ms | 1,952 ms | 484x |
| Mindmap | 4.20 ms | 1,949 ms | 465x |
| Journey | 4.93 ms | 1,941 ms | 394x |
| Timeline | 3.28 ms | 1,954 ms | 596x |
| Git Graph | 5.43 ms | 1,931 ms | 356x |
| Quadrant | 5.82 ms | 1,914 ms | 329x |
| Requirement | 4.01 ms | 1,985 ms | 495x |
| C4 | 4.23 ms | 2,110 ms | 498x |
| Sankey | 5.08 ms | 1,944 ms | 382x |
| ZenUML | 6.32 ms | 2,028 ms | 321x |
| Block | 6.48 ms | 1,907 ms | 294x |
| Packet | 2.65 ms | 1,936 ms | 732x |
| Kanban | 2.04 ms | 1,985 ms | 973x |
| Architecture | 2.86 ms | 1,967 ms | 688x |
| Radar | 1.98 ms | 1,919 ms | 968x |
| Treemap | 4.03 ms | 1,940 ms | 482x |

## Notes
- These runs include process startup and file I/O.
- Mermaid CLI time includes headless Chromium launch.
- Numbers are local measurements; expect variation across machines.

## Improvement Prioritization Benchmark

Use `scripts/priority_bench.py` to rank where layout work should focus next by combining
quality pain (crossings, node-edge intersections, bends, port congestion, overlaps,
edge detour, and whitespace efficiency) with layout time.
Priority weights are derived automatically from the fixture corpus by default
(`--weight-mode auto`) to reduce hand-tuned bias.

```bash
# Full suite (tests + benches)
python3 scripts/priority_bench.py --runs 3 --warmup 1

# Focus flowchart family first
python3 scripts/priority_bench.py --pattern flowchart --top 15

# Use fixed fallback weights only if needed
python3 scripts/priority_bench.py --pattern flowchart --weight-mode manual
```

The script writes a machine-readable report to `target/priority-bench.json` and prints:
- top fixtures by quality pain
- top quick wins by pain-per-layout-millisecond
- top fixtures by space inefficiency (wasted space, component gap, center offset)

Recent stress fixtures for visual quality include:
- `benches/fixtures/flowchart_ports_heavy.mmd`
- `benches/fixtures/flowchart_weave.mmd`
- `benches/fixtures/flowchart_backedges_subgraphs.mmd`
- `benches/fixtures/flowchart_sparse_components.mmd`
- `benches/fixtures/flowchart_lanes_crossfeed.mmd`
- `benches/fixtures/flowchart_grid_feedback.mmd`
- `benches/fixtures/flowchart_fanout_returns.mmd`
- `benches/fixtures/flowchart_label_collision.mmd`
- `benches/fixtures/flowchart_nested_clusters.mmd`
- `benches/fixtures/flowchart_asymmetric_components.mmd`
- `benches/fixtures/flowchart_parallel_merges.mmd`
- `benches/fixtures/flowchart_long_edge_labels.mmd`
- `benches/fixtures/flowchart_selfloop_bidi.mmd`
- `benches/fixtures/flowchart_component_packing.mmd`
- `benches/fixtures/flowchart_direction_conflict.mmd`
- `benches/fixtures/flowchart_parallel_label_stack.mmd`
- `benches/fixtures/flowchart_port_alignment_matrix.mmd`
- `benches/fixtures/flowchart_path_occlusion_maze.mmd`
- `benches/fixtures/flowchart_mega_multihub_control.mmd`
- `benches/fixtures/flowchart_mega_crosslane_subgraphs.mmd`
- `benches/fixtures/flowchart_mega_braid_feedback.mmd`
- `benches/fixtures/flowchart_mega_event_mesh.mmd`
- `benches/fixtures/flowchart_mega_nested_regions.mmd`

Latest flowchart quality compare (`scripts/quality_bench.py --engine both --pattern flowchart`, February 6, 2026):
- `mmdr`: 30 fixtures, average weighted score `435.06`
- `mermaid-cli`: 30 fixtures, average weighted score `1140.45`
- `mmdr avg wasted space ratio`: `0.177`
- `mmdr avg edge detour ratio`: `1.253`
- `mmdr avg component gap ratio`: `0.086`
- `mmdr avg label out-of-bounds count`: `0.000`

Recent layout/readability fixes validated by these runs:
- Fixed flowchart parsing of hyphenated pipe labels (no phantom nodes from labels like `|high-risk order|`).
- Edge-label placement now clamps to canvas bounds and optimizes overlap first, removing suite-level label clipping.
- Added multi-anchor edge-label search (longest-segment + path-fraction anchors) and priority-aware edge routing order on larger graphs, reducing crossings on heavy backedge fixtures.
- Added an objective stage between placement and routing:
  - class multiplicity edge-span relaxation (removed multiplicity label-label overlap in `tests/fixtures/class/multiplicity.mmd`)
  - tiny-cycle overlap resolution (removed node overlap and label overlap in `tests/fixtures/flowchart/cycles.mmd`)
  - chain-aware top-level subgraph wrapping for very large flowcharts (`benches/fixtures/flowchart_large.mmd` aspect elongation `153.63 -> 1.71`, wasted space `0.286 -> 0.071`).
- `label_overlap_count` now ignores tiny text-box slivers (`<= 10px²`) to
  reduce host/font jitter noise in cross-machine comparisons.

## Benchmark History Logging

`scripts/quality_bench.py` now appends a JSONL run history record by default:
- file: `tmp/benchmark-history/quality-runs.jsonl`
- metadata: timestamp, CLI args, fixture/pattern selection, host info, git commit SHA, branch, dirty state
- summaries: average scores and comparison/dominance stats (for `--engine both`)

Disable per run if needed:

```bash
python3 scripts/quality_bench.py --engine both --no-history-log
```

Mermaid-cli caching is enabled by default for `quality_bench.py` and
`label_bench.py`:
- cache dir: `tmp/benchmark-cache/mmdc`
- cache key inputs: fixture contents, config contents, mermaid-cli command,
  mermaid-cli version, and benchmark script revision

Useful flags:

```bash
python3 scripts/quality_bench.py --engine both --mmdc-cache-dir tmp/benchmark-cache/mmdc
python3 scripts/quality_bench.py --engine both --no-mmdc-cache
python3 scripts/label_bench.py --engine both --no-mmdc-cache
```

## Label Path-Gap Benchmark

To benchmark edge-label placement directly, use:

```bash
python3 scripts/label_bench.py --engine both --pattern flowchart
```

This benchmark reports `edge_label_path_gap_*` metrics where:
- `edge_label_path_gap_mean`: average label-box to nearest edge-path gap
- `edge_label_path_gap_p95`: 95th percentile gap
- `edge_label_path_touch_ratio`: fraction of labels touching their nearest edge path (`0` gap)
- `edge_label_path_non_touch_ratio`: fraction of labels not touching their nearest edge path (`1 - touch_ratio`)
- `edge_label_path_clearance_score_mean`: average clearance quality score in `[0,1]`
  where touching scores `0` and score peaks near a small positive clearance band
- `edge_label_path_in_band_ratio`: fraction of labels in the target clearance band (`1px..6px`)
- `edge_label_path_gap_bad_ratio`: fraction of labels beyond diagram-specific gap thresholds

Candidate selection details:
- If explicit edge-label boxes are present in SVG, only those boxes are scored.
- Sequence rendering now emits explicit `.edgeLabel` rectangles for message labels
  (in addition to text) so sequence path-gap metrics are measured against the
  rendered label box geometry instead of inferred text-only bounds.
- Fallback text-label scoring is enabled only for fixtures that appear to contain
  explicit edge labels in source syntax.
- Sequence fallback candidates are capped to the expected message-label count and
  ranked by nearest-path gap to avoid actor/footbox text polluting label metrics.

Run history for this benchmark is also logged by default to:
- `tmp/benchmark-history/label-runs.jsonl`

`scripts/priority_bench.py` now appends run history by default to:
- `tmp/benchmark-history/priority-runs.jsonl`

`scripts/bench_compare.py` now appends run history by default to:
- `tmp/benchmark-history/bench-compare-runs.jsonl`

`scripts/bench_compare.py` also supports mermaid-cli result caching by default
to avoid rerunning slow CLI loops on unchanged inputs:
- cache dir default: `tmp/benchmark-cache/bench-compare/mmdc`
- cache key inputs: fixture contents, mermaid-cli command + version, bench
  sampling settings (`MMD_CLI_RUNS`, `MMD_CLI_WARMUP`), and optional
  `MMDC_CONFIG`
- default mermaid-cli sampling is lightweight for speed:
  `MMD_CLI_RUNS=1`, `MMD_CLI_WARMUP=0` (override as needed)
- mermaid-cli execution is parallelized by default with
  `MMD_CLI_JOBS=min(4, cpu_count)`; set `MMD_CLI_JOBS=1` for serial runs
- when `MMD_CLI_WARMUP=0`, bench runs do not perform an extra preflight CLI
  invocation, so cold runtime scales with measured run count only
- mermaid-cli memory probing is opt-in (`MMD_CLI_MEASURE_MEMORY=1`) because it
  adds an extra CLI execution per case
- mmdr memory probing is also opt-in (`MMDR_MEASURE_MEMORY=1`) because it adds
  one extra mmdr execution per case
- each run prints a runtime breakdown (`mmdr`, `mermaid-cli`, charts, history,
  total) and writes runtime fields into bench history records

Useful environment knobs:

```bash
MMDC_CACHE_DIR=tmp/benchmark-cache/bench-compare/mmdc python3 scripts/bench_compare.py
NO_MMDC_CACHE=1 python3 scripts/bench_compare.py
MMDC_CONFIG=tests/fixtures/modern-config.json python3 scripts/bench_compare.py
MMD_CLI_RUNS=5 MMD_CLI_WARMUP=1 python3 scripts/bench_compare.py
MMD_CLI_JOBS=1 python3 scripts/bench_compare.py
MMD_CLI_JOBS=4 python3 scripts/bench_compare.py
MMD_CLI_MEASURE_MEMORY=1 python3 scripts/bench_compare.py
MMDR_MEASURE_MEMORY=1 python3 scripts/bench_compare.py
```

Each history record includes timestamp, git commit/branch/dirty state, host metadata, run settings, and summary metrics.
