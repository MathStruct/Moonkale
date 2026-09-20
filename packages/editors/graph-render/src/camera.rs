//! Camera: 2D (world ↔ screen, pan, zoom-at-pointer, fit, hit-testing) and,
//! since Milestone 8, a 3D mode — an orbiting perspective camera around a
//! target, with the same public operations. The shaders get one uniform
//! block carrying both.

use crate::graph::Graph;

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// World point at the viewport centre (2D) / orbit target (3D).
    pub cx: f32,
    pub cy: f32,
    pub cz: f32,
    /// Pixels per world unit (2D).
    pub scale: f32,
    /// Viewport size in CSS pixels.
    pub width: f32,
    pub height: f32,
    /// 3D mode (Milestone 8).
    pub three_d: bool,
    /// Orbit angles (radians) and distance to the target.
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            cx: 0.0,
            cy: 0.0,
            cz: 0.0,
            scale: 1.0,
            width: 800.0,
            height: 600.0,
            three_d: false,
            yaw: 0.6,
            pitch: 0.8,
            dist: 1200.0,
        }
    }
}

/// Column-major 4×4 (wgsl `mat4x4<f32>` layout).
pub type Mat4 = [f32; 16];

fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut m = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            m[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    m
}

fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y / 2.0).tan();
    let mut m = [0.0; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = far / (near - far);
    m[11] = -1.0;
    m[14] = near * far / (near - far);
    m
}

fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> Mat4 {
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let norm = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let f = norm(sub(target, eye));
    let s = norm(cross(f, up));
    let u = cross(s, f);
    [
        s[0],
        u[0],
        -f[0],
        0.0,
        s[1],
        u[1],
        -f[1],
        0.0,
        s[2],
        u[2],
        -f[2],
        0.0,
        -dot(s, eye),
        -dot(u, eye),
        dot(f, eye),
        1.0,
    ]
}

impl Camera {
    /// Where the eye is in 3D mode.
    pub fn eye(&self) -> [f32; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        [
            self.cx + self.dist * cp * sy,
            self.cy - self.dist * sp,
            self.cz + self.dist * cp * cy,
        ]
    }

    /// The 3D view-projection matrix (identity-ish in 2D; unused there).
    pub fn view_proj(&self) -> Mat4 {
        let aspect = (self.width / self.height.max(1.0)).max(0.1);
        let near = (self.dist * 0.05).max(1.0);
        let far = self.dist * 20.0 + 4000.0;
        let proj = perspective(50f32.to_radians(), aspect, near, far);
        let view = look_at(self.eye(), [self.cx, self.cy, self.cz], [0.0, -1.0, 0.0]);
        mul(&proj, &view)
    }

    /// Screen position (and clip w = depth) of a world point in 3D; `None`
    /// behind the camera.
    pub fn project(&self, x: f32, y: f32, z: f32) -> Option<(f32, f32, f32)> {
        let m = self.view_proj();
        let cx = m[0] * x + m[4] * y + m[8] * z + m[12];
        let cy = m[1] * x + m[5] * y + m[9] * z + m[13];
        let cw = m[3] * x + m[7] * y + m[11] * z + m[15];
        if cw <= 1e-4 {
            return None;
        }
        Some((
            (cx / cw + 1.0) * 0.5 * self.width,
            (1.0 - cy / cw) * 0.5 * self.height,
            cw,
        ))
    }

    /// Camera basis in 3D: forward, right, up (unit vectors, y-down world).
    fn basis(&self) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let eye = self.eye();
        let t = [self.cx, self.cy, self.cz];
        let norm = |v: [f32; 3]| {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
            [v[0] / l, v[1] / l, v[2] / l]
        };
        let cross = |a: [f32; 3], b: [f32; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let f = norm([t[0] - eye[0], t[1] - eye[1], t[2] - eye[2]]);
        let s = norm(cross(f, [0.0, -1.0, 0.0]));
        let u = cross(s, f);
        (f, s, u)
    }

