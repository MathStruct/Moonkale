//! 2D camera: world ↔ screen, pan, zoom-at-pointer, fit, hit-testing.

use crate::graph::Graph;

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// World point at the viewport centre.
    pub cx: f32,
    pub cy: f32,
    /// Pixels per world unit.
    pub scale: f32,
    /// Viewport size in CSS pixels.
    pub width: f32,
    pub height: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self { cx: 0.0, cy: 0.0, scale: 1.0, width: 800.0, height: 600.0 }
    }
}

impl Camera {
    pub fn world_to_screen(&self, x: f32, y: f32) -> (f32, f32) {
        ((x - self.cx) * self.scale + self.width / 2.0, (y - self.cy) * self.scale + self.height / 2.0)
    }

    pub fn screen_to_world(&self, sx: f32, sy: f32) -> (f32, f32) {
        ((sx - self.width / 2.0) / self.scale + self.cx, (sy - self.height / 2.0) / self.scale + self.cy)
    }

    pub fn pan(&mut self, dx_px: f32, dy_px: f32) {
        self.cx -= dx_px / self.scale;
        self.cy -= dy_px / self.scale;
    }

    /// Zoom by `factor` keeping the world point under (sx, sy) fixed.
    pub fn zoom_at(&mut self, factor: f32, sx: f32, sy: f32) {
        let (wx, wy) = self.screen_to_world(sx, sy);
        self.scale = (self.scale * factor).clamp(0.05, 20.0);
        let (nx, ny) = self.screen_to_world(sx, sy);
        self.cx += wx - nx;
        self.cy += wy - ny;
    }

    pub fn fit(&mut self, graph: &Graph, padding: f32) {
        let Some((x0, y0, x1, y1)) = graph.bounds() else { return };
        let w = (x1 - x0).max(1.0);
        let h = (y1 - y0).max(1.0);
        self.cx = (x0 + x1) / 2.0;
        self.cy = (y0 + y1) / 2.0;
        self.scale = ((self.width - 2.0 * padding) / w).min((self.height - 2.0 * padding) / h).clamp(0.05, 4.0);
    }

    /// Index of the node under the screen point, if any (nearest wins).
    pub fn hit(&self, graph: &Graph, sx: f32, sy: f32) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, n) in graph.nodes.iter().enumerate() {
            let (px, py) = self.world_to_screen(n.x, n.y);
            let r = n.radius * self.scale.max(0.6) + 4.0;
            let d2 = (px - sx).powi(2) + (py - sy).powi(2);
            if d2 <= r * r && best.is_none_or(|(_, bd)| d2 < bd) {
                best = Some((i, d2));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Uniform block for the shaders.
    pub fn uniform(&self) -> [f32; 8] {
        [self.cx, self.cy, self.scale, 0.0, self.width, self.height, 0.0, 0.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_point_under_cursor() {
        let mut c = Camera { cx: 10.0, cy: -5.0, scale: 1.0, width: 400.0, height: 300.0 };
        let before = c.screen_to_world(50.0, 60.0);
        c.zoom_at(2.0, 50.0, 60.0);
        let after = c.screen_to_world(50.0, 60.0);
        assert!((before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3);
    }

    #[test]
    fn round_trip() {
        let c = Camera { cx: 3.0, cy: 4.0, scale: 2.5, width: 640.0, height: 480.0 };
        let (sx, sy) = c.world_to_screen(-7.0, 9.0);
        let (wx, wy) = c.screen_to_world(sx, sy);
        assert!((wx + 7.0).abs() < 1e-4 && (wy - 9.0).abs() < 1e-4);
    }
}
