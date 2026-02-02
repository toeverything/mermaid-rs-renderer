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
