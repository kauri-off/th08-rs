use crate::anmmgr::{AnmManager, Fog};
use crate::anmvm::{self, AnmVm, Timer};
use crate::game::Ctx;
use crate::gfx::{Camera, Gfx, Mat4, PLAYFIELD, Rect, Vertex};
use crate::player::PlayerRef;
use crate::stdfile::{self, QuadKind, Record, Std, Vec3};
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;
use std::cell::RefCell;
use std::f32::consts::PI;
use std::rc::Rc;

pub const ANM_SLOT: usize = 4;
pub const CALC_PRIO: u16 = 8;
pub const DRAW_LOW_PRIO: u16 = 6;
pub const DRAW_HIGH_PRIO: u16 = 7;
const NEAR: f32 = 30.0;
const OVERLAY_SLOT: usize = 0;
const OVERLAY_SCRIPT: usize = 33;
const FAR: f32 = 1800.0;

const STD_NAMES: [&str; 9] = ["stage1", "stage2", "stage3", "stage4a", "stage4b", "stage5", "stage6", "stage7", "stage8"];
const BG_NAMES: [&str; 9] = ["stg1bg", "stg2bg", "stg3bg", "stg4abg", "stg4abg", "stg5bg", "stg6bg", "stg7bg", "stg8bg"];

// Stage index 0..17 (9..17 = `_s` variants) → (.std, bg .anm).
pub fn files(index: usize) -> (String, String) {
    let k = index % 9;
    let s = if index >= 9 { "_s" } else { "" };
    (format!("{}{s}.std", STD_NAMES[k]), format!("{}.anm", BG_NAMES[k]))
}

