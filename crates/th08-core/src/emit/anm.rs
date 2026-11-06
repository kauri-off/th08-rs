//! ANM emit half: DrawByMode, EmitQuad, Draw3DWorld and ring-strip / billboard draws over `&AnmManager` (05 §9).

use crate::anmmgr::{AnmManager, SlotSprite, Tint, length3};
use crate::anmvm::{self, AnmVm};
use crate::camera::Camera;
use crate::drawlist::{self, Blend, Recorder, Vertex};

// Linear vertex fog (06 §10.1): f = clamp((far − z) / (far − near), 0, 1) on eye-space z (no range fog); RGB bytes.
#[derive(Clone, Copy, Debug)]
pub struct Fog {
    pub near: f32,
    pub far: f32,
    pub color: [u8; 3],
}

pub fn emit(anm: &AnmManager, rec: &mut Recorder, vm: &AnmVm) {
    emit_mode_z(anm, rec, vm, vm.draw_mode, vm.pos[2]);
}

pub fn emit_mode(anm: &AnmManager, rec: &mut Recorder, vm: &AnmVm, mode: i32) {
    emit_mode_z(anm, rec, vm, mode, vm.pos[2]);
}

// Colour or colour2 (bit 17), then the global tint; returns RGBA.
fn vm_color(anm: &AnmManager, vm: &AnmVm) -> [u8; 4] {
    use anmvm::*;
    let c = if vm.has(VmFlags::COLOR2) {
        vm.color2
    } else {
        vm.color
    };
    anm.tinted([c[R], c[G], c[B], c[A]])
}

// Emit of `vm` under the tint a draw-phase step saw before it turned the global tint off (None: as is).
pub fn emit_tinted(
    anm: &AnmManager,
    rec: &mut Recorder,
    vm: &AnmVm,
    mode: i32,
    z: f32,
    t: Option<Tint>,
) {
    match t {
        Some(t) => {
            debug_assert!(!anm.tint_on, "a saved tint applied under the global tint");
            emit_mode_z(anm, rec, &t.vm(vm), mode, z);
        }
        None => emit_mode_z(anm, rec, vm, mode, z),
    }
}

// Blend bits 4–5: 0 alpha, 1 additive, 2/3 set nothing.
pub fn blend_for(rec: &mut Recorder, vm: &AnmVm) -> Blend {
    let blend = match vm.blend() {
        0 => Blend::Alpha,
        1 => Blend::Add,
        _ => rec.anm.last_blend,
    };
    rec.anm.last_blend = blend;
    blend
}

// Emit half of AnmManager_Draw3DWorld 0x464470 (after `prepare` mode 5): unit quad ±128 through `cam` with vertex
// fog; the screen-shake offset is added to the translation.
pub fn emit_3d_world(
    anm: &AnmManager,
    rec: &mut Recorder,
    vm: &AnmVm,
    cam: &Camera,
    fog: Option<Fog>,
) {
    use anmvm::*;
    let Some(spr) = anm.drawable(vm) else {
        return;
    };
    let (hw, hh) = (
        (vm.sprite_w * vm.scale[0] / 2.0).abs(),
        (vm.sprite_h * vm.scale[1] / 2.0).abs(),
    );
    let ax = if vm.has(VmFlags::ANCHOR_X) { hw } else { 0.0 };
    let ay = if vm.has(VmFlags::ANCHOR_Y) { hh } else { 0.0 };
    let mut w = vm.world_matrix;
    w.0[3] = [
        vm.pos[0] + ax + anm.screen_offset[0],
        vm.pos[1] + ay + anm.screen_offset[1],
        vm.pos[2],
        1.0,
    ];
    // Texture matrix offset is re-uploaded only for a new sprite or a non-zero u offset (v-only scroll quirk).
    let key = (vm.anm_slot, vm.sprite_index);
    // The texture is re-bound only on the same cache miss, so a 2D draw in between can leave another bound.
    let (off, tex) = match (rec.anm.last_tex_matrix, rec.anm.bound_tex) {
        (Some((k, o)), Some(t)) if k == key && vm.uv_off[0] == 0.0 => (o, t),
        _ => (
            [spr.uv[0] + vm.uv_off[0], spr.uv[1] + vm.uv_off[1]],
            spr.tex,
        ),
    };
    rec.anm.bound_tex = Some(tex);
    rec.anm.last_tex_matrix = Some((key, off));
    let (su, sv) = (spr.uv[2] - spr.uv[0], spr.uv[3] - spr.uv[1]);
    let color = vm_color(anm, vm);
    let vp = rec.viewport();
    let vproj = cam.view_proj();
    let v = [
        [-128.0, -128.0, 0.0, 0.0],
        [128.0, -128.0, 1.0, 0.0],
        [-128.0, 128.0, 0.0, 1.0],
        [128.0, 128.0, 1.0, 1.0],
    ]
    .map(|[x, y, u, t]| {
        let p = w.transform([x, y, 0.0]);
        let p = [p[0], p[1], p[2]];
        let (fog_color, f) = match fog {
            Some(fg) => {
                let z = cam.view.transform(p)[2];
                let [r, g, b] = fg.color;
                (
                    [r, g, b, 255],
                    ((fg.far - z) / (fg.far - fg.near)).clamp(0.0, 1.0),
                )
            }
            None => ([0; 4], 1.0),
        };
        Vertex {
            pos: drawlist::clip_in_viewport(vproj.transform(p), vp),
            uv: [off[0] + u * su, off[1] + t * sv],
            color,
            fog_color,
            fog: f,
        }
    });
    let blend = blend_for(rec, vm);
    rec.set_log_fog(fog.map(|f| drawlist::LogFog {
        color: f.color,
        start: f.near,
        end: f.far,
    }));
    rec.strip(tex, blend, !vm.has(VmFlags::ZWRITE_OFF), v);
    rec.set_log_fog(None);
}

