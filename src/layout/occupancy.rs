use std::collections::{HashMap, HashSet};

pub(crate) type Rect = (f32, f32, f32, f32);

#[derive(Debug, Clone, Copy)]
pub(crate) struct PlacementBounds {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl PlacementBounds {
    fn clamp_center(self, center: (f32, f32), width: f32, height: f32) -> (f32, f32) {
        let min_x = self.min_x + width / 2.0;
        let max_x = self.max_x - width / 2.0;
        let min_y = self.min_y + height / 2.0;
        let max_y = self.max_y - height / 2.0;
        let x = if min_x <= max_x {
            center.0.clamp(min_x, max_x)
        } else {
            (min_x + max_x) * 0.5
        };
        let y = if min_y <= max_y {
            center.1.clamp(min_y, max_y)
        } else {
            (min_y + max_y) * 0.5
        };
        (x, y)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AnchoredRectPlacement {
    pub center: (f32, f32),
    pub rect: Rect,
}

pub(crate) fn rect_from_center(center: (f32, f32), width: f32, height: f32) -> Rect {
    (
        center.0 - width / 2.0,
        center.1 - height / 2.0,
        width,
        height,
    )
}

pub(crate) fn inflate_rect(rect: Rect, pad_x: f32, pad_y: f32) -> Rect {
    (
        rect.0 - pad_x,
        rect.1 - pad_y,
        rect.2 + pad_x * 2.0,
        rect.3 + pad_y * 2.0,
    )
}

pub(crate) fn rect_overlap_area(a: Rect, b: Rect) -> f32 {
    let x1 = a.0.max(b.0);
    let y1 = a.1.max(b.1);
    let x2 = (a.0 + a.2).min(b.0 + b.2);
    let y2 = (a.1 + a.3).min(b.1 + b.3);
    if x2 <= x1 || y2 <= y1 {
        return 0.0;
    }
    (x2 - x1) * (y2 - y1)
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    (dx * dx + dy * dy).sqrt()
}

pub(crate) struct ObstacleGrid {
    cell: f32,
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl ObstacleGrid {
    pub(crate) fn new(cell: f32, rects: &[Rect]) -> Self {
        let cell = cell.max(16.0);
        let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (i, rect) in rects.iter().enumerate() {
            let x0 = (rect.0 / cell).floor() as i32;
            let y0 = (rect.1 / cell).floor() as i32;
            let x1 = ((rect.0 + rect.2) / cell).floor() as i32;
            let y1 = ((rect.1 + rect.3) / cell).floor() as i32;
            for ix in x0..=x1 {
                for iy in y0..=y1 {
                    cells.entry((ix, iy)).or_default().push(i);
                }
            }
        }
        Self { cell, cells }
    }

    fn query(&self, rect: &Rect) -> impl Iterator<Item = usize> + '_ {
        let x0 = (rect.0 / self.cell).floor() as i32;
        let y0 = (rect.1 / self.cell).floor() as i32;
        let x1 = ((rect.0 + rect.2) / self.cell).floor() as i32;
        let y1 = ((rect.1 + rect.3) / self.cell).floor() as i32;
        let mut seen = HashSet::new();
        (x0..=x1)
            .flat_map(move |ix| (y0..=y1).map(move |iy| (ix, iy)))
            .flat_map(move |key| {
                self.cells
                    .get(&key)
                    .map(|items| items.as_slice())
                    .unwrap_or(&[])
                    .iter()
                    .copied()
            })
            .filter(move |idx| seen.insert(*idx))
    }
}

pub(crate) fn place_anchored_rect(
    width: f32,
    height: f32,
    candidates: &[(f32, f32)],
    occupied: &[Rect],
    bounds: Option<PlacementBounds>,
    anchor: (f32, f32),
    rank_weight: f32,
    distance_weight: f32,
) -> AnchoredRectPlacement {
    let fallback_center = bounds
        .map(|limit| limit.clamp_center(anchor, width, height))
        .unwrap_or(anchor);
    let fallback_rect = rect_from_center(fallback_center, width, height);
    let grid = ObstacleGrid::new(width.max(height).max(32.0), occupied);
    let mut best = AnchoredRectPlacement {
        center: fallback_center,
        rect: fallback_rect,
    };
    let mut best_score = f32::INFINITY;

    for (idx, candidate) in candidates.iter().copied().enumerate() {
        let center = bounds
            .map(|limit| limit.clamp_center(candidate, width, height))
            .unwrap_or(candidate);
        let rect = rect_from_center(center, width, height);
        let mut overlap_area = 0.0f32;
        let mut overlap_count = 0usize;
        for obstacle_idx in grid.query(&rect) {
            let overlap = rect_overlap_area(rect, occupied[obstacle_idx]);
            if overlap > 0.0 {
                overlap_area += overlap;
                overlap_count += 1;
            }
        }
        let score = overlap_count as f32 * 4_000.0
            + overlap_area * 120.0
            + distance(center, anchor) * distance_weight
            + idx as f32 * rank_weight;
        if score < best_score {
            best = AnchoredRectPlacement { center, rect };
            best_score = score;
        }
    }

    best
}