// Accepts 1, 2, 3, 4a, 4b, 5, 6a, 6b, ex (also 6/7/8 = file digits), stageN[_s][.std], or `idx:<n>`.
pub fn parse_stage_arg(a: &str) -> Option<usize> {
    let a = a.trim().to_ascii_lowercase();
    if let Some(n) = a.strip_prefix("idx:") {
        return n.parse().ok().filter(|&n| n < 18);
    }
    let a = a.trim_end_matches(".std");
    let a = a.strip_prefix("stage").unwrap_or(a);
    let (a, alt) = match a.strip_suffix("_s") {
        Some(b) => (b, 9),
        None => (a, 0),
    };
    let k = match a {
        "1" => 0,
        "2" => 1,
        "3" => 2,
        "4a" => 3,
        "4b" => 4,
        "5" => 5,
        "6" | "6a" => 6,
        "7" | "6b" => 7,
        "8" | "ex" | "extra" => 8,
        _ => return None,
    };
    Some(k + alt)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FogState {
    pub near: f32,
    pub far: f32,
    pub color: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct CamBlock {
    v: [Vec3; 3],
    fov: f32,
}

#[derive(Clone, Debug)]
pub struct StageCamera {
    target: CamBlock,
    start: CamBlock,
    tan0: [Vec3; 3],
    tan1: [Vec3; 3],
    pub v: [Vec3; 3],
    pub fov: f32,
    pub facing_norm: Vec3,
    pub eye_offset: Vec3,
    dur: [i32; 4],
    elapsed: [Timer; 4],
    mode: [i32; 4],
}

const POS: usize = 0;
const FACING: usize = 1;
const UP: usize = 2;
const FOV: usize = 3;

impl Default for StageCamera {
    fn default() -> Self {
        let v = [[0.0, 0.0, 1000.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let fov = std::f32::consts::FRAC_PI_6;
        let block = CamBlock { v, fov };
        Self {
            target: block,
            start: block,
            tan0: [[0.0; 3]; 3],
            tan1: [[0.0; 3]; 3],
            v,
            fov,
            facing_norm: [0.0; 3],
            eye_offset: [0.0; 3],
            dur: [0; 4],
            elapsed: [Timer::default(); 4],
            mode: [0; 4],
        }
    }
}

// Stage camera easing: 1–3 ease-out, 4–6 ease-in (numbering differs from the ANM VM).
fn cam_ease(mode: i32, t: f32) -> f32 {
    match mode {
        1 => 1.0 - (1.0 - t) * (1.0 - t),
        2 => 1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t),
        3 => 1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t) * (1.0 - t),
        4 => t * t,
        5 => t * t * t,
        6 => t * t * t * t,
        _ => t,
    }
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: Vec3, s: f32) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalize(a: Vec3) -> Vec3 {
    let l = dot(a, a).sqrt();
    if l == 0.0 { a } else { scale(a, 1.0 / l) }
}

impl StageCamera {
    // Returns the interpolation factor for slot s (None if inactive), advancing its timer.
    fn step(&mut self, s: usize, speed: f32) -> Option<f32> {
        let d = self.dur[s];
        if d == 0 {
            return None;
        }
        let t = if self.elapsed[s].cur >= d {
            self.elapsed[s].set(d);
            self.dur[s] = 0;
            1.0
        } else {
            self.elapsed[s].tick(speed);
            self.elapsed[s].getf() / d as f32
        };
        Some(t)
    }

    fn update(&mut self, speed: f32) {
        for s in [POS, FACING, UP] {
            let Some(t) = self.step(s, speed) else { continue };
            let (a, b) = (self.start.v[s], self.target.v[s]);
            self.v[s] = if self.mode[s] == 7 {
                let (t0, t1) = (self.tan0[s], self.tan1[s]);
                std::array::from_fn(|k| {
                    (2.0 * t + 1.0) * (t - 1.0) * (t - 1.0) * a[k]
                        + (3.0 - 2.0 * t) * t * t * b[k]
                        + t * (1.0 - t) * (1.0 - t) * t0[k]
                        + (t - 1.0) * t * t * t1[k]
                })
            } else {
                let t = cam_ease(self.mode[s], t);
                std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t)
            };
        }
        if let Some(t) = self.step(FOV, speed) {
            let t = cam_ease(self.mode[FOV], t);
            self.fov = self.start.fov + (self.target.fov - self.start.fov) * t;
        }
        self.facing_norm = normalize(self.v[FACING]);
    }

    pub fn eye(&self) -> Vec3 {
        add(self.v[POS], self.eye_offset)
    }

    pub fn camera(&self, vp: Rect) -> Camera {
        Camera {
            view: Mat4::look_at_lh(self.eye(), add(self.v[POS], self.v[FACING]), self.v[UP]),
            proj: Mat4::perspective_fov_lh(self.fov, vp.w / vp.h, NEAR, FAR),
        }
    }
}

pub struct Stage {
    pub index: usize,
    pub std: Std,
    obj_flags: Vec<u8>,
    vms: Vec<AnmVm>,
    vm_a: AnmVm,
    vm_b: AnmVm,
    vm_c: AnmVm,
    clock: Timer,
    ip: usize,
    pub frame: u32,
    origin: Vec3,
    pub clear_color: u32,
    pub fog: FogState,
    fog_start: FogState,
    fog_target: FogState,
    fog_dur: i32,
    fog_elapsed: Timer,
    pub section_request: i32,
    pub cam: StageCamera,
    jumped: bool,
    sway_mode: u8,
    sway_clock: Timer,
    cull_dist2: f32,
    pub fast_forward: u32,
    overlay: AnmVm,
    overlay_mode: u8,
    overlay_timer: Timer,
    pub player: Option<PlayerRef>,
}

impl Stage {
    pub fn new(index: usize, std: Std) -> Self {
        let cull_dist2 = match index {
            5 => 1_822_500.0,
            6 | 7 => 3_240_000.0,
            _ => 1_322_500.0,
        };
        let fog = FogState { near: 200.0, far: 500.0, color: 0xFF00_0000 };
        Self {
            index,
            obj_flags: vec![1; std.objects.len()],
            vms: vec![AnmVm::default(); std.quad_count],
            std,
            vm_a: AnmVm::default(),
            vm_b: AnmVm::default(),
            vm_c: AnmVm::default(),
            clock: Timer::default(),
            ip: 0,
            frame: 0,
            origin: [0.0; 3],
            clear_color: 0,
            fog,
            fog_start: fog,
            fog_target: fog,
            fog_dur: 0,
            fog_elapsed: Timer::default(),
            section_request: 0,
            cam: StageCamera::default(),
            jumped: false,
            sway_mode: 0,
            sway_clock: Timer::default(),
            cull_dist2,
            fast_forward: 0,
            overlay: AnmVm::default(),
            overlay_mode: 0,
            overlay_timer: Timer::default(),
            player: None,
        }
    }

    // Stage_Load steps 1, 3 and 4 (files already loaded): every quad's script, then the overlay VM (text.anm 33).
    pub fn start(&mut self, anm: &mut AnmManager) {
        anm.set_script(&mut self.overlay, OVERLAY_SLOT, OVERLAY_SCRIPT);
        self.overlay.pending_interrupt = 2;
        self.overlay_mode = 0;
        self.clock.set(0);
        self.ip = 0;
        for q in self.std.objects.iter().flat_map(|o| &o.quads) {
            if let Some(vm) = self.vms.get_mut(q.vm) {
                anm.set_script(vm, ANM_SLOT, q.script.max(0) as usize);
            }
        }
    }

    pub fn register(ctx: &mut Ctx, index: usize, player: Option<PlayerRef>) -> Result<Rc<RefCell<Stage>>> {
        let (std_name, bg) = files(index);
        let std = stdfile::parse(&ctx.archive.read(&std_name)?)?;
        ctx.anm.load(&mut ctx.gfx, &ctx.archive, ANM_SLOT, &bg)?;
        let mut st = Stage::new(index, std);
        st.player = player;
        st.fast_forward = crate::env::var("TH08_STAGE_FRAME").and_then(|v| v.parse().ok()).unwrap_or(0);
        st.start(&mut ctx.anm);
        let st = Rc::new(RefCell::new(st));
        let t: task::TaskRef<Ctx> = st.clone();
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_LOW_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_HIGH_PRIO, 1, t)?;
        Ok(st)
    }

    fn bg_vm(&mut self, anm: &mut AnmManager, which: u8, script: i32) {
        let vm = match which {
            0 => &mut self.vm_a,
            1 => &mut self.vm_b,
            _ => &mut self.vm_c,
        };
        if script >= 0 {
            anm.set_script(vm, ANM_SLOT, script as usize);
        } else if which == 2 {
            self.vm_c.sprite_index = -1;
        } else {
            // Quirk: op 30 with a negative arg disables VM A, not VM B.
            self.vm_a.sprite_index = -1;
        }
    }

    fn fog_from(c: u32, near: f32, far: f32) -> FogState {
        FogState { near, far, color: c }
    }

    // Executes one record; returns false when processing must stop for this frame (op 3).
    fn exec(&mut self, anm: &mut AnmManager, r: Record) -> bool {
        let cam = &mut self.cam;
        match r.op {
            0 => self.origin = r.v(),
            1 => {
                self.fog = Self::fog_from(r.args[0], r.f(1), r.f(2));
                self.fog_target = self.fog;
            }
            2 => {
                self.fog_start = self.fog;
                self.fog_dur = r.i(0);
                self.fog_elapsed.set(0);
            }
            3 => {
                if self.section_request == 0 {
                    return false;
                }
                self.section_request = 0;
            }
            4 => {
                self.ip = r.i(0).max(0) as usize;
                self.clock.set(r.i(1));
                cam.dur[POS] = 0;
                self.jumped = true;
                return true;
            }
            5 | 7 | 9 => {
                let s = ((r.op - 5) / 2) as usize;
                if s == POS && self.jumped {
                    // SPEC-GAP: shifting kind-51 effects by the target delta waits for the effect system.
                    self.jumped = false;
                }
                cam.start.v[s] = cam.target.v[s];
                cam.target.v[s] = r.v();
                if cam.dur[s] == 0 {
                    cam.v[s] = r.v();
                }
            }
            6 | 8 | 10 | 12 => {
                let s = ((r.op - 6) / 2) as usize;
                cam.dur[s] = r.i(0);
                cam.elapsed[s].set(0);
                cam.mode[s] = r.i(1);
            }
            11 => {
                cam.start.fov = cam.target.fov;
                cam.target.fov = r.f(0);
                if cam.dur[FOV] == 0 {
                    cam.fov = r.f(0);
                }
            }
            13 => self.clear_color = r.args[0],
            14..=28 => {
                let s = ((r.op - 14) / 5) as usize;
                match (r.op - 14) % 5 {
                    0 => cam.start.v[s] = r.v(),
                    1 => cam.target.v[s] = r.v(),
                    2 => cam.tan0[s] = r.v(),
                    3 => cam.tan1[s] = r.v(),
                    _ => {
                        cam.dur[s] = r.i(0);
                        cam.elapsed[s].set(0);
                        cam.mode[s] = 7;
                    }
                }
            }
            29 => self.bg_vm(anm, 0, r.i(0)),
            30 => self.bg_vm(anm, 1, r.i(0)),
            34 => self.bg_vm(anm, 2, r.i(0)),
            32 => cam.eye_offset = r.v(),
            33 => {
                self.sway_mode = r.i(0) as u8;
                self.sway_clock.set(0);
            }
            _ => {}
        }
        self.ip += 1;
        true
    }

    fn handle_section_request(&mut self) {
        if self.section_request == 0 {
            return;
        }
        if let Some(k) = self.std.script.iter().position(|r| r.time != -1 && r.op == 31 && r.i(0) == self.section_request) {
            self.ip = k + 1;
            self.clock.set(self.std.script[k].time);
            self.section_request = 0;
        }
    }

    fn sway(&mut self, speed: f32) {
        let c = self.sway_clock.getf();
        let (p, wrap) = match self.sway_mode {
            1 | 2 => (480.0, 480),
            3 => (4800.0, 4800),
            _ => return,
        };
        let phi = (c * PI + c * PI) / p - PI;
        let s = phi.sin();
        match self.sway_mode {
            1 => self.cam.eye_offset[0] = s * 40.0,
            2 => {
                self.cam.eye_offset[0] = s * 70.0;
                self.cam.v[UP][0] = -s * 0.1;
            }
            _ => {
                self.cam.v[UP][0] = s;
                self.cam.v[UP][2] = phi.cos();
            }
        }
        self.sway_clock.tick(speed);
        if self.sway_clock.cur >= wrap {
            self.sway_clock.set(0);
        }
    }

    fn update_fog(&mut self, speed: f32) {
        if self.fog_dur == 0 {
            return;
        }
        self.fog_elapsed.tick(speed);
        let d = self.fog_dur as f32;
        let t = (self.fog_elapsed.getf() / d).min(1.0);
        let (a, b) = (self.fog_start.color.to_le_bytes(), self.fog_target.color.to_le_bytes());
        let c: [u8; 4] = std::array::from_fn(|k| ((b[k] as f32 - a[k] as f32) * t + a[k] as f32) as u8);
        self.fog.color = u32::from_le_bytes(c);
        self.fog.near = (self.fog_target.near - self.fog_start.near) * t + self.fog_start.near;
        self.fog.far = (self.fog_target.far - self.fog_start.far) * t + self.fog_start.far;
        if self.fog_elapsed.getf() >= d {
            self.fog_dur = 0;
        }
    }

    // Stage_UpdateObjects (06 §9): the overlay VM follows the player mode byte (human blue, youkai white).
    fn update_objects(&mut self, anm: &mut AnmManager) {
        let mode = self.player.as_ref().map_or(0, |p| p.borrow().mode);
        if (mode != 0) != (self.overlay_mode != 0) {
            self.overlay_mode = (mode != 0) as u8;
            self.overlay.pending_interrupt = if mode != 0 { 1 } else { 2 };
            self.overlay_timer = Timer::default();
        }
        if self.overlay.script.is_some() {
            self.overlay.run(anm);
        }
        self.overlay_timer.tick(anm.speed);
        let overlay = if self.overlay.has_sprite { self.overlay.color } else { [255u8; 4] };
        for (o, obj) in self.std.objects.iter().enumerate() {
            if self.obj_flags[o] & 1 == 0 {
                continue;
            }
            let mut running = 0;
            for q in &obj.quads {
                if !self.vms[q.vm].run(anm) {
                    running += 1;
                }
            }
            if let Some(last) = obj.quads.last() {
                let vm = &mut self.vms[last.vm];
                if vm.tag == 1 {
                    vm.flags |= anmvm::F_COLOR2;
                    vm.color2 = std::array::from_fn(|k| ((vm.color[k] as u32 * overlay[k] as u32) / 256) as u8);
                }
            }
            if running == 0 {
                self.obj_flags[o] &= !1;
            }
        }
    }

    pub fn step(&mut self, anm: &mut AnmManager) {
        let speed = anm.speed;
        self.handle_section_request();
        while let Some(&r) = self.std.script.get(self.ip) {
            if r.time == -1 || self.clock.cur < r.time || !self.exec(anm, r) {
                break;
            }
        }
        self.cam.update(speed);
        self.sway(speed);
        self.update_fog(speed);
        if self.std.script.get(self.ip).is_none_or(|r| r.op != 3) {
            self.clock.tick(speed);
        }
        self.update_objects(anm);
        // SPEC-GAP: spell-background VMs (stage state +0xB24) and sparkle effects wait for later phases.
        if self.vm_a.sprite_index > 0 {
            self.vm_a.run(anm);
        }
        if self.vm_b.sprite_index > 0 {
            self.vm_b.run(anm);
        }
        if self.vm_c.sprite_index > 0 {
            self.vm_c.run(anm);
            self.clear_color = u32::from_le_bytes(self.vm_c.color);
        }
        self.frame += 1;
    }

    fn fog_params(&self, anm: &AnmManager) -> Fog {
        let c = self.fog.color.to_le_bytes();
        let [r, g, b, _] = anm.tinted([c[2], c[1], c[0], c[3]]);
        Fog { near: self.fog.near, far: self.fog.far, color: [r, g, b] }
    }

    fn draw_layer(&mut self, anm: &mut AnmManager, gfx: &mut Gfx, layer: u8) {
        let vp = gfx.viewport();
        let cam = self.cam.camera(vp);
        anm.stage_camera = Some(cam);
        let m = cam.view.0;
        let right = normalize([m[0][0], m[0][1], m[0][2]]);
        let view = View { cam, vp, right, eye: self.cam.eye(), fog: self.fog };
        let fog = self.fog_params(anm);
        for inst in &self.std.instances {
            let obj = &self.std.objects[inst.object];
            if obj.layer != layer {
                continue;
            }
            let centre = add(sub(add(obj.pos, inst.pos), self.origin), scale(obj.size, 0.5));
            let rel = sub(centre, view.eye);
            if dot(rel, rel) > self.cull_dist2 {
                continue;
            }
            let d = dot(rel, self.cam.facing_norm);
            if !(80.0..=dot(obj.size, obj.size).sqrt() / 2.0 + 960.0).contains(&d) {
                continue;
            }
            self.obj_flags[inst.object] |= 2;
            for q in &obj.quads {
                let vm = &mut self.vms[q.vm];
                match q.kind {
                    QuadKind::Sprite { pos, w, h } => {
                        vm.pos = sub(add(add(vm.pos_offset, pos), inst.pos), self.origin);
                        if w != 0.0 && vm.sprite_w != 0.0 {
                            vm.scale[0] = w / vm.sprite_w;
                        }
                        if h != 0.0 && vm.sprite_h != 0.0 {
                            vm.scale[1] = h / vm.sprite_h;
                        }
                        if vm.tag & 0xF == 2 {
                            view.billboard(anm, gfx, vm, w);
                        } else {
                            anm.draw_3d_world(gfx, vm, &cam, Some(fog));
                        }
                    }
                    QuadKind::Ribbon { a, b, w } => {
                        let pa = sub(add(a, inst.pos), self.origin);
                        let pb = sub(add(b, inst.pos), self.origin);
                        view.ribbon(anm, gfx, vm, pa, pb, w);
                    }
                }
            }
        }
    }
}

