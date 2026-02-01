# Layout Overhaul Plan

## Goal

Build a layout system that scores well against the objective in `docs/layout_objective.md`, with hard guarantees for overlaps and port placement, and measurable improvements in crossing/length/bends.

## Scope

- Flowchart-style diagrams (flowchart, state, class, ER, requirement, packet, journey) are the primary target.
- Other diagram types keep their specialized layout unless they share routing/ports.

## Phase 1: Baseline + Metrics

1. Run fixture render suite for all diagram types.
2. Record baseline timings (parse/layout/render/end-to-end) on all medium fixtures.
3. Run mmdr vs mermaid-cli comparison for the same fixture set.
4. Capture layout dumps where needed for diffs and debugging.

## Phase 2: Scoring

Implement a layout scoring utility (internal-only) to compute:

- Edge crossings count
- Total edge length
- Bend count
- Port congestion by side
- Edge overlap segments
- Layout area

## Phase 3: Architecture

Split flowchart layout into explicit stages:

1. Node size resolution
2. Rank assignment
3. Ordering / crossing minimization
4. Coordinate assignment
5. Port assignment
6. Routing with obstacle avoidance
7. Post-pass normalization

## Phase 4: Implementation

- Replace or upgrade each stage independently with feature flags.
- Keep the old pipeline available for A/B comparison.

## Phase 5: Evaluation

- Use the scoring utility to compare old vs new on every fixture.
- Verify parity using `scripts/conformance_compare.py`.
- Ensure performance is within baseline bounds.
