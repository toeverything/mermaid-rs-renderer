# Architecture

## Scope
- Supports **12+ diagram types**: flowchart, sequence, class, state, ER, pie, gantt, journey, timeline, mindmap, gitGraph, xychart, quadrant.
- Support Mermaid init directives: `%%{init}%%` + per-diagram config.
- Deterministic output for stable diffs.

## Core pipeline decisions
- Parser: use a Rust parser generator (candidate: `pest` or `nom`).
- IR: typed nodes/edges/subgraphs, with explicit label + style objects.
- Layout: layered DAG layout (ranking + crossing minimization) with a dedicated routing phase.
- Routing: obstacle-aware orthogonal routing with a grid A* router + heuristic fallback, using
  a shared occupancy grid to reduce edge-edge overlap.
- Rendering: generate SVG directly; rasterize PNG/PDF via `resvg`/`tiny-skia`.

## CLI decisions
- `mmdc`-compatible flags where possible: `-i`, `-o`, `-t`, `-c`, `-w`, `-H`.
- Config merge order: CLI flags > init directives > config file defaults.
- Zero network access by default.

## Theming decisions
- Map Mermaid theme variables into a Rust `Theme` struct.
- Emit CSS variables into the SVG for easy overrides.

## Extensibility decisions
- Add a diagram-type registry for future expansions.
- Keep parsing + layout isolated per diagram type.

## Flowchart routing architecture
- **Port assignment**: per-node side ordering based on target alignment; offsets can snap to
  the routing grid to stabilize edge paths.
- **Rank ordering**: median-based ordering per layer with damped barycentric placement to
  reduce crossings and keep node structure stable.
- **Obstacles**: nodes + visible subgraphs expanded by padding; edges avoid these regions.
- **Global routing**: a grid A* router searches orthogonal paths with turn penalties and
  occupancy cost; if no grid path is found (or the grid is too large), heuristic
  candidates are used.
- **Occupancy**: routed edges mark a shared grid to discourage later overlaps while keeping
  routing deterministic.

## Diagrams
- `docs/diagrams/architecture.mmd`
- `docs/diagrams/pipeline.mmd`