// Anm_DrawRingStrip (07 §10.6 trail, flag 8): one triangle strip of (pos, uv, alpha) vertices with the VM's
// texture, blend and colour, emitted as consecutive 4-vertex strips.
pub fn draw_ring_strip(
    anm: &AnmManager,
    rec: &mut Recorder,
    vm: &AnmVm,
    v: &[([f32; 2], [f32; 2], u8)],
) {
    use anmvm::*;
    let Some(&spr) = anm.sprite(
        table_index(i32::from(vm.anm_slot)),
        table_index(i32::from(vm.sprite_index)),
    ) else {
        return;
    };
    let c = if vm.has(VmFlags::COLOR2) {
        vm.color2
    } else {
        vm.color
    };
    let blend = blend_for(rec, vm);
    let off = anm.screen_offset;
    let vert = |&([x, y], uv, a): &([f32; 2], [f32; 2], u8)| {
        let rgba = anm.tinted([c[R], c[G], c[B], a]);
        Vertex::screen(x + off[0] + 0.5, y + off[1] + 0.5, 0.0, uv, rgba)
    };
    let v: Vec<Vertex> = v.iter().map(vert).collect();
    for q in (0..v.len().saturating_sub(2)).step_by(2) {
        if q + 3 < v.len() {
            rec.strip(
                spr.tex,
                blend,
                !vm.has(VmFlags::ZWRITE_OFF),
                [v[q], v[q + 1], v[q + 2], v[q + 3]],
            );
        }
    }
    rec.anm.bound_tex = Some(spr.tex);
}

// Emit half of AnmManager_DrawByMode 0x45E960: skip unless visible, enabled and primary alpha ≠ 0; `z` is the
// vertex depth (projected depth for stage billboards, 0 otherwise); mode 5 uses the world matrix `prepare` left.
pub fn emit_mode_z(anm: &AnmManager, rec: &mut Recorder, vm: &AnmVm, mode: i32, z: f32) {
    emit_mode_z_at(anm, rec, vm, mode, z, anm.screen_offset);
}

