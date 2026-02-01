# Layout Objective

This document defines the layout objective for the planned overhaul. The goal is not just to make diagrams smaller, but to make them more readable, stable, and consistent across diagram types.

## Primary goal

Produce layouts that are readable, deterministic, and visually consistent while preserving Mermaid semantics.

## Hard constraints (must always hold)

- No node overlaps (including subgraph boundaries).
- Edge paths must not pass through node or subgraph shapes (except at ports).
- Ports must lie on the node boundary and reflect the inferred edge direction.
- Subgraph containment must remain intact.
- Output must be deterministic for a fixed input + config.

## Soft objectives (minimize weighted score)

Weights are initial targets and can be tuned after baseline measurements.

1. Edge crossings (weight 5)
2. Total edge length (weight 2)
3. Edge bends / turns (weight 2)
4. Port congestion on a side (weight 2)
5. Edge overlaps / near-overlaps (weight 1)
6. Diagram area (weight 1)
7. Node displacement vs. prior layout (weight 3, when a prior layout exists)

## Secondary goals

- Preserve user-specified node order and subgraph direction overrides.
- Minimize diff noise between runs (stability).
- Keep runtime within current benchmarks for medium diagrams.

## Evaluation artifacts

- Fixture render smoke tests for every diagram type.
- Benchmarks on all medium fixtures plus flowchart small/large.
- Optional conformance report vs mermaid-cli (image diff + layout diff).
