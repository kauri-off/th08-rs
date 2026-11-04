//! View / projection matrices: Camera_Set2D (05 §9), the default and stage cameras (00 §1, 06 §10.1), ANM world matrix.
// Trig on `fmath` (05 §10, step 11p): draw-only, its vertices differ from the old std sin_cos / tan for |a| > 2.

use crate::drawlist::{Rect, SCREEN_H, SCREEN_W};
use crate::fmath::{sin_cos_f, tan_f};

pub type Vec3 = [f32; 3];

// Row-vector convention (p' = p * M, as D3D); the same memory read as WGSL column-major gives M^T, so `m * v` works.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Mat4(pub [[f32; 4]; 4]);

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(a: Vec3) -> Vec3 {
    let l = dot(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);

    pub fn scale(x: f32, y: f32, z: f32) -> Self {
        Mat4([
            [x, 0.0, 0.0, 0.0],
            [0.0, y, 0.0, 0.0],
            [0.0, 0.0, z, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    pub fn rotation_x(a: f32) -> Self {
        let (s, c) = sin_cos_f(a);
        Mat4([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, c, s, 0.0],
            [0.0, -s, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    pub fn rotation_y(a: f32) -> Self {
        let (s, c) = sin_cos_f(a);
        Mat4([
            [c, 0.0, -s, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [s, 0.0, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    pub fn rotation_z(a: f32) -> Self {
        let (s, c) = sin_cos_f(a);
        Mat4([
            [c, s, 0.0, 0.0],
            [-s, c, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    // D3DXMatrixLookAtLH / D3DXMatrixPerspectiveFovLH equivalents (left-handed, row vectors).
    pub fn look_at_lh(eye: Vec3, at: Vec3, up: Vec3) -> Self {
        let z = normalize(sub(at, eye));
        let x = normalize(cross(up, z));
        let y = cross(z, x);
        Mat4([
            [x[0], y[0], z[0], 0.0],
            [x[1], y[1], z[1], 0.0],
            [x[2], y[2], z[2], 0.0],
            [-dot(x, eye), -dot(y, eye), -dot(z, eye), 1.0],
        ])
    }

    pub fn perspective_fov_lh(fovy: f32, aspect: f32, near: f32, far: f32) -> Self {
        let ys = 1.0 / tan_f(fovy / 2.0);
        let xs = ys / aspect;
        let q = far / (far - near);
        Mat4([
            [xs, 0.0, 0.0, 0.0],
            [0.0, ys, 0.0, 0.0],
            [0.0, 0.0, q, 1.0],
            [0.0, 0.0, -near * q, 0.0],
        ])
    }

    pub fn mul(&self, o: &Mat4) -> Mat4 {
        let mut r = [[0.0; 4]; 4];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..4).map(|k| self.0[i][k] * o.0[k][j]).sum();
            }
        }
        Mat4(r)
    }

    pub fn transform(&self, p: Vec3) -> [f32; 4] {
        let m = &self.0;
        let v = [p[0], p[1], p[2], 1.0];
        std::array::from_fn(|j| (0..4).map(|k| v[k] * m[k][j]).sum())
    }
}

// View and projection of one camera: Camera_Set2D (05 §9) or the stage camera (06 §10.1).
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub view: Mat4,
    pub proj: Mat4,
}

impl Camera {
    // Default camera (00 §1): FOV π/6, aspect 4/3, near 100, far 10000.
    pub const FOV: f32 = std::f32::consts::PI / 6.0;
    pub const NEAR: f32 = 100.0;
    pub const FAR: f32 = 10000.0;

    pub fn default_2d() -> Self {
        let d = (SCREEN_H / 2.0) / tan_f(Self::FOV / 2.0);
        let (cx, cy) = (SCREEN_W / 2.0, -SCREEN_H / 2.0);
        Self {
            view: Mat4::look_at_lh([cx, cy, -d], [cx, cy, 0.0], [0.0, 1.0, 0.0]),
            proj: Mat4::perspective_fov_lh(Self::FOV, SCREEN_W / SCREEN_H, Self::NEAR, Self::FAR),
        }
    }

    // Camera_Set2D 0x40B5A0 (05 §9): fovY π/10, near 1, far 10000, up (0, −1, 0); world (x, y, 0) maps to viewport pixel (x, y).
    pub fn set_2d(vp: Rect) -> Self {
        let fov = std::f32::consts::PI / 10.0;
        let (cx, cy) = (vp.w / 2.0, vp.h / 2.0);
        Self {
            view: Mat4::look_at_lh(
                [cx, cy, cy / tan_f(fov / 2.0)],
                [cx, cy, 0.0],
                [0.0, -1.0, 0.0],
            ),
            proj: Mat4::perspective_fov_lh(fov, vp.w / vp.h, 1.0, 10000.0),
        }
    }

    pub fn view_proj(&self) -> Mat4 {
        self.view.mul(&self.proj)
    }

    // World point → (screen x, screen y, depth 0..1) inside viewport `vp`.
    // D3DXVec3Project with world = identity (05 §9 modes 4/5).
    pub fn project(&self, p: Vec3, vp: Rect) -> Vec3 {
        let c = self.view_proj().transform(p);
        let (x, y, z) = (c[0] / c[3], c[1] / c[3], c[2] / c[3]);
        [
            vp.x + (x + 1.0) * vp.w / 2.0,
            vp.y + (1.0 - y) * vp.h / 2.0,
            z,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawlist::{FULLSCREEN, PLAYFIELD};

    #[test]
    fn default_camera_maps_z0_to_pixels() {
        let cam = Camera::default_2d();
        for (x, y) in [
            (0.0, 0.0),
            (320.0, 240.0),
            (640.0, 480.0),
            (32.0, 16.0),
            (416.0, 464.0),
            (100.5, 377.25),
        ] {
            let s = cam.project([x, -y, 0.0], FULLSCREEN);
            assert!(
                (s[0] - x).abs() < 1e-3 && (s[1] - y).abs() < 1e-3,
                "({x},{y}) -> {s:?}"
            );
            assert!(s[2] > 0.0 && s[2] < 1.0);
        }
    }

    #[test]
    fn set_2d_maps_to_viewport_pixels() {
        let cam = Camera::set_2d(PLAYFIELD);
        for (x, y) in [(0.0, 0.0), (192.0, 224.0), (384.0, 448.0), (10.0, 300.0)] {
            let s = cam.project([x, y, 0.0], PLAYFIELD);
            assert!(
                (s[0] - x - 32.0).abs() < 1e-2 && (s[1] - y - 16.0).abs() < 1e-2,
                "({x},{y}) -> {s:?}"
            );
        }
    }
}