// As `emit_mode_z` with the EmitQuad global offset `off` the draw saw (a step zeroed `screen_offset` since).
pub fn emit_mode_z_at(
    anm: &AnmManager,
    rec: &mut Recorder,
    vm: &AnmVm,
    mode: i32,
    z: f32,
    off: [f32; 2],
) {
    use anmvm::*;
    let Some(spr) = anm.drawable(vm) else {
        return;
    };
    let (hw, hh) = (
        vm.sprite_w * vm.scale[0] / 2.0,
        vm.sprite_h * vm.scale[1] / 2.0,
    );
    let [px, py, pz] = vm.pos;
    let ax = if vm.has(VmFlags::ANCHOR_X) { hw } else { 0.0 };
    let ay = if vm.has(VmFlags::ANCHOR_Y) { hh } else { 0.0 };
    let axis_aligned = || {
        let (x0, y0) = (px - hw + ax, py - hh + ay);
        let (x1, y1) = (px + hw + ax, py + hh + ay);
        [[x0, y0], [x1, y0], [x0, y1], [x1, y1]]
    };
    // Modes 2/3 with rotation z ≠ 0: corners rotated, the anchor shift added afterwards unrotated.
    let rotated = || {
        let (s, c) = crate::fmath::sin_cos_f(vm.rotation[2]);
        [(-hw, -hh), (hw, -hh), (-hw, hh), (hw, hh)]
            .map(|(dx, dy)| [dx * c - dy * s + px + ax, dx * s + dy * c + py + ay])
    };
    // Mode 4 sizes the quad from the projection; both 3D modes carry the projected z as vertex depth.
    let mut depth = [z; 4];
    let stage_cam = vm.has(VmFlags::STAGE_CAM);
    let cam = |anm: &AnmManager, vp| match (stage_cam, anm.stage_camera) {
        (true, Some(c)) => c,
        _ => Camera::set_2d(vp),
    };
    let (corners, snap) = match mode {
        // 0 Draw2DSnap 0x462FF0, 1 Draw2D 0x463800, 2 Draw2DRot 0x463210 (snaps only when unrotated),
        // 3 Draw2DRotNoSnap 0x463470.
        0 => (axis_aligned(), true),
        1 => (axis_aligned(), false),
        2 | 3 if vm.rotation[2] == 0.0 => (axis_aligned(), mode == 2),
        2 | 3 => (rotated(), false),
        // 4 DrawBillboard 0x463CF0: never reached through the dispatch (05 §9 reachability), kept for completeness.
        4 => {
            let vp = rec.viewport();
            let cam = cam(anm, vp);
            let s = cam.project([px, py, pz], vp);
            if !(0.0..=1.0).contains(&s[2]) {
                return;
            }
            let r = anm.cam_right;
            let len = length3(sub3(cam.project([px + r[0], py + r[1], pz + r[2]], vp), s));
            depth = [s[2]; 4];
            (billboard_corners(vm, s, len), false)
        }
        // 5 Draw3DQuad 0x464070; the anchor shift uses the absolute half size.
        5 => {
            let mut w = vm.world_matrix;
            w.0[3] = [px + ax.abs(), py + ay.abs(), pz, 1.0];
            let vp = rec.viewport();
            let cam = cam(anm, vp);
            let projected = [
                [-128.0, -128.0],
                [128.0, -128.0],
                [-128.0, 128.0],
                [128.0, 128.0],
            ]
            .map(|[x, y]| {
                let p = w.transform([x, y, 0.0]);
                cam.project([p[0], p[1], p[2]], vp)
            });
            // EmitQuad never rewrites z: the projected z is the vertex depth (≈ 0.9994 for z = 0, 2D camera).
            depth = projected.map(|s| s[2]);
            (projected.map(|s| [s[0], s[1]]), false)
        }
        _ => return,
    };
    emit_quad_at(anm, rec, vm, spr, corners, depth, snap, off);
}

// DrawBillboard 0x463CF0 (13 §2.6c) from an already projected point `s` and the 3D length `len` of
// project(pos + right) − s; list B moves `s` in screen space (callback 0x428310) between the two.
pub fn draw_billboard(anm: &AnmManager, rec: &mut Recorder, vm: &AnmVm, s: [f32; 3], len: f32) {
    let Some(spr) = anm.drawable(vm) else {
        return;
    };
    emit_quad(
        anm,
        rec,
        vm,
        spr,
        billboard_corners(vm, s, len),
        [s[2]; 4],
        false,
    );
}

fn emit_quad(
    anm: &AnmManager,
    rec: &mut Recorder,
    vm: &AnmVm,
    spr: SlotSprite,
    corners: [[f32; 2]; 4],
    depth: [f32; 4],
    snap: bool,
) {
    emit_quad_at(anm, rec, vm, spr, corners, depth, snap, anm.screen_offset);
}