    /// The world point under screen (sx, sy) at view depth `w` (the clip w
    /// [`project`](Self::project) returned): the inverse for dragging a node
    /// in its own depth plane (Milestone 9).
    pub fn unproject(&self, sx: f32, sy: f32, w: f32) -> (f32, f32, f32) {
        let eye = self.eye();
        let (f, r, u) = self.basis();
        let aspect = (self.width / self.height.max(1.0)).max(0.1);
        let t = (25f32.to_radians()).tan();
        let xn = sx / self.width * 2.0 - 1.0;
        let yn = 1.0 - sy / self.height * 2.0;
        let kx = xn * aspect * t * w;
        let ky = yn * t * w;
        (
            eye[0] + f[0] * w + r[0] * kx + u[0] * ky,
            eye[1] + f[1] * w + r[1] * kx + u[1] * ky,
            eye[2] + f[2] * w + r[2] * kx + u[2] * ky,
        )
    }

    /// Screen-space size factor for a node radius in 3D (nearer = bigger).
    pub fn size_factor(&self, w: f32) -> f32 {
        (self.dist / w.max(1.0)).clamp(0.2, 4.0)
    }

    pub fn world_to_screen(&self, x: f32, y: f32) -> (f32, f32) {
        (
            (x - self.cx) * self.scale + self.width / 2.0,
            (y - self.cy) * self.scale + self.height / 2.0,
        )
    }

    pub fn screen_to_world(&self, sx: f32, sy: f32) -> (f32, f32) {
        (
            (sx - self.width / 2.0) / self.scale + self.cx,
            (sy - self.height / 2.0) / self.scale + self.cy,
        )
    }

    /// Pan by screen pixels: in 3D the target moves in the camera's plane.
    pub fn pan(&mut self, dx_px: f32, dy_px: f32) {
        if self.three_d {
            let (sy, cy) = self.yaw.sin_cos();
            let (sp, cp) = self.pitch.sin_cos();
            // Camera right = (cos yaw, 0, -sin yaw); camera up ≈ (-sin yaw·sin pitch, -cos pitch, -cos yaw·sin pitch) in our y-down world.
            let k = self.dist / self.height.max(1.0) * 1.2;
            let right = [cy, 0.0, -sy];
            let up = [-sy * sp, -cp, -cy * sp];
            self.cx -= (right[0] * dx_px - up[0] * dy_px) * k;
            self.cy -= (right[1] * dx_px - up[1] * dy_px) * k;
            self.cz -= (right[2] * dx_px - up[2] * dy_px) * k;
        } else {
            self.cx -= dx_px / self.scale;
            self.cy -= dy_px / self.scale;
        }
    }

    /// Orbit (3D): drag in pixels → yaw/pitch.
    pub fn orbit(&mut self, dx_px: f32, dy_px: f32) {
        self.yaw -= dx_px * 0.006;
        self.pitch = (self.pitch + dy_px * 0.006).clamp(-1.5, 1.5);
    }

    /// Zoom by `factor` keeping the world point under (sx, sy) fixed (2D);
    /// dolly toward the target in 3D.
    pub fn zoom_at(&mut self, factor: f32, sx: f32, sy: f32) {
        if self.three_d {
            self.dist = (self.dist / factor).clamp(40.0, 60_000.0);
            return;
        }
        let (wx, wy) = self.screen_to_world(sx, sy);
        self.scale = (self.scale * factor).clamp(0.05, 20.0);
        let (nx, ny) = self.screen_to_world(sx, sy);
        self.cx += wx - nx;
        self.cy += wy - ny;
    }

