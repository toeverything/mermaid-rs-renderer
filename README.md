# mermaid-rs-renderer

A minimal Mermaid flowchart renderer in Rust.

## Status
- Supports `flowchart` / `graph` with `TD/TB/LR/BT/RL` and subgraph `direction`
- Node shapes: rectangle, round-rect, stadium, circle/double-circle, diamond, hexagon, cylinder, subroutine, trapezoids, parallelograms
- Edge styles: solid, dotted, thick; arrowheads on start/end; labels
- Styling directives: `classDef`, `class`, inline `:::class`, `style` (nodes + subgraphs), `linkStyle` (fill/stroke/text/stroke-width/dasharray subset)
- Subgraphs (`subgraph ... end`)
- Mermaid init directives: `%%{init}%%` (themeVariables subset, JSON5-style allowed)
- Modern default theme

## Usage

```bash
# Render SVG to stdout
cat diagram.mmd | mmdr -e svg

# Render a Markdown file (all mermaid blocks)
# If output is a directory, renders diagram-1.svg, diagram-2.svg, ...
mmdr -i README.md -o /tmp/diagrams -e svg

# Render to PNG
mmdr -i diagram.mmd -o diagram.png -e png

# Use config file (Mermaid-like themeVariables)
mmdr -i diagram.mmd -o diagram.svg -e svg -c config.json
```

## Config
We accept a subset of Mermaid `themeVariables` in a JSON config file. Example:

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
    "fontFamily": "Inter, Segoe UI, system-ui",
    "fontSize": 13
  },
  "flowchart": {
    "nodeSpacing": 40,
    "rankSpacing": 80
  }
}
```

## Development

```bash
cargo test
cargo run -- -i docs/diagrams/architecture.mmd -o /tmp/arch.svg -e svg
```

## License
MIT