#[allow(clippy::too_many_arguments)]
fn emit_quad_at(
    anm: &AnmManager,
    rec: &mut Recorder,
    vm: &AnmVm,
    spr: SlotSprite,
    corners: [[f32; 2]; 4],
    depth: [f32; 4],
    snap: bool,
    off: [f32; 2],
) {
    use anmvm::*;
    // AnmManager_EmitQuad 0x4628B0: global offset, optional snapping, then the D3D pixel centre (+0.5) for wgpu.
    let corners = corners.map(|[x, y]| {
        let (mut x, mut y) = (x + off[0], y + off[1]);
        // Snap: x87 round-to-nearest-even, minus 0.5.
        if snap {
            x = x.round_ties_even() - 0.5;
            y = y.round_ties_even() - 0.5;
        }
        [x + 0.5, y + 0.5]
    });
    let vp = rec.viewport();
    let (minx, maxx) = corners
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), c| (a.min(c[0]), b.max(c[0])));
    let (miny, maxy) = corners
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), c| (a.min(c[1]), b.max(c[1])));
    // Cull unless the bounding box meets the viewport, in D3D coordinates (undo the +0.5).
    let (minx, maxx, miny, maxy) = (minx - 0.5, maxx - 0.5, miny - 0.5, maxy - 0.5);
    if maxx < vp.x || minx > vp.x + vp.w || maxy < vp.y || miny > vp.y + vp.h {
        return;
    }
    // EmitQuad step 2: v0 (u0,v0), v1 (u1,v0), v2 (u0,v1), v3 (u1,v1).
    let (u0, v0) = (spr.uv[0] + vm.uv_off[0], spr.uv[1] + vm.uv_off[1]);
    let (u1, v1) = (spr.uv[2] + vm.uv_off[0], spr.uv[3] + vm.uv_off[1]);
    let rgba = vm_color(anm, vm);
    let blend = blend_for(rec, vm);
    let uv = [[u0, v0], [u1, v0], [u0, v1], [u1, v1]];
    let v = std::array::from_fn(|i| {
        Vertex::screen(corners[i][0], corners[i][1], depth[i], uv[i], rgba)
    });
    rec.anm.bound_tex = Some(spr.tex);
    rec.strip(spr.tex, blend, !vm.has(VmFlags::ZWRITE_OFF), v);
}

// Billboard corners (13 §2.6c): half size ((len·0.5)·sprite_w)·scale.x, rotated by rotation.z about s.
fn billboard_corners(vm: &AnmVm, s: [f32; 3], len: f32) -> [[f32; 2]; 4] {
    let hw = ((len * 0.5) * vm.sprite_w) * vm.scale[0];
    let hh = ((len * 0.5) * vm.sprite_h) * vm.scale[1];
    let (sn, cs) = crate::fmath::sin_cos_f(vm.rotation[2]);
    [(-hw, -hh), (hw, -hh), (-hw, hh), (hw, hh)]
        .map(|(dx, dy)| [dx * cs - dy * sn + s[0], dx * sn + dy * cs + s[1]])
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    // A saved tint applied by the emit (global tint off) records what the draw under the global tint records, also
    // with colour2 and a tint alpha that zeroes a small primary alpha (the skip tests the untinted colour).
    #[test]
    fn saved_tint_equals_global_tint() {
        let mut anm = AnmManager::default();
        let chunk = th08_formats::anm::Chunk {
            name: "t".into(),
            width: 256,
            height: 256,
            sprites: vec![th08_formats::anm::Sprite {
                id: 0,
                x: 0.0,
                y: 0.0,
                w: 32.0,
                h: 16.0,
            }],
            scripts: Vec::new(),
            has_texture: false,
        };
        anm.insert(0, "t", vec![chunk], |_| crate::drawlist::TexId(0));
        let mut vm = AnmVm::default();
        vm.init();
        vm.set_sprite(&anm, 0);
        vm.pos = [100.0, 100.0, 0.0];
        for (tint, color, color2) in [
            (0xFFAB_ABAB, [255, 255, 255, 255], None),
            (0x20F0_1010, [10, 200, 30, 3], None),
            (0x8060_6060, [1, 2, 3, 4], Some([200, 100, 50, 255])),
        ] {
            vm.color = color;
            vm.flags &= !anmvm::F_COLOR2;
            if let Some(c) = color2 {
                (vm.color2, vm.flags) = (c, vm.flags | anmvm::F_COLOR2);
            }
            let mut global = Recorder::new();
            (anm.tint, anm.tint_on) = (tint, true);
            emit_mode_z(&anm, &mut global, &vm, 0, 0.0);
            anm.tint_off();
            let mut saved = Recorder::new();
            emit_tinted(&anm, &mut saved, &vm, 0, 0.0, Some(Tint(tint)));
            assert_eq!(global.draws.len(), 1);
            assert_eq!(format!("{:?}", global.draws), format!("{:?}", saved.draws));
        }
    }

    // 13 §2.6c: half size ((len·0.5)·sprite_w)·scale.x around the projected point, unrotated.
    #[test]
    fn billboard_half_size() {
        let mut vm = AnmVm::default();
        (vm.sprite_w, vm.sprite_h, vm.scale) = (32.0, 16.0, [2.0, 0.5]);
        let c = billboard_corners(&vm, [100.0, 50.0, 0.5], 1.5);
        assert_eq!(c[0], [100.0 - 48.0, 50.0 - 6.0]);
        assert_eq!(c[3], [100.0 + 48.0, 50.0 + 6.0]);
        assert_eq!(length3([3.0, 0.0, 4.0]), 5.0);
    }
}
