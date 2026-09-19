//! Barnes–Hut quadtree for the repulsion term: O(n log n) instead of O(n²).
//! Rebuilt every layout step (building is cheap next to the pair loop it
//! replaces). Works on every backend — the point of doing it on the CPU
//! before any compute shader (P-22).

/// One cell: either a leaf holding a body index or an inner node with four
/// children. Mass and centre of mass are accumulated for the approximation.
#[derive(Clone, Debug)]
struct Cell {
    x0: f32,
    y0: f32,
    size: f32,
    mass: f32,
    cx: f32,
    cy: f32,
    body: Option<usize>,
    children: Option<[usize; 4]>,
}

pub struct QuadTree {
    cells: Vec<Cell>,
}

impl QuadTree {
    /// Build over the points (bodies of mass 1).
    pub fn build(points: &[(f32, f32)]) -> Self {
        let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in points {
            minx = minx.min(x);
            miny = miny.min(y);
            maxx = maxx.max(x);
            maxy = maxy.max(y);
        }
        if points.is_empty() {
            return Self { cells: Vec::new() };
        }
        let size = (maxx - minx).max(maxy - miny).max(1.0) * 1.001;
        let mut tree = Self {
            cells: vec![Cell {
                x0: minx,
                y0: miny,
                size,
                mass: 0.0,
                cx: 0.0,
                cy: 0.0,
                body: None,
                children: None,
            }],
        };
        for (i, &(x, y)) in points.iter().enumerate() {
            tree.insert(0, i, x, y, points, 0);
        }
        tree
    }

    fn insert(
        &mut self,
        cell: usize,
        body: usize,
        x: f32,
        y: f32,
        points: &[(f32, f32)],
        depth: u32,
    ) {
        // Accumulate mass on the way down.
        {
            let c = &mut self.cells[cell];
            let m = c.mass + 1.0;
            c.cx = (c.cx * c.mass + x) / m;
            c.cy = (c.cy * c.mass + y) / m;
            c.mass = m;
        }
        if self.cells[cell].children.is_none() {
            match self.cells[cell].body {
                None => {
                    self.cells[cell].body = Some(body);
                    return;
                }
                Some(other) => {
                    // Coincident bodies (or too deep): keep as a heavier leaf.
                    if depth > 24 {
                        return;
                    }
                    let (ox, oy) = points[other];
                    self.subdivide(cell);
                    self.cells[cell].body = None;
                    let q = self.quadrant(cell, ox, oy);
                    let child = self.cells[cell].children.unwrap()[q];
                    self.insert(child, other, ox, oy, points, depth + 1);
                }
            }
        }
        let q = self.quadrant(cell, x, y);
        let child = self.cells[cell].children.unwrap()[q];
        self.insert(child, body, x, y, points, depth + 1);
    }

    fn subdivide(&mut self, cell: usize) {
        let (x0, y0, half) = {
            let c = &self.cells[cell];
            (c.x0, c.y0, c.size / 2.0)
        };
        let base = self.cells.len();
        for (dx, dy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            self.cells.push(Cell {
                x0: x0 + dx * half,
                y0: y0 + dy * half,
                size: half,
                mass: 0.0,
                cx: 0.0,
                cy: 0.0,
                body: None,
                children: None,
            });
        }
        self.cells[cell].children = Some([base, base + 1, base + 2, base + 3]);
    }

    fn quadrant(&self, cell: usize, x: f32, y: f32) -> usize {
        let c = &self.cells[cell];
        let half = c.size / 2.0;
        let right = x >= c.x0 + half;
        let down = y >= c.y0 + half;
        (right as usize) + 2 * (down as usize)
    }

    /// Repulsive force on body `i` at `(x, y)` with strength `k2 / d`,
    /// approximating far cells (`size / d < theta`) by their centre of mass.
    pub fn force(&self, i: usize, x: f32, y: f32, k2: f32, theta: f32) -> (f32, f32) {
        let (mut fx, mut fy) = (0.0f32, 0.0f32);
        if self.cells.is_empty() {
            return (fx, fy);
        }
        let mut stack = vec![0usize];
        while let Some(cell) = stack.pop() {
            let c = &self.cells[cell];
            if c.mass == 0.0 {
                continue;
            }
            let dx = x - c.cx;
            let dy = y - c.cy;
            let d2 = dx * dx + dy * dy;
            let is_self_leaf = c.body == Some(i) && c.children.is_none();
            if is_self_leaf {
                continue;
            }
            let d = d2.sqrt();
            if c.children.is_none() || c.size / d.max(1e-3) < theta {
                // Leaf or far enough: treat as one body of `mass`.
                let (dx, dy, d) = if d2 < 0.01 {
                    let n = (i as f32 + 1.0) * 0.1;
                    (n, n * 0.7, (n * n * 1.49).sqrt())
                } else {
                    (dx, dy, d)
                };
                let f = c.mass * k2 / d;
                fx += dx / d * f;
                fy += dy / d * f;
            } else if let Some(children) = c.children {
                stack.extend(children);
            }
        }
        (fx, fy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_exact_pairs_on_a_small_set() {
        let pts: Vec<(f32, f32)> = (0..40)
            .map(|i| ((i * 37 % 100) as f32, (i * 53 % 100) as f32))
            .collect();
        let tree = QuadTree::build(&pts);
        let k2 = 100.0;
        for (i, &(x, y)) in pts.iter().enumerate() {
            let (mut ex, mut ey) = (0.0, 0.0);
            for (j, &(ox, oy)) in pts.iter().enumerate() {
                if i == j {
                    continue;
                }
                let (dx, dy) = (x - ox, y - oy);
                let d = (dx * dx + dy * dy).sqrt().max(0.1);
                ex += dx / d * k2 / d;
                ey += dy / d * k2 / d;
            }
            // theta = 0 → exact.
            let (fx, fy) = tree.force(i, x, y, k2, 0.0);
            assert!(
                (fx - ex).abs() < 1e-2 && (fy - ey).abs() < 1e-2,
                "{i}: {fx},{fy} vs {ex},{ey}"
            );
            // theta = 0.8 → close.
            let (ax, ay) = tree.force(i, x, y, k2, 0.8);
            let err = ((ax - ex).powi(2) + (ay - ey).powi(2)).sqrt();
            let mag = (ex * ex + ey * ey).sqrt().max(1.0);
            assert!(err / mag < 0.25, "{i}: approx error {err} of {mag}");
        }
    }
}
