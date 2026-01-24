<div align="center">

# mmdr

**500-1000x faster Mermaid rendering. Pure Rust. Zero browser dependencies.**

[Installation](#installation) | [Quick Start](#quick-start) | [Benchmarks](#performance) | [Examples](#diagram-types)

</div>

## Performance

mmdr renders diagrams **500-1000x faster** than mermaid-cli by eliminating browser overhead.

| Diagram | mmdr | mermaid-cli | Speedup |
|:--------|-----:|------------:|--------:|
| flowchart (small) | 2.85 ms | 2,473 ms | **868x** |
| flowchart (medium) | 4.45 ms | 2,445 ms | **549x** |
| flowchart (large) | 5.74 ms | 2,751 ms | **479x** |
| classDiagram | 1.71 ms | 2,430 ms | **1,421x** |
| stateDiagram-v2 | 1.96 ms | 2,425 ms | **1,237x** |
| sequenceDiagram | 2.01 ms | 2,293 ms | **1,141x** |

<sub>Intel Core Ultra 7 256V, Linux 6.18.2 | mermaid-cli 11.12.0 via Puppeteer/Chromium</sub>

## Why mmdr?

The official `mermaid-cli` spawns a **headless Chromium browser** for every diagram, adding 2-3 seconds of startup overhead.

| Use Case | mermaid-cli | mmdr |
|:---------|:------------|:-----|
| CI/CD pipeline with 50 diagrams | ~2 minutes | **< 1 second** |
| Real-time editor preview | Unusable lag | **Instant** |
| Batch doc generation | Coffee break | **Blink of an eye** |

mmdr parses Mermaid syntax natively in Rust and renders directly to SVG. No browser. No Node.js. No Puppeteer.

## Installation

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

## Quick Start

```bash
# Pipe diagram to stdout
echo 'flowchart LR; A-->B-->C' | mmdr -e svg

# File to file
mmdr -i diagram.mmd -o output.svg -e svg
mmdr -i diagram.mmd -o output.png -e png

# Render all diagrams from a Markdown file
mmdr -i README.md -o ./diagrams/ -e svg
```

## Diagram Types

mmdr supports the four most common Mermaid diagram types:

<table>
<tr>
<td align="center" width="50%">
<strong>Flowchart</strong><br>
<img src="docs/comparisons/flowchart_mmdr.svg" alt="Flowchart" width="100%">
</td>
<td align="center" width="50%">
<strong>Class Diagram</strong><br>
<img src="docs/comparisons/class_mmdr.svg" alt="Class Diagram" width="100%">
</td>
</tr>
<tr>
<td align="center" width="50%">
<strong>State Diagram</strong><br>
<img src="docs/comparisons/state_mmdr.svg" alt="State Diagram" width="100%">
</td>
<td align="center" width="50%">
<strong>Sequence Diagram</strong><br>
<img src="docs/comparisons/sequence_mmdr.svg" alt="Sequence Diagram" width="100%">
</td>
</tr>
</table>

<details>
<summary><strong>Compare with mermaid-cli output</strong></summary>

> **Note:** Visual output is actively being improved.

| Type | mmdr | mermaid-cli |
|:-----|:----:|:-----------:|
| Flowchart | <img src="docs/comparisons/flowchart_mmdr.svg" width="350"> | <img src="docs/comparisons/flowchart_official.svg" width="350"> |
| Class | <img src="docs/comparisons/class_mmdr.svg" width="350"> | <img src="docs/comparisons/class_official.svg" width="350"> |
| State | <img src="docs/comparisons/state_mmdr.svg" width="350"> | <img src="docs/comparisons/state_official.svg" width="350"> |
| Sequence | <img src="docs/comparisons/sequence_mmdr.svg" width="350"> | <img src="docs/comparisons/sequence_official.svg" width="350"> |

</details>

## More Diagrams

<details>
<summary><strong>Node Shapes</strong></summary>

| Shape | Syntax |
|:------|:-------|
| Rectangle | `[text]` |
| Round | `(text)` |
| Stadium | `([text])` |
| Diamond | `{text}` |
| Hexagon | `{{text}}` |
| Cylinder | `[(text)]` |
| Circle | `((text))` |
| Double Circle | `(((text)))` |
| Subroutine | `[[text]]` |
| Parallelogram | `[/text/]` |
| Trapezoid | `[/text\]` |
| Asymmetric | `>text]` |

</details>

<details>
<summary><strong>Edge Styles</strong></summary>

| Type | Syntax | Description |
|:-----|:-------|:------------|
| Arrow | `-->` | Standard arrow |
| Open | `---` | No arrowhead |
| Dotted | `-.->` | Dashed line with arrow |
| Thick | `==>` | Bold arrow |
| Circle end | `--o` | Circle decoration |
| Cross end | `--x` | X decoration |
| Diamond end | `<-->` | Bidirectional |
| With label | `--\|text\|-->` | Labeled edge |

</details>

<details>
<summary><strong>Subgraphs</strong></summary>

```
flowchart TB
    subgraph Frontend
        A[React App] --> B[API Client]
    end
    subgraph Backend
        C[Express Server] --> D[(PostgreSQL)]
    end
    B --> C
```

Subgraphs support:
- Custom labels
- Direction override (`direction LR`)
- Nesting
- Styling

</details>

<details>
<summary><strong>Styling Directives</strong></summary>

```
flowchart LR
    A[Start] --> B[End]

    classDef highlight fill:#f9f,stroke:#333
    class A highlight

    style B fill:#bbf,stroke:#333
    linkStyle 0 stroke:red,stroke-width:2px
```

Supported:
- `classDef` - Define CSS classes
- `class` - Apply classes to nodes
- `:::class` - Inline class syntax
- `style` - Direct node styling
- `linkStyle` - Edge styling
- `%%{init}%%` - Theme configuration

</details>

## Features

**Diagram types:** `flowchart` / `graph` (TD, TB, LR, RL, BT) | `classDiagram` | `stateDiagram-v2` | `sequenceDiagram`

**Node shapes:** rectangle, round-rect, stadium, circle, double-circle, diamond, hexagon, cylinder, subroutine, trapezoid, parallelogram, asymmetric

**Edges:** solid, dotted, thick | Decorations: arrow, circle, cross, diamond | Labels

**Styling:** `classDef`, `class`, `:::class`, `style`, `linkStyle`, `%%{init}%%`

**Layout:** subgraphs with direction, nested subgraphs, automatic spacing

## Configuration

```bash
mmdr -i diagram.mmd -o out.svg -c config.json
mmdr -i diagram.mmd -o out.svg --nodeSpacing 60 --rankSpacing 120
```

<details>
<summary><strong>config.json example</strong></summary>

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

</details>

## How It Works

<img src="docs/diagrams/architecture.svg" alt="Architecture comparison" width="100%">

**mmdr** implements the entire Mermaid pipeline natively:

```
.mmd → parser.rs → ir.rs → layout.rs (dagre_rust) → render.rs → SVG → resvg → PNG
```

**mermaid-cli** requires browser infrastructure:

```
.mmd → mermaid-js → dagre → Browser DOM → Puppeteer → Chromium → Screenshot → PNG
```

| | mmdr | mermaid-cli |
|:--|:-----|:------------|
| Runtime | Native binary | Node.js + Chromium |
| Cold start | ~3 ms | ~2,000 ms |
| Memory | ~15 MB | ~300+ MB |
| Dependencies | None | Node.js, npm, Chromium |

## Development

```bash
cargo test
cargo run -- -i docs/diagrams/architecture.mmd -o /tmp/out.svg -e svg
```

**Benchmarks:**
```bash
cargo bench --bench renderer              # Microbenchmarks
cargo build --release && python scripts/bench_compare.py  # vs mermaid-cli
```

## License

MIT