    pub fn fit(&mut self, graph: &Graph, padding: f32) {
        let Some((x0, y0, x1, y1)) = graph.fit_bounds() else {
            return;
        };
        let w = (x1 - x0).max(1.0);
        let h = (y1 - y0).max(1.0);
        self.cx = (x0 + x1) / 2.0;
        self.cy = (y0 + y1) / 2.0;
        let (z0, z1) = graph
            .nodes
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), n| (a.min(n.z), b.max(n.z)));
        self.cz = if z0 <= z1 { (z0 + z1) / 2.0 } else { 0.0 };
        self.scale = ((self.width - 2.0 * padding) / w)
            .min((self.height - 2.0 * padding) / h)
            .clamp(0.05, 12.0);
        // 3D: far enough that the bounding sphere fits the 50° field of view.
        let radius = (w * w + h * h + (z1 - z0).max(0.0).powi(2)).sqrt() / 2.0;
        self.dist = (radius / (25f32.to_radians()).sin() * 1.1).clamp(200.0, 60_000.0);
    }

    /// Index of the node under the screen point, if any (nearest wins).
    pub fn hit(&self, graph: &Graph, sx: f32, sy: f32) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, n) in graph.nodes.iter().enumerate() {
            let (px, py, r) = if self.three_d {
                let Some((px, py, w)) = self.project(n.x, n.y, n.z) else {
                    continue;
                };
                (px, py, n.radius * self.size_factor(w) + 4.0)
            } else {
                let (px, py) = self.world_to_screen(n.x, n.y);
                (px, py, n.radius * self.scale.max(0.6) + 4.0)
            };
            let d2 = (px - sx).powi(2) + (py - sy).powi(2);
            if d2 <= r * r && best.is_none_or(|(_, bd)| d2 < bd) {
                best = Some((i, d2));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Uniform block for the shaders: 2D fields, mode, viewport, then the
    /// 3D view-projection matrix and the distance (for size attenuation).
    pub fn uniform(&self) -> [f32; 28] {
        let mut u = [0.0f32; 28];
        u[0] = self.cx;
        u[1] = self.cy;
        u[2] = self.scale;
        u[3] = if self.three_d { 1.0 } else { 0.0 };
        u[4] = self.width;
        u[5] = self.height;
        u[6] = self.dist;
        u[7] = 0.0;
        u[8..24].copy_from_slice(&self.view_proj());
        u
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_the_point_under_the_cursor() {
        let mut c = Camera {
            width: 800.0,
            height: 600.0,
            ..Default::default()
        };
        let (wx, wy) = c.screen_to_world(100.0, 50.0);
        c.zoom_at(2.0, 100.0, 50.0);
        let (nx, ny) = c.screen_to_world(100.0, 50.0);
        assert!((wx - nx).abs() < 1e-3 && (wy - ny).abs() < 1e-3);
    }

    #[test]
    fn projection_puts_the_target_at_the_centre_and_nearer_is_bigger() {
        let c = Camera {
            three_d: true,
            width: 800.0,
            height: 600.0,
            ..Default::default()
        };
        let (sx, sy, w) = c.project(0.0, 0.0, 0.0).unwrap();
        assert!(
            (sx - 400.0).abs() < 0.5 && (sy - 300.0).abs() < 0.5,
            "{sx} {sy}"
        );
        assert!((w - c.dist).abs() < 1.0);
        let eye = c.eye();
        // A point halfway to the eye is nearer (smaller w) and drawn bigger.
        let (_, _, w2) = c.project(eye[0] / 2.0, eye[1] / 2.0, eye[2] / 2.0).unwrap();
        assert!(w2 < w && c.size_factor(w2) > c.size_factor(w));
        // Behind the camera: nothing.
        assert!(c
            .project(eye[0] * 2.0, eye[1] * 2.0, eye[2] * 2.0)
            .is_none());
    }

    #[test]
    fn unproject_inverts_project_at_the_same_depth() {
        let c = Camera {
            three_d: true,
            width: 800.0,
            height: 600.0,
            yaw: 0.9,
            pitch: 0.4,
            ..Default::default()
        };
        for p in [
            [100.0, -40.0, 70.0],
            [-300.0, 200.0, -140.0],
            [0.0, 0.0, 0.0],
        ] {
            let (sx, sy, w) = c.project(p[0], p[1], p[2]).unwrap();
            let (x, y, z) = c.unproject(sx, sy, w);
            assert!(
                (x - p[0]).abs() < 0.5 && (y - p[1]).abs() < 0.5 && (z - p[2]).abs() < 0.5,
                "{p:?} → {x} {y} {z}"
            );
        }
    }
}
