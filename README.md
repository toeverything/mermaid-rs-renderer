# mermaid-rs-renderer

A minimal Mermaid flowchart renderer in Rust.

## Status
- Supports `flowchart` / `graph` with `TD/TB/LR`
- Nodes with `[]` or `()` labels
- Directed (`-->`) and undirected (`---`) edges
- Subgraphs (`subgraph ... end`)
- Modern default theme

## Usage

```bash
# Render SVG to stdout
cat diagram.mmd | mmdr -e svg

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
