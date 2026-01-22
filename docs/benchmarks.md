# Benchmarks

Date: January 22, 2026

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
- runs: 0.025, 0.016, 0.018, 0.018, 0.019
- mean: 0.0191
- p50: 0.0183
- min/max: 0.0159 / 0.0246

Mermaid CLI (mmdc + Puppeteer):
- runs: 2.384, 2.099, 2.071, 2.207, 2.065
- mean: 2.1652
- p50: 2.0994
- min/max: 2.0647 / 2.3842

## Notes
- These runs include process startup and file I/O.
- Mermaid CLI time includes headless Chromium launch.
- Numbers are local measurements; expect variation across machines.
