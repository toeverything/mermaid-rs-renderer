# mermaid-rs-renderer

A fast, native Mermaid diagram renderer. No browser, no Node.js, no Puppeteer.

## Performance

**~500-1000x faster than mermaid-cli** for typical diagrams.

| Diagram | mmdr | mermaid-cli | Speedup |
|---------|-----:|------------:|--------:|
| flowchart_small | 2.85 ms | 2,784 ms | **976x** |
| flowchart_medium | 3.09 ms | 2,962 ms | **960x** |
| flowchart_large | 6.42 ms | 3,343 ms | **521x** |

<sub>Intel Core Ultra 7 256V, Linux 6.18.2 | mermaid-cli 11.12.0 via Puppeteer/Chromium</sub>

## Why?

The official `mermaid-cli` spawns a headless Chromium browser via Puppeteer for every diagram, adding ~2-3 seconds of overhead. This makes it painful for:
- CI/CD pipelines rendering many diagrams
- Real-time previews in editors
- Batch documentation generation

`mmdr` parses Mermaid syntax natively and renders directly to SVG, then optionally rasterizes via `resvg`. No browser needed.

## Install

```bash
# From source
cargo install --path .

# Homebrew (macOS/Linux)
brew tap 1jehuang/mmdr && brew install mmdr

# Scoop (Windows)
scoop bucket add mmdr https://github.com/1jehuang/scoop-mmdr && scoop install mmdr

# AUR (Arch)
yay -S mmdr-bin
```

## Usage

```bash
# SVG to stdout
cat diagram.mmd | mmdr -e svg

# Render to file
mmdr -i diagram.mmd -o output.svg -e svg
mmdr -i diagram.mmd -o output.png -e png

# Render all mermaid blocks from Markdown
mmdr -i README.md -o ./diagrams/ -e svg

# Custom config
mmdr -i diagram.mmd -o out.svg -e svg -c config.json

# Override spacing
mmdr -i diagram.mmd -o out.svg -e svg --nodeSpacing 60 --rankSpacing 120
```

## Visual Comparison

> **Note:** Visual output is actively being improved. Examples show current rendering.

<details>
<summary>Flowchart</summary>

| mmdr | Official mermaid-cli |
|------|----------------------|
| <img src="docs/comparisons/flowchart_mmdr.svg" alt="mmdr flowchart" width="400"> | <img src="docs/comparisons/flowchart_official.svg" alt="official flowchart" width="400"> |

</details>

<details>
<summary>Class Diagram</summary>

| mmdr | Official mermaid-cli |
|------|----------------------|
| <img src="docs/comparisons/class_mmdr.svg" alt="mmdr class" width="400"> | <img src="docs/comparisons/class_official.svg" alt="official class" width="400"> |

</details>

<details>
<summary>State Diagram</summary>

| mmdr | Official mermaid-cli |
|------|----------------------|
| <img src="docs/comparisons/state_mmdr.svg" alt="mmdr state" width="400"> | <img src="docs/comparisons/state_official.svg" alt="official state" width="400"> |

</details>

<details>
<summary>Sequence Diagram</summary>

| mmdr | Official mermaid-cli |
|------|----------------------|
| <img src="docs/comparisons/sequence_mmdr.svg" alt="mmdr sequence" width="400"> | <img src="docs/comparisons/sequence_official.svg" alt="official sequence" width="400"> |

</details>

## Supported Features

**Diagram types:**
- `flowchart` / `graph` (TD, TB, LR, RL, BT)
- `classDiagram`
- `stateDiagram-v2`
- `sequenceDiagram` (participants, messages, lifelines)

**Node shapes:** rectangle, round-rect, stadium, circle, double-circle, diamond, hexagon, cylinder, subroutine, trapezoid, parallelogram, asymmetric

**Edges:** solid, dotted, thick; arrows + circle/cross/diamond decorations; labels

**Styling:** `classDef`, `class`, inline `:::class`, `style`, `linkStyle`, `%%{init}%%` directives

**Layout:** subgraphs with `direction`, nested subgraphs

## Configuration

Accepts Mermaid-compatible `themeVariables`:

```json
{
  "themeVariables": {
    "primaryColor": "#F8FAFF",
    "primaryTextColor": "#1C2430",
    "primaryBorderColor": "#C7D2E5",
    "lineColor": "#7A8AA6",
    "secondaryColor": "#F0F4FF",
    "tertiaryColor": "#E8EEFF",
    "edgeLabelBackground": "#FFFFFF",
    "clusterBkg": "#F8FAFF",
    "clusterBorder": "#C7D2E5",
    "background": "#FFFFFF",
    "fontFamily": "Inter, system-ui, sans-serif",
    "fontSize": 13
  },
  "flowchart": {
    "nodeSpacing": 50,
    "rankSpacing": 50
  }
}
```

## Architecture

<img src="docs/diagrams/architecture.svg" alt="Architecture comparison" width="700">

```
mmdr (native)                    mermaid-cli
─────────────                    ───────────
.mmd input                       .mmd input
    ↓                                ↓
parser.rs                        mermaid-js
    ↓                                ↓
ir.rs (Graph IR)                 dagre (layout)
    ↓                                ↓
layout.rs + dagre_rust           Browser DOM
    ↓                                ↓
render.rs                        Puppeteer
    ↓                                ↓
SVG output                       Chromium (headless)
    ↓ (optional)                     ↓
resvg → PNG                      Screenshot → PNG
```

## Development

```bash
cargo test
cargo run -- -i docs/diagrams/architecture.mmd -o /tmp/out.svg -e svg
```

**Benchmarks:**
```bash
# Microbenchmarks (parse/layout/render)
cargo bench --bench renderer

# End-to-end vs mermaid-cli
cargo build --release && python scripts/bench_compare.py
```

## License

MIT