struct View {
    cam: Camera,
    vp: Rect,
    right: Vec3,
    eye: Vec3,
    fog: FogState,
}

impl View {
    // Software fog (D3DCOLOR byte order); None when fully fogged (f ≥ 1).
    fn soft_fog(&self, color: [u8; 4], dist: f32) -> Option<[u8; 4]> {
        let (near, far) = (self.fog.near, self.fog.far);
        if dist <= near {
            return Some(color);
        }
        let f = (near - dist) / (near - far);
        if f >= 1.0 {
            return None;
        }
        let fc = self.fog.color.to_le_bytes();
        let mut c = color;
        for k in 0..3 {
            c[k] = (c[k] as i32 - ((c[k] as i32 - fc[k] as i32) as f32 * f) as i32) as u8;
        }
        c[3] = (c[3] as f32 * (1.0 - f)) as u8;
        Some(c)
    }

    fn dist(&self, p: Vec3) -> f32 {
        let r = sub(p, self.eye);
        dot(r, r).sqrt()
    }

    fn billboard(&self, anm: &mut AnmManager, gfx: &mut Gfx, vm: &mut AnmVm, w: f32) {
        let s = self.cam.project(vm.pos, self.vp);
        let ww = if w != 0.0 { w } else { vm.sprite_w };
        let s2 = self.cam.project(add(vm.pos, scale(self.right, ww * vm.scale[0])), self.vp);
        let d = sub(s2, s);
        let sc = dot(d, d).sqrt() / ww;
        vm.scale = [sc, if ww < 0.0 { -sc } else { sc }];
        let dist = self.dist(vm.pos);
        let saved = vm.color;
        let Some(c) = self.soft_fog(vm.color, dist) else { return };
        vm.color = c;
        vm.pos = s;
        if (0.0..=1.0).contains(&s[2]) {
            anm.draw_mode_z(gfx, vm, 1, s[2]);
        }
        // SPEC-GAP: sparkle points (tag & 0xF0 == 0x10) wait for the effect system.
        vm.color = saved;
    }

