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
- runs: 0.033, 0.021, 0.019, 0.019, 0.026
- mean: 0.0237
- p50: 0.0206
- min/max: 0.0194 / 0.0334

Mermaid CLI (mmdc + Puppeteer):
- runs: 2.321, 2.347, 2.349, 2.389, 2.794
- mean: 2.4400
- p50: 2.3491
- min/max: 2.3208 / 2.7941

## Notes
- These runs include process startup and file I/O.
- Mermaid CLI time includes headless Chromium launch.
- Numbers are local measurements; expect variation across machines.
