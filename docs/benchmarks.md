# Benchmarks

Date: February 2, 2026

## Render target
- Input: `docs/diagrams/architecture.mmd`
- Output: PNG

## Environment
- Rust: `rustc 1.92.0 (ded5c06cf 2025-12-08) (Arch Linux rust 1:1.92.0-1)`
- Node: `v25.2.1`
- Mermaid CLI: `npx -y @mermaid-js/mermaid-cli`
- Headless Chromium: `chrome-headless-shell` via Puppeteer cache

## Results (seconds)

Rust renderer (this project):
- runs: 0.025, 0.024, 0.021, 0.020, 0.020
- mean: 0.0222
- p50: 0.0211
- min/max: 0.0201 / 0.0253

Mermaid CLI (mmdc + Puppeteer):
- runs: 2.398, 2.280, 2.602, 2.115, 2.090
- mean: 2.2970
- p50: 2.2802
- min/max: 2.0903 / 2.6017

## Font cache (warm)

After the font cache is populated (default behavior), tiny/common diagrams reach 500–900×:

| Diagram | mmdr (warm cache) | mermaid-cli | Speedup |
|:--|--:|--:|--:|
| Flowchart (tiny) | 2.96 ms | 2,259 ms | 764× |
| Class (tiny) | 2.55 ms | 2,347 ms | 919× |
| State (tiny) | 2.67 ms | 2,111 ms | 789× |
| Sequence (tiny) | 3.75 ms | 2,010 ms | 536× |

## Fast text metrics (tiny diagrams)

Using `mmdr --fastText` on tiny/common diagrams (measured Feb 2, 2026):

| Diagram | mmdr `--fastText` | mermaid-cli | Speedup |
|:--|--:|--:|--:|
| Flowchart (tiny) | 1.32 ms | 2,116 ms | 1,601× |
| Class (tiny) | 1.23 ms | 2,314 ms | 1,880× |
| State (tiny) | 1.09 ms | 2,258 ms | 2,069× |
| Sequence (tiny) | 1.16 ms | 2,158 ms | 1,868× |

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