    // DrawCustomQuad skip rules: enabled, visible, primary alpha ≠ 0 (06 §13).
    fn ribbon(&self, anm: &mut AnmManager, gfx: &mut Gfx, vm: &mut AnmVm, a: Vec3, b: Vec3, w: f32) {
        use anmvm::*;
        if vm.flags & (F_VISIBLE | F_ENABLED) != (F_VISIBLE | F_ENABLED) || vm.color[A] == 0 || !vm.has_sprite {
            return;
        }
        let Some(&spr) = anm.sprite(vm.anm_slot as usize, vm.sprite_index as usize) else { return };
        let ww = if w != 0.0 { w } else { vm.sprite_w };
        let base = if vm.flags & F_COLOR2 != 0 { vm.color2 } else { vm.color };
        let point = |p: Vec3| {
            let sp = self.cam.project(p, self.vp);
            let e = sub(self.cam.project(add(p, scale(self.right, ww)), self.vp), sp);
            let mut c = self.soft_fog(base, self.dist(p)).unwrap_or([base[0], base[1], base[2], 0]);
            c = anm.tinted([c[R], c[G], c[B], c[A]]);
            (sp, dot(e, e).sqrt() / 2.0, c)
        };
        let (sa, wa, ca) = point(a);
        let (sb, wb, cb) = point(b);
        let (dx, dy) = (sb[0] - sa[0], sb[1] - sa[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-5 || !(0.0..=1.0).contains(&sa[2]) || !(0.0..=1.0).contains(&sb[2]) {
            return;
        }
        let (dx, dy) = (dx / len, dy / len);
        let (u0, v0) = (spr.uv[0] + vm.uv_off[0], spr.uv[1] + vm.uv_off[1]);
        let (u1, v1) = (spr.uv[2] + vm.uv_off[0], spr.uv[3] + vm.uv_off[1]);
        let o = anm.screen_offset;
        let vx = |x: f32, y: f32, z: f32, uv: [f32; 2], c: [u8; 4]| Vertex::screen(x + o[0] + 0.5, y + o[1] + 0.5, z, uv, c);
        let v = [
            vx(sa[0] + dy * wa, sa[1] - dx * wa, sa[2], [u0, v0], ca),
            vx(sa[0] - dy * wa, sa[1] + dx * wa, sa[2], [u1, v0], ca),
            vx(sb[0] + dy * wb, sb[1] - dx * wb, sb[2], [u0, v1], cb),
            vx(sb[0] - dy * wb, sb[1] + dx * wb, sb[2], [u1, v1], cb),
        ];
        let blend = anm.blend_for(vm);
        anm.bind(spr.tex);
        gfx.strip(spr.tex, blend, vm.flags & F_ZWRITE_OFF == 0, v);
    }
}

impl Task<Ctx> for Stage {
    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc {
            ctx.anm.unload(&mut ctx.gfx, ANM_SLOT);
        }
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        for _ in 0..std::mem::take(&mut self.fast_forward) {
            self.step(&mut ctx.anm);
        }
        self.step(&mut ctx.anm);
        Ret::Next
    }

    fn draw(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        let (anm, gfx) = (&mut ctx.anm, &mut ctx.gfx);
        if tag == 0 {
            gfx.set_viewport(PLAYFIELD);
            anm.reset_render_state();
            for vm in [&mut self.vm_a, &mut self.vm_b] {
                if vm.sprite_index > 0 {
                    anm.draw_mode(gfx, vm, 2);
                }
            }
            let c = self.clear_color.to_le_bytes();
            let rgba = [c[2], c[1], c[0], c[3]];
            if c[3] == 0xFF {
                gfx.clear_depth(Some(rgba));
            } else {
                if self.clear_color != 0 {
                    gfx.clear_viewport(rgba);
                }
                gfx.clear_depth(None);
            }
            gfx.set_depth_test(true);
            self.draw_layer(anm, gfx, 0);
            self.draw_layer(anm, gfx, 1);
        } else {
            self.draw_layer(anm, gfx, 2);
            self.draw_layer(anm, gfx, 3);
            gfx.set_depth_test(false);
            anm.stage_camera = None;
            gfx.set_viewport(PLAYFIELD);
        }
        Ret::Next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(time: i32, op: i16, a: [u32; 3]) -> Record {
        Record { time, op, args: a }
    }

    fn fl(v: [f32; 3]) -> [u32; 3] {
        v.map(f32::to_bits)
    }

    fn synthetic(script: Vec<Record>) -> Stage {
        let std = Std { quad_count: 0, bgm: Default::default(), objects: Vec::new(), instances: Vec::new(), script };
        Stage::new(0, std)
    }

    #[test]
    fn camera_interpolation_and_jump() {
        let mut st = synthetic(vec![
            rec(0, 5, fl([0.0, 0.0, 0.0])),
            rec(0, 6, [10, 0, 0]),
            rec(0, 5, fl([0.0, 100.0, 0.0])),
            rec(20, 4, [3, 0, 0]),
            rec(-1, 0, [0; 3]),
        ]);
        let mut anm = AnmManager::default();
        st.step(&mut anm);
        assert_eq!(st.cam.v[POS], [0.0, 10.0, 0.0]);
        for _ in 0..9 {
            st.step(&mut anm);
        }
        assert_eq!(st.cam.v[POS], [0.0, 100.0, 0.0]);
        for _ in 0..30 {
            st.step(&mut anm);
        }
        assert_eq!(st.clock.cur, 20);
        assert_eq!(st.ip, 3);
    }

    #[test]
    fn halt_and_section_request() {
        let mut st = synthetic(vec![
            rec(0, 3, [0; 3]),
            rec(0, 13, [1, 0, 0]),
            rec(50, 31, [2, 0, 0]),
            rec(50, 13, [2, 0, 0]),
            rec(-1, 0, [0; 3]),
        ]);
        let mut anm = AnmManager::default();
        for _ in 0..5 {
            st.step(&mut anm);
        }
        assert_eq!((st.ip, st.clock.cur, st.clear_color), (0, 0, 0));
        st.section_request = 2;
        st.step(&mut anm);
        assert_eq!((st.clear_color, st.section_request, st.clock.cur), (2, 0, 51));
    }

    #[test]
    fn fog_interpolates() {
        let mut st = synthetic(vec![rec(0, 2, [4, 0, 0]), rec(0, 1, [0xFF40_8000, 100f32.to_bits(), 300f32.to_bits()]), rec(-1, 0, [0; 3])]);
        let mut anm = AnmManager::default();
        st.step(&mut anm);
        assert_eq!(st.fog.near, 175.0);
        assert_eq!(st.fog.color, 0xFF10_2000);
        for _ in 0..3 {
            st.step(&mut anm);
        }
        assert_eq!(st.fog, FogState { near: 100.0, far: 300.0, color: 0xFF40_8000 });
        assert_eq!(st.fog_dur, 0);
    }

    #[test]
    fn stage_args() {
        assert_eq!(parse_stage_arg("4b"), Some(4));
        assert_eq!(parse_stage_arg("6B"), Some(7));
        assert_eq!(parse_stage_arg("stage7_s.std"), Some(16));
        assert_eq!(parse_stage_arg("ex"), Some(8));
        assert_eq!(files(4), ("stage4b.std".into(), "stg4abg.anm".into()));
    }

    #[test]
    fn all_std_scripts_run() {
        let Some(dir) = std::env::var("TH08_DIR").ok() else { return };
        let ar = crate::archive::Archive::from_bytes(std::fs::read(format!("{dir}/th08.dat")).unwrap()).unwrap();
        for index in 0..18 {
            let (std_name, bg) = files(index);
            let std = stdfile::parse(&ar.read(&std_name).unwrap()).unwrap();
            let mut anm = AnmManager::default();
            anm.insert(ANM_SLOT, &bg, crate::anm::parse(&ar.read(&bg).unwrap()).unwrap(), |_| 0);
            let mut st = Stage::new(index, std);
            st.start(&mut anm);
            for f in 0..6000 {
                if f == 3000 {
                    st.section_request = 1;
                }
                st.step(&mut anm);
                let cam = st.cam.camera(PLAYFIELD);
                assert!(cam.view.0.iter().flatten().all(|v| v.is_finite()), "{std_name} frame {f}: camera {:?}", st.cam.v);
            }
            assert!(st.ip > 0, "{std_name}: script never advanced");
        }
    }
}
