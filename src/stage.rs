//! Stage background g_Stage 0x4E4030: .std script VM, 3D camera, fog, layers, spell background (06, 13 §5).

use crate::anmmgr::{AnmManager, Fog};
use crate::anmvm::{self, AnmVm, Timer};
use crate::camera::{Camera, Mat4};
use crate::drawlist::Recorder;
use crate::drawlist::{PLAYFIELD, Rect, Vertex};
use crate::game::{Ctx, Sys};
use crate::stdfile::{self, QuadKind, Record, Std, Vec3};
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;
use std::f32::consts::PI;
use std::sync::Arc;

// stgNbg.anm slot (06 §2 step 2).
pub const ANM_SLOT: usize = 4;
// Stage_Calc 0x407400 / Stage_DrawLow 0x409200 / Stage_DrawHigh 0x409640 (06).
pub const CALC_PRIO: u16 = 8;
pub const DRAW_LOW_PRIO: u16 = 6;
pub const DRAW_HIGH_PRIO: u16 = 7;
// PerspectiveFovLH near/far planes (06 §10.1).
const NEAR: f32 = 30.0;
// Overlay/tint VM +0x844: text.anm (slot 0) script 33 (06 §2 step 4, §13).
const OVERLAY_SLOT: usize = 0;
const OVERLAY_SCRIPT: usize = 33;
const FAR: f32 = 1800.0;

const STD_NAMES: [&str; 9] = [
    "stage1", "stage2", "stage3", "stage4a", "stage4b", "stage5", "stage6", "stage7", "stage8",
];
const BG_NAMES: [&str; 9] = [
    "stg1bg", "stg2bg", "stg3bg", "stg4abg", "stg4abg", "stg5bg", "stg6bg", "stg7bg", "stg8bg",
];

// Stage index 0..8 → (.std, bg .anm); spell practice (GameFlags bit 14) takes `stageN_s.std` (07 §1, 06 §2, 04).
pub fn files(index: usize, spell_practice: bool) -> (String, String) {
    let s = if spell_practice { "_s" } else { "" };
    (
        format!("{}{s}.std", STD_NAMES[index]),
        format!("{}.anm", BG_NAMES[index]),
    )
}

// Accepts 1, 2, 3, 4a, 4b, 5, 6a, 6b, ex (also 6/7/8 = file digits), stageN[.std], or `idx:<n>`.
pub fn parse_stage_arg(a: &str) -> Option<usize> {
    let a = a.trim().to_ascii_lowercase();
    if let Some(n) = a.strip_prefix("idx:") {
        return n.parse().ok().filter(|&n| n < 9);
    }
    let a = a.trim_end_matches(".std");
    let a = a.strip_prefix("stage").unwrap_or(a);
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
    Some(k)
}

// Fog near/far (eye-space z) and colour (D3DCOLOR); current at +0xAEC (06 §3, §8).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FogState {
    pub near: f32,
    pub far: f32,
    pub color: u32,
}

// Camera value block: pos, facing, up (indexed POS/FACING/UP) and fov (06 §3).
#[derive(Clone, Copy, Debug, Default)]
struct CamBlock {
    v: [Vec3; 3],
    fov: f32,
}

// Stage camera fields of g_Stage +0x6264..+0x6474 (06 §3).
#[derive(Clone, Debug)]
pub struct StageCamera {
    // +0x6264 target values.
    target: CamBlock,
    // +0x62B0 start values.
    start: CamBlock,
    // +0x6348 / +0x62FC Hermite start / end tangents.
    tan0: [Vec3; 3],
    tan1: [Vec3; 3],
    // +0x6394 position, +0x63A0 facing (look-at = pos + facing), +0x63AC up.
    pub v: [Vec3; 3],
    // +0x63DC vertical fov, radians.
    pub fov: f32,
    // +0x63B8, recomputed every frame.
    pub facing_norm: Vec3,
    // +0x63D0: eye = pos + eye offset (op 32, sway).
    pub eye_offset: Vec3,
    // +0x63E0 / +0x63F4 / +0x6430 per slot POS/FACING/UP/FOV: duration (0 = inactive), elapsed, mode.
    dur: [i32; 4],
    elapsed: [Timer; 4],
    mode: [i32; 4],
}

const POS: usize = 0;
const FACING: usize = 1;
const UP: usize = 2;
const FOV: usize = 3;

// Stage_Load defaults (06 §2 step 5): pos (0, 0, 1000), up (0, 1, 0), fov π/6; start/target copied.
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

// Stage camera easing (06 §6): 1–3 ease-out, 4–6 ease-in (numbering differs from the ANM VM).
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
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalize(a: Vec3) -> Vec3 {
    let l = dot(a, a).sqrt();
    if l == 0.0 { a } else { scale(a, 1.0 / l) }
}

// Sparkle-point test `v < fog_near` (06 §11.1). Quirk kept: past fog_near the original has overwritten v with
// the fog factor f, so any drawn quad passes there whenever fog_near > 1.
fn sparkle_point_ok(dist: f32, fog: FogState) -> bool {
    let v = if dist <= fog.near {
        dist
    } else {
        (fog.near - dist) / (fog.near - fog.far)
    };
    v < fog.near
}

impl StageCamera {
    // 0x408D60 timer part (06 §6): factor for slot s (None if inactive); finishing sets t = 1 and duration 0.
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

    // 06 §5: slots pos, facing, up (mode 7 = Hermite), then fov (modes 0–6), then facing_norm.
    fn update(&mut self, speed: f32) {
        for s in [POS, FACING, UP] {
            let Some(t) = self.step(s, speed) else {
                continue;
            };
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
        // +0x63B8 (06 §3): D3DX SSE normalise, not facing / |facing| (05 §10).
        self.facing_norm = crate::d3dx::vec3_normalize(self.v[FACING]);
    }

    pub fn eye(&self) -> Vec3 {
        add(self.v[POS], self.eye_offset)
    }

    // Stage_SetCamera's +0x63C4 (06 §10.1, 13 §2.6c): normalise(cross(raw facing +0x63A0, up +0x63AC)), not view row 0.
    pub fn right(&self) -> Vec3 {
        normalize(cross(self.v[FACING], self.v[UP]))
    }

    // Stage_SetCamera 0x40B6D0 (06 §10.1): LookAtLH(eye, pos + facing, up), PerspectiveFovLH(fov, w/h, 30, 1800).
    pub fn camera(&self, vp: Rect) -> Camera {
        Camera {
            view: Mat4::look_at_lh(self.eye(), add(self.v[POS], self.v[FACING]), self.v[UP]),
            proj: Mat4::perspective_fov_lh(self.fov, vp.w / vp.h, NEAR, FAR),
        }
    }
}

// g_Stage 0x4E4030 (06 §3); `Ctx.stage`, its chain nodes are `StageTask`.
#[derive(Clone)]
pub struct Stage {
    // Stage index 0..8 (g_StageIdx; also 0..8 in spell practice, 07 §1).
    pub index: usize,
    // The parsed .std file, read-only after load (shared between World clones).
    pub std: Arc<Std>,
    // Object +3 runtime flags (06 §1.1): bit 0 script running, bit 1 drawn.
    obj_flags: Vec<u8>,
    // +0x000 one VM per quad.
    vms: Vec<AnmVm>,
    // +0x004 / +0x2A8 / +0x54C background VMs A / B / C (ops 29 / 30 / 34).
    vm_a: AnmVm,
    vm_b: AnmVm,
    vm_c: AnmVm,
    // +0x80C script clock, +0x818 instruction index.
    clock: Timer,
    ip: usize,
    // +0x81C frames since stage start.
    pub frame: u32,
    // +0x824 op-0 origin, subtracted from every world position.
    origin: Vec3,
    // +0x830 D3DCOLOR (op 13; VM C's colour while VM C runs).
    pub clear_color: u32,
    // +0xAE8 stage 6B ring effect (fixed slot 12, 06 §5.1); None = 0.
    ring_6b: Option<usize>,
    // +0xAEC current, +0xAF8 start, +0xB04 target, +0xB10 duration, +0xB14 elapsed (06 §8).
    pub fog: FogState,
    fog_start: FogState,
    fog_target: FogState,
    fog_dur: i32,
    fog_elapsed: Timer,
    // +0x6260 written by ECL op 147 (06 §4.1).
    pub section_request: i32,
    pub cam: StageCamera,
    // +0x6464 set by op 4, consumed by the next op 5.
    jumped: bool,
    // +0x6474 op-33 sway mode, +0x6424 sway clock (06 §7).
    sway_mode: u8,
    sway_clock: Timer,
    // +0x6470 cull distance² (06 §2 step 6).
    cull_dist2: f32,
    // Port-only: TH08_STAGE_FRAME debug fast-forward.
    pub fast_forward: u32,
    // +0x844 overlay/tint VM, +0x834 youkai latch, +0x838 switch timer (06 §13).
    overlay: AnmVm,
    overlay_mode: u8,
    overlay_timer: Timer,
    // +0xB24: 0 off, 1 spell bg starting, 2 stage layers hidden (13 §5).
    pub spell_state: i32,
    // cfg effect level (list-B effects only at 2).
    pub effect_level: u8,
    // +0xB28 frames since Stage_StartSpellBg.
    spell_frames: i32,
    // +0xB30 count, +0xB34 first script, +0xB38 VMs (13 §5).
    pub spell_count: usize,
    pub spell_first: usize,
    spell_vms: Vec<AnmVm>,
    // +0x625C mask callback 0x4235A0 set (07 §12.1).
    pub spell_mask: bool,
    // HUD cover FUN_00437d87 (06 §10.2): impl+0x398C ≥ 0 and VM impl+0x3778 waiting for an interrupt.
    // Set by the HUD once VM impl+0x3778 (msg op 9 stage-clear capture, RE-Q32 CODE BUG 12) is ported.
    pub hud_cover: bool,
    // +0x6480 sparkle points: count, screen positions (06 §11.1); written by the draw step, read by the next calc.
    sparkles: (usize, [Vec3; 32]),
    // +0x647C sparkle flag, cleared at the end of Stage_DrawHigh (06 §10.2).
    sparkle_on: bool,
    // Port-only: draws recorded by the DrawLow / DrawHigh steps, replayed by their emits (ARCH decision 2).
    drawn: [Vec<Item>; 2],
}

// One recorded command of a stage draw node, in draw order. VMs are snapshots at draw time: object instances share
// their quads' VMs, and the spell mask redraws spell VM 0 / 1 modified.
#[derive(Clone)]
enum Item {
    Viewport,
    AnmReset,
    DepthTest(bool),
    ClearDepth([u8; 4]),
    ClearViewport([u8; 4]),
    ClearZ(u32),
    // Spell-start fade rect (32, 16)–(416, 464), RGBA.
    Fade([u8; 4]),
    // DrawByMode `mode` at vertex depth `z`.
    Vm {
        vm: AnmVm,
        mode: i32,
        z: f32,
    },
    // Draw3DWorld through the stage camera with the tinted fog (06 §11.2).
    World {
        vm: AnmVm,
        cam: Camera,
        fog: Fog,
    },
    // DrawCustomQuad (06 §11.3): `vm` gives blend and z-write.
    Ribbon {
        vm: AnmVm,
        tex: crate::drawlist::TexId,
        v: [Vertex; 4],
    },
    // Stage 6B ring, effect slot (06 §5.1).
    Ring(usize),
    Background(crate::effect::BgDraw),
    // Spell mask frame (07 §12.1): inner / outer half sizes, ring centre.
    MaskFrame {
        a: f32,
        b: f32,
        c: Vec3,
    },
}

impl Stage {
    // Stage_Load step 5–6 defaults (06 §2).
    pub fn new(index: usize, std: Std) -> Self {
        let cull_dist2 = match index {
            5 => 1_822_500.0,
            6 | 7 => 3_240_000.0,
            _ => 1_322_500.0,
        };
        let fog = FogState {
            near: 200.0,
            far: 500.0,
            color: 0xFF00_0000,
        };
        Self {
            index,
            obj_flags: vec![1; std.objects.len()],
            vms: vec![AnmVm::default(); std.quad_count],
            std: Arc::new(std),
            vm_a: AnmVm::default(),
            vm_b: AnmVm::default(),
            vm_c: AnmVm::default(),
            clock: Timer::default(),
            ip: 0,
            frame: 0,
            origin: [0.0; 3],
            clear_color: 0,
            ring_6b: None,
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
            spell_state: 0,
            effect_level: 2,
            spell_frames: 0,
            spell_count: 2,
            spell_first: 0,
            spell_vms: Vec::new(),
            spell_mask: false,
            hud_cover: false,
            sparkles: (0, [[0.0; 3]; 32]),
            sparkle_on: false,
            drawn: [Vec::new(), Vec::new()],
            fast_forward: 0,
            overlay: AnmVm::default(),
            overlay_mode: 0,
            overlay_timer: Timer::default(),
        }
    }

    // Stage_Load steps 1, 3 and 4 (06 §2; files already loaded).
    // The overlay VM (step 4) is started before the quad scripts (step 3): no script draws RNG, order is moot (06 §2).
    pub fn start(&mut self, anm: &mut AnmManager, rng: &mut crate::rng::Rng) {
        anm.set_script(rng, &mut self.overlay, OVERLAY_SLOT, OVERLAY_SCRIPT);
        self.overlay.pending_interrupt = 2;
        self.overlay_mode = 0;
        self.clock.set(0);
        self.ip = 0;
        for q in self.std.objects.iter().flat_map(|o| &o.quads) {
            if let Some(vm) = self.vms.get_mut(q.vm) {
                anm.set_script(rng, vm, ANM_SLOT, q.script.max(0) as usize);
            }
        }
    }

    // Stage_Register: the stage becomes `ctx.stage` (replacing the last one) plus its four nodes.
    pub fn register(ctx: &mut Ctx, index: usize) -> Result<crate::chain::Node> {
        let (std_name, bg) = files(index, ctx.game.spell_practice());
        let std = stdfile::parse(&ctx.archive.read(&std_name)?)?;
        ctx.anm.load(&mut ctx.rec, &ctx.archive, ANM_SLOT, &bg)?;
        let mut st = Stage::new(index, std);
        st.effect_level = ctx.game.cfg.effect_level;
        st.fast_forward = ctx.core.stage_frame;
        st.start(&mut ctx.anm, &mut ctx.rng);
        ctx.stage = Some(Box::new(st));
        let t = crate::chain::Node::Stage;
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t)?;
        task::add(ctx, Chain::Draw, DRAW_LOW_PRIO, 0, t)?;
        task::add(ctx, Chain::Draw, DRAW_HIGH_PRIO, 1, t)?;
        // Port-only tail of Stage_DrawHigh (step only): its last writes come after the node's emit, see `draw_step`.
        task::add(ctx, Chain::Draw, DRAW_HIGH_PRIO, 2, t)?;
        Ok(t)
    }

    // Ops 29 / 30 / 34 (06 §4): start a script on VM A / B / C, or disable it (sprite index −1).
    fn bg_vm(&mut self, anm: &mut AnmManager, rng: &mut crate::rng::Rng, which: u8, script: i32) {
        let vm = match which {
            0 => &mut self.vm_a,
            1 => &mut self.vm_b,
            _ => &mut self.vm_c,
        };
        if script >= 0 {
            anm.set_script(rng, vm, ANM_SLOT, script as usize);
        } else if which == 2 {
            self.vm_c.sprite_index = -1;
        } else {
            // Quirk kept (06 §4): op 30 with a negative arg disables VM A, not VM B.
            self.vm_a.sprite_index = -1;
        }
    }

    fn fog_from(c: u32, near: f32, far: f32) -> FogState {
        FogState {
            near,
            far,
            color: c,
        }
    }

    // One record of the Stage_Calc switch (06 §4); false when processing stops for this frame (op 3).
    fn exec(&mut self, anm: &mut AnmManager, r: Record, sys: &mut Sys) -> bool {
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
            // Jump: cancels only the position interpolation; continues in the same frame.
            4 => {
                self.ip = r.i(0).max(0) as usize;
                self.clock.set(r.i(1));
                cam.dur[POS] = 0;
                self.jumped = true;
                return true;
            }
            5 | 7 | 9 => {
                let s = ((r.op - 5) / 2) as usize;
                // After a jump, kind-51 background particles shift by the target change (06 §4, 13 §2.6a).
                if s == POS && self.jumped {
                    let d = sub(r.v(), cam.target.v[s]);
                    sys.fx.shift_kind(51, d);
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
            // 14–18 / 19–23 / 24–28: start, end, tan0, tan1, Hermite (mode 7) for pos / facing / up.
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
            29 => self.bg_vm(anm, sys.rng, 0, r.i(0)),
            30 => self.bg_vm(anm, sys.rng, 1, r.i(0)),
            34 => self.bg_vm(anm, sys.rng, 2, r.i(0)),
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

    // Stage_Calc head for g_StageIdx 7 (06 §5.1): spawn the ring, else drive it with section requests 1–4.
    fn ring_6b_step(&mut self, anm: &mut AnmManager, sys: &mut Sys) {
        let Some(i) = self.ring_6b else {
            let i = sys
                .fx
                .spawn_fixed(anm, sys.rng, 0x40, [0.0; 3], 12, 0xFFFF_FFFF);
            anm.file_set_script(sys.rng, &mut sys.fx.fx[i].vm, ANM_SLOT, 11);
            self.ring_6b = Some(i);
            return;
        };
        let req = self.section_request;
        if !(1..=4).contains(&req) {
            return;
        }
        // Writes through the stored pointer, no active check.
        {
            let vm = &mut sys.fx.fx[i].vm;
            if req == 1 {
                anm.file_set_script(sys.rng, vm, ANM_SLOT, 11);
                return;
            }
            // Requests 2–4: copy the VM, (2: script 12), interrupt, restore pos interpolation slot 0 and colour.
            let k = vm.clone();
            if req == 2 {
                anm.file_set_script(sys.rng, vm, ANM_SLOT, 12);
            }
            vm.pending_interrupt = req as u16;
            (vm.pos_end, vm.pos_start) = (k.pos_end, k.pos_start);
            vm.interp_elapsed[0] = k.interp_elapsed[0];
            vm.interp_duration[0] = k.interp_duration[0];
            vm.interp_mode[0] = k.interp_mode[0];
            vm.color = k.color;
        }
    }

    // 06 §4.1: jump past the op 31 whose arg matches; an unmatched request stays pending.
    fn handle_section_request(&mut self) {
        if self.section_request == 0 {
            return;
        }
        if let Some(k) = self
            .std
            .script
            .iter()
            .position(|r| r.time != -1 && r.op == 31 && r.i(0) == self.section_request)
        {
            self.ip = k + 1;
            self.clock.set(self.std.script[k].time);
            self.section_request = 0;
        }
    }

    // 06 §7: φ = 2πc/P − π (written c·π + c·π as in the original); clock wraps to 0 at P.
    fn sway(&mut self, speed: f32) {
        let c = self.sway_clock.getf();
        let (p, wrap) = match self.sway_mode {
            1 | 2 => (480.0, 480),
            3 => (4800.0, 4800),
            _ => return,
        };
        let phi = (c * PI + c * PI) / p - PI;
        // Each value is f32(sin φ · k) rounded once from the extended product (06 §7, RE-Q58 AS).
        let s = |k: f64| crate::fmath::sin_mul(phi, k);
        match self.sway_mode {
            1 => self.cam.eye_offset[0] = s(40.0),
            2 => {
                self.cam.eye_offset[0] = s(70.0);
                self.cam.v[UP][0] = -s(0.1);
            }
            _ => {
                self.cam.v[UP][0] = s(1.0);
                self.cam.v[UP][2] = crate::fmath::cos_f(phi);
            }
        }
        self.sway_clock.tick(speed);
        if self.sway_clock.cur >= wrap {
            self.sway_clock.set(0);
        }
    }

    // 06 §8: each colour byte truncated; near/far linear; done when elapsed ≥ duration.
    fn update_fog(&mut self, speed: f32) {
        if self.fog_dur == 0 {
            return;
        }
        self.fog_elapsed.tick(speed);
        let d = self.fog_dur as f32;
        let t = (self.fog_elapsed.getf() / d).min(1.0);
        let (a, b) = (
            self.fog_start.color.to_le_bytes(),
            self.fog_target.color.to_le_bytes(),
        );
        let c: [u8; 4] =
            std::array::from_fn(|k| ((b[k] as f32 - a[k] as f32) * t + a[k] as f32) as u8);
        self.fog.color = u32::from_le_bytes(c);
        self.fog.near = (self.fog_target.near - self.fog_start.near) * t + self.fog_start.near;
        self.fog.far = (self.fog_target.far - self.fog_start.far) * t + self.fog_start.far;
        if self.fog_elapsed.getf() >= d {
            self.fog_dur = 0;
        }
    }

    // Stage_UpdateObjects (06 §9): the overlay VM follows the player mode byte (human blue, youkai white).
    fn update_objects(&mut self, anm: &mut AnmManager, rng: &mut crate::rng::Rng, mode: u8) {
        if (mode != 0) != (self.overlay_mode != 0) {
            self.overlay_mode = (mode != 0) as u8;
            self.overlay.pending_interrupt = if mode != 0 { 1 } else { 2 };
            self.overlay_timer = Timer::default();
        }
        if self.overlay.script.is_some() {
            self.overlay.run(anm, rng);
        }
        self.overlay_timer.tick(anm.speed);
        // Port-only: white when text.anm is not loaded (unit tests).
        let overlay = if self.overlay.has_sprite {
            self.overlay.color
        } else {
            [255u8; 4]
        };
        for (o, obj) in self.std.objects.iter().enumerate() {
            if self.obj_flags[o] & 1 == 0 {
                continue;
            }
            let mut running = 0;
            for q in &obj.quads {
                if !self.vms[q.vm].run(anm, rng) {
                    running += 1;
                }
            }
            // Only the last quad's VM gets the tag-1 colour2 = colour × overlay / 256.
            if let Some(last) = obj.quads.last() {
                let vm = &mut self.vms[last.vm];
                if vm.tag == 1 {
                    vm.flags |= anmvm::F_COLOR2;
                    vm.color2 = std::array::from_fn(|k| {
                        ((vm.color[k] as u32 * overlay[k] as u32) / 256) as u8
                    });
                }
            }
            if running == 0 {
                self.obj_flags[o] &= !1;
            }
        }
    }

    // Stage_Calc body (06 §5).
    pub fn step(&mut self, anm: &mut AnmManager, sys: &mut Sys) {
        // GameFlags bit 10 (bullets frozen): nothing runs, the sparkle flag is not set and the frame counter holds.
        if sys
            .player
            .as_deref()
            .is_some_and(|p| p.freeze_bullets(*sys.flags))
        {
            return;
        }
        let speed = anm.speed;
        if self.index == 7 {
            self.ring_6b_step(anm, sys);
        }
        self.handle_section_request();
        while let Some(&r) = self.std.script.get(self.ip) {
            if r.time == -1 || self.clock.cur < r.time || !self.exec(anm, r, sys) {
                break;
            }
        }
        self.cam.update(speed);
        self.sway(speed);
        self.update_fog(speed);
        // The clock does not advance while the current record is a halt (op 3).
        if self.std.script.get(self.ip).is_none_or(|r| r.op != 3) {
            self.clock.tick(speed);
        }
        let mode = sys.player.as_deref().map_or(0, |p| p.mode);
        self.update_objects(anm, sys.rng, mode);
        // 13 §5: spell background; after 60 frames the stage layers are no longer drawn.
        if self.spell_state > 0 {
            if self.spell_frames == 60 {
                self.spell_state = 2;
            }
            self.spell_frames += 1;
            for vm in self.spell_vms.iter_mut().filter(|v| v.script.is_some()) {
                vm.run(anm, sys.rng);
            }
        }
        // Port-only hand-off: camera and overlay colour for the effect manager (13 §2.6a, §2.6c).
        sys.fx.view = crate::effect::View {
            cam: Some(self.cam.camera(PLAYFIELD)),
            pos: self.cam.v[POS],
            eye: self.cam.eye(),
            facing: self.cam.v[FACING],
            facing_norm: self.cam.facing_norm,
        };
        let overlay = self.overlay.color;
        sys.fx.overlay = overlay;
        if self.vm_a.sprite_index > 0 {
            self.vm_a.run(anm, sys.rng);
        }
        if self.vm_b.sprite_index > 0 {
            self.vm_b.run(anm, sys.rng);
        }
        if self.vm_c.sprite_index > 0 {
            self.vm_c.run(anm, sys.rng);
            self.clear_color = u32::from_le_bytes(self.vm_c.color);
        }
        // 06 §5 quirk kept: sparkles at all 12 points, used or not (unused ones at screen (0, 0)).
        if self.sparkle_due(*sys.flags) {
            let pts = self.sparkles.1;
            for p in &pts[..12] {
                let i = sys.fx.spawn(anm, sys.rng, 0x3E, *p, 1, 0x20FFFFFF);
                sys.fx.get(i, |e| e.layer = 4);
            }
        }
        // Sparkle flag +0x647C (06 §5); cleared at the end of Stage_DrawHigh.
        self.sparkle_on = true;
        self.frame += 1;
    }

    // Clone test state (worldclone.rs): script position, clocks, camera, fog, sparkle points as raw bits.
    #[cfg(test)]
    pub fn trace(&self) -> Vec<u64> {
        let mut t = vec![
            self.frame as u64,
            self.ip as u64,
            self.clock.cur as u64,
            self.section_request as u64,
            self.clear_color as u64,
            self.sparkle_on as u64,
            self.sparkles.0 as u64,
            self.fog.near.to_bits() as u64,
            self.fog.color as u64,
            self.fog_elapsed.cur as u64,
        ];
        t.extend(self.cam.v.iter().flatten().map(|v| v.to_bits() as u64));
        t
    }

    // 06 §10.2: stage layers, list-B effects and the fade rect are skipped while +0xB24 ≥ 2 or the HUD cover is on.
    fn layers_hidden(&self) -> bool {
        self.spell_state >= 2 || self.hud_cover
    }

    // 06 §5: every 3rd frame, from frame 700 (from frame 0 in spell practice, GameFlags bit 14), while +0xB24 < 2.
    fn sparkle_due(&self, flags: u32) -> bool {
        let practice = flags & crate::gamemgr::F_SPELL_PRACTICE != 0;
        self.frame % 3 == 0 && (self.frame > 699 || practice) && self.spell_state < 2
    }

    // Stage_StartSpellBg 0x415CE0 (13 §5): spell VMs run scripts first..first+count of the slot-9 file.
    pub fn start_spell_bg(&mut self, anm: &mut AnmManager, rng: &mut crate::rng::Rng) {
        self.spell_state = 1;
        self.spell_frames = 0;
        self.spell_mask = false;
        self.spell_vms = (0..self.spell_count)
            .map(|i| {
                let mut vm = AnmVm::default();
                anm.set_script(rng, &mut vm, crate::effect::BG_SLOT, self.spell_first + i);
                vm
            })
            .collect();
    }

    // Stage_SpellMaskCallback 0x4235A0 (07 §12.1): black square frame between the native 3/6/20 rings,
    // inside it the spell background redrawn mirrored.
    fn spell_mask_step(
        &mut self,
        anm: &AnmManager,
        fx: &crate::effect::Effects,
        out: &mut Vec<Item>,
    ) {
        let ra = fx.peek(crate::effect::FIXED_BASE + 9, |e| (e.vm.pos[0], e.pos));
        let rb = fx.peek(crate::effect::FIXED_BASE + 10, |e| e.vm.pos[0]);
        let ((a, c), b) = (ra, rb);
        out.push(Item::MaskFrame {
            a: a * std::f32::consts::FRAC_1_SQRT_2,
            b: b * std::f32::consts::FRAC_1_SQRT_2,
            c,
        });
        out.push(Item::AnmReset);
        out.push(Item::DepthTest(true));
        // vm.color is stored B,G,R,A (D3DCOLOR little-endian), so the bytes are used as they are (07 §12.1).
        let grey = 0xFFE0C0C0u32.to_le_bytes();
        // The restore writes fixed values, not the saved scale / pos; the dirty bits stay set (07 §12.1).
        if let Some(vm) = self.spell_vms.get_mut(0) {
            let col = vm.color;
            vm.scale = [-1.5, -1.75];
            vm.pos = [416.0, 464.0, 0.7];
            vm.color = grey;
            vm.flags |= anmvm::F_SCALE_DIRTY;
            anm.prepare(vm, 2);
            out.push(Item::Vm {
                vm: vm.clone(),
                mode: 2,
                z: 0.7,
            });
            vm.scale = [1.5, 1.75];
            vm.pos = [32.0, 16.0, 0.5];
            vm.color = col;
            vm.flags |= anmvm::F_SCALE_DIRTY;
        }
        // Step 5: VM 1 with rotation z negated at z 0.6.
        if let Some(vm) = self.spell_vms.get_mut(1) {
            let col = vm.color;
            vm.rotation[2] = -vm.rotation[2];
            vm.flags |= anmvm::F_ROT_DIRTY;
            vm.color = grey;
            anm.prepare(vm, 2);
            out.push(Item::Vm {
                vm: vm.clone(),
                mode: 2,
                z: 0.6,
            });
            vm.rotation[2] = -vm.rotation[2];
            vm.flags |= anmvm::F_ROT_DIRTY;
            vm.color = col;
        }
        out.push(Item::DepthTest(false));
    }

    // ECL native 12: interrupt 2 (pause) / 1 to spell VMs 0 and 1 (07 §12).
    pub fn spell_interrupt(&mut self, k: u16) {
        for vm in self.spell_vms.iter_mut().take(2) {
            vm.pending_interrupt = k;
        }
    }

    // Spell_End 0x416AD0: +0xB24 = 0 (13 §5).
    pub fn end_spell_bg(&mut self) {
        self.spell_state = 0;
        self.spell_vms.clear();
    }

    // Stage_DrawLow step 6: fog colour × global ANM tint when active (06 §8, §10.2).
    fn fog_params(&self, anm: &AnmManager) -> Fog {
        let c = self.fog.color.to_le_bytes();
        let [r, g, b, _] = anm.tinted([c[2], c[1], c[0], c[3]]);
        Fog {
            near: self.fog.near,
            far: self.fog.far,
            color: [r, g, b],
        }
    }

    // Stage_DrawLayer 0x40A1B0 (06 §10.3), draw-phase step. The viewport is the playfield: DrawLow sets it and
    // DrawHigh (prio 7) follows it with no draw node between (screen effects draw at prio 1 / 0x10 / 0x15).
    fn layer_step(&mut self, anm: &mut AnmManager, layer: u8, out: &mut Vec<Item>) {
        let vp = PLAYFIELD;
        let cam = self.cam.camera(vp);
        anm.stage_camera = Some(cam);
        // Only Stage_SetCamera writes 0x4EA3F4: while the layers are skipped it keeps the last drawn frame's value.
        anm.cam_right = self.cam.right();
        let m = cam.view.0;
        let right = normalize([m[0][0], m[0][1], m[0][2]]);
        let fog = self.fog_params(anm);
        // +0xB24 ≤ 1 is redundant: Stage_DrawLayer runs only while +0xB24 < 2 (06 §11.1, §10.2).
        let points = if self.sparkle_on && self.spell_state <= 1 {
            Some(&mut self.sparkles)
        } else {
            None
        };
        let mut view = View {
            cam,
            vp,
            right,
            eye: self.cam.eye(),
            fog: self.fog,
            points,
            pv: [0.0; 3],
        };
        // Reused vertex-colour slots; initial values = stack residue (06 §11.3, INFERRED constant).
        let mut slots = [0x00910D20u32, 0x00676E85, 0, 0].map(u32::to_le_bytes);
        for inst in &self.std.instances {
            let obj = &self.std.objects[inst.object];
            if obj.layer != layer {
                continue;
            }
            let centre = add(
                sub(add(obj.pos, inst.pos), self.origin),
                scale(obj.size, 0.5),
            );
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
                        // The sprite size ≠ 0 tests are port-only (division guard).
                        if w != 0.0 && vm.sprite_w != 0.0 {
                            vm.scale[0] = w / vm.sprite_w;
                        }
                        if h != 0.0 && vm.sprite_h != 0.0 {
                            vm.scale[1] = h / vm.sprite_h;
                        }
                        if vm.tag & 0xF == 2 {
                            view.billboard(anm, vm, w, out);
                        } else {
                            anm.prepare(vm, 5);
                            out.push(Item::World {
                                vm: vm.clone(),
                                cam,
                                fog,
                            });
                        }
                    }
                    QuadKind::Ribbon { a, b, w } => {
                        let pa = sub(add(a, inst.pos), self.origin);
                        let pb = sub(add(b, inst.pos), self.origin);
                        view.ribbon(anm, vm, &mut slots, pa, pb, w, out);
                    }
                }
            }
        }
    }
}

// Per-Stage_DrawLayer state: camera, viewport, right axis (view row 0), eye, fog, sparkle sink.
struct View<'a> {
    cam: Camera,
    vp: Rect,
    right: Vec3,
    eye: Vec3,
    fog: FogState,
    points: Option<&'a mut (usize, [Vec3; 32])>,
    // Local ebp−0x9C: D3DXVec3Project input vector of every projection, (0,0,0) per call, set by each ribbon
    // (06 §11.4 QUIRK).
    pv: Vec3,
}

impl View<'_> {
    // D3DXVec3Project with world = translation(p) and input `pv`: project(p + pv) (06 §11.4).
    fn project(&self, p: Vec3) -> Vec3 {
        self.cam.project(add(p, self.pv), self.vp)
    }

    // Software fog (06 §11.1), D3DCOLOR byte order: c −= trunc((c − fog)·f), alpha ·= 1 − f; None when f ≥ 1.
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

    // Billboard sprite, tag & 0xF == 2 (06 §11.1).
    fn billboard(&mut self, anm: &AnmManager, vm: &mut AnmVm, w: f32, out: &mut Vec<Item>) {
        let s = self.project(vm.pos);
        let ww = if w != 0.0 { w } else { vm.sprite_w };
        // Quirk kept (06 §13): uses the previous frame's billboard scale.x (feedback); 3D length of s2 − s.
        let s2 = self.project(add(vm.pos, scale(self.right, ww * vm.scale[0])));
        let d = sub(s2, s);
        let sc = dot(d, d).sqrt() / ww;
        vm.scale = [sc, if ww < 0.0 { -sc } else { sc }];
        let dist = self.dist(vm.pos);
        let saved = vm.color;
        let Some(c) = self.soft_fog(vm.color, dist) else {
            return;
        };
        vm.color = c;
        vm.pos = s;
        if (0.0..=1.0).contains(&s[2]) {
            anm.prepare(vm, 1);
            out.push(Item::Vm {
                vm: vm.clone(),
                mode: 1,
                z: s[2],
            });
            // Only drawn quads append a sparkle point (06 §11.1).
            if vm.tag & 0xF0 == 0x10
                && let Some(p) = self.points.as_deref_mut()
            {
                if sparkle_point_ok(dist, self.fog) {
                    // Port-only bound: the original has no check (the shipped data stays ≤ 15 points, 06 §11.1).
                    if p.0 < 32 {
                        let n = p.0;
                        p.1[n] = [s[0], s[1], 0.0];
                        p.0 += 1;
                    }
                }
            }
        }
        vm.color = saved;
    }

    // Colours go to the call's reused slots first; f ≥ 1 clears only alpha (06 §11.3). The projections and the `pv`
    // update (06 §11.4) come before the depth and DrawCustomQuad checks.
    #[allow(clippy::too_many_arguments)]
    fn ribbon(
        &mut self,
        anm: &AnmManager,
        vm: &AnmVm,
        slots: &mut [[u8; 4]; 4],
        a: Vec3,
        b: Vec3,
        w: f32,
        out: &mut Vec<Item>,
    ) {
        use anmvm::*;
        let base = vm.color;
        for (p, k) in [(a, 0), (b, 2)] {
            for s in &mut slots[k..k + 2] {
                match self.soft_fog(base, self.dist(p)) {
                    Some(c) => *s = c,
                    None => s[A] = 0,
                }
            }
        }
        let ww = if w != 0.0 { w } else { vm.sprite_w };
        let point = |p: Vec3| {
            let sp = self.project(p);
            let e = sub(self.project(add(p, scale(self.right, ww))), sp);
            (sp, dot(e, e).sqrt() / 2.0)
        };
        let (sa, wa) = point(a);
        let (sb, wb) = point(b);
        let d = sub(sb, sa);
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        // `Vec3_ScaleInv` 0x40B470: one reciprocal, three multiplies; len < 1e-5 keeps the raw difference.
        self.pv = if len >= 1e-5 { scale(d, 1.0 / len) } else { d };
        if vm.flags & (F_VISIBLE | F_ENABLED) != (F_VISIBLE | F_ENABLED)
            || vm.color[A] == 0
            || !vm.has_sprite
        {
            return;
        }
        let Some(&spr) = anm.sprite(vm.anm_slot as usize, vm.sprite_index as usize) else {
            return;
        };
        // `AnmManager_DrawCustomQuad` 0x464DD0 pushes the colours as given: no global tint (06 §11.3, 05 §12.1).
        let col = |c: [u8; 4]| [c[R], c[G], c[B], c[A]];
        let (c0, c1, c2, c3) = (col(slots[0]), col(slots[1]), col(slots[2]), col(slots[3]));
        if len < 1e-5 || !(0.0..=1.0).contains(&sa[2]) || !(0.0..=1.0).contains(&sb[2]) {
            return;
        }
        let (dx, dy) = (d[0] / len, d[1] / len);
        let (u0, v0) = (spr.uv[0] + vm.uv_off[0], spr.uv[1] + vm.uv_off[1]);
        let (u1, v1) = (spr.uv[2] + vm.uv_off[0], spr.uv[3] + vm.uv_off[1]);
        // DrawCustomQuad / PushQuad add no screen-shake offset (06 §11.3).
        let vx = |x: f32, y: f32, z: f32, uv: [f32; 2], c: [u8; 4]| {
            Vertex::screen(x + 0.5, y + 0.5, z, uv, c)
        };
        let v = [
            vx(sa[0] + dy * wa, sa[1] - dx * wa, sa[2], [u0, v0], c0),
            vx(sa[0] - dy * wa, sa[1] + dx * wa, sa[2], [u1, v0], c1),
            vx(sb[0] + dy * wb, sb[1] - dx * wb, sb[2], [u0, v1], c2),
            vx(sb[0] - dy * wb, sb[1] + dx * wb, sb[2], [u1, v1], c3),
        ];
        out.push(Item::Ribbon {
            vm: vm.clone(),
            tex: spr.tex,
            v,
        });
    }
}

// Chain nodes of `ctx.stage` (calc prio 8, draw prio 6 / 7 / 7): stateless forwarder (STEP9_WORLD.md §3).
pub struct StageTask;

impl Task<Ctx> for StageTask {
    // Port-only: unloads stgNbg.anm when the stage calc node goes away.
    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc {
            ctx.anm.unload(&mut ctx.rec, ANM_SLOT);
        }
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let st = ctx.stage.as_deref_mut().expect("stage");
        let (p, b) = (ctx.player.as_deref_mut(), ctx.bullets.as_deref_mut());
        let sys = &mut crate::sys!(ctx; p; b; None);
        for _ in 0..std::mem::take(&mut st.fast_forward) {
            st.step(&mut ctx.anm, sys);
        }
        st.step(&mut ctx.anm, sys);
        Ret::Next
    }

    fn draw_step(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        let st = ctx.stage.as_deref_mut().expect("stage");
        st.draw_step(&mut ctx.anm, &mut ctx.effects, tag);
        Ret::Next
    }

    fn emit(&self, ctx: &Ctx, rec: &mut Recorder, tag: u8) {
        if let Some(st) = ctx.stage.as_deref() {
            st.emit(ctx, rec, tag);
        }
    }
}

impl Stage {
    // Tag 0 = Stage_DrawLow, tag 1 = Stage_DrawHigh (06 §10.2), tag 2 = DrawHigh's last writes. The tint off and
    // stage-camera reset end DrawHigh but its draws read the tint: tag 2, linked right after tag 1 at the same prio,
    // makes them after tag 1's emit and before any other draw node.
    // Not implemented: DrawLow step 2 (+0xB2C clear).
    pub fn draw_step(&mut self, anm: &mut AnmManager, fx: &mut crate::effect::Effects, tag: u8) {
        if tag == 2 {
            self.sparkle_on = false;
            // +0x646C: the tint stays on while a bomb drew last frame (06 §10.2).
            if !std::mem::take(&mut anm.bomb_drew) {
                anm.tint_off();
            }
            anm.stage_camera = None;
            return;
        }
        let mut out = std::mem::take(&mut self.drawn[tag as usize]);
        out.clear();
        if tag == 0 {
            self.sparkles.0 = 0;
            self.sparkles.1[..16].fill([0.0; 3]);
            out.push(Item::Viewport);
            out.push(Item::AnmReset);
            // Step 3: last frame's bomb tint request (14 §1.3) turns the tint on.
            if let Some(c) = anm.stage_tint.take() {
                (anm.tint, anm.tint_on) = (c, true);
            }
            let hide = self.layers_hidden();
            for vm in [&mut self.vm_a, &mut self.vm_b] {
                if vm.sprite_index > 0 && !hide {
                    anm.prepare(vm, 2);
                    out.push(Item::Vm {
                        vm: vm.clone(),
                        mode: 2,
                        z: vm.pos[2],
                    });
                }
            }
            if let Some(i) = self.ring_6b.filter(|_| !hide) {
                out.push(Item::Ring(i));
            }
            // Step 5: alpha 0xFF → clear target + z; 0 → z only; else blend-fill the playfield, then z.
            let c = self.clear_color.to_le_bytes();
            let rgba = [c[2], c[1], c[0], c[3]];
            if c[3] == 0xFF {
                out.push(Item::ClearDepth(rgba));
            } else {
                if self.clear_color != 0 {
                    out.push(Item::ClearViewport(rgba));
                }
                // The z-only clear passes clear_color (+0x830) as its ignored colour argument.
                out.push(Item::ClearZ(self.clear_color));
            }
            out.push(Item::DepthTest(true));
            if !hide {
                self.layer_step(anm, 0, &mut out);
                self.layer_step(anm, 1, &mut out);
            }
        } else {
            if !self.layers_hidden() {
                self.layer_step(anm, 2, &mut out);
                self.layer_step(anm, 3, &mut out);
                let mut bg = Vec::new();
                fx.background_step(anm, self.effect_level, &mut bg);
                out.extend(bg.into_iter().map(Item::Background));
                if self.spell_state == 1 {
                    // Spell-start fade: black, alpha +0xB28·255/60, ZFUNC ALWAYS (Screen_FillRect 0x45B1E0).
                    out.push(Item::AnmReset);
                    out.push(Item::DepthTest(false));
                    out.push(Item::Fade([0, 0, 0, (self.spell_frames * 255 / 60) as u8]));
                }
            }
            out.push(Item::DepthTest(false));
            out.push(Item::Viewport);
            // AnmManager_DrawRotFlush 0x40BAF0 = Draw2DRot (mode 2) whatever the VM's draw mode (13 §5).
            for vm in &mut self.spell_vms {
                anm.prepare(vm, 2);
                out.push(Item::Vm {
                    vm: vm.clone(),
                    mode: 2,
                    z: vm.pos[2],
                });
            }
            if self.spell_mask && self.spell_state > 0 {
                self.spell_mask_step(anm, fx, &mut out);
            }
            out.push(Item::Viewport);
        }
        self.drawn[tag as usize] = out;
    }

    pub fn emit(&self, ctx: &Ctx, rec: &mut Recorder, tag: u8) {
        let anm = &ctx.anm;
        for it in self.drawn.get(tag as usize).into_iter().flatten() {
            match it {
                Item::Viewport => rec.set_viewport(PLAYFIELD),
                Item::AnmReset => rec.anm.reset(),
                Item::DepthTest(on) => rec.set_depth_test(*on),
                Item::ClearDepth(c) => rec.clear_depth(Some(*c)),
                Item::ClearViewport(c) => rec.clear_viewport(*c),
                Item::ClearZ(arg) => rec.clear_z(*arg),
                Item::Fade(c) => rec.rect(32.0, 16.0, 384.0, 448.0, *c),
                Item::Vm { vm, mode, z } => anm.emit_mode_z(rec, vm, *mode, *z),
                Item::World { vm, cam, fog } => anm.emit_3d_world(rec, vm, cam, Some(*fog)),
                Item::Ribbon { vm, tex, v } => {
                    let blend = AnmManager::blend_for(rec, vm);
                    rec.anm.bound_tex = Some(*tex);
                    rec.strip(*tex, blend, vm.flags & anmvm::F_ZWRITE_OFF == 0, *v);
                }
                Item::Ring(i) => crate::effect::draw_ring_at(&ctx.effects, anm, rec, *i),
                Item::Background(d) => ctx.effects.emit_background(anm, rec, d),
                Item::MaskFrame { a, b, c } => emit_mask_frame(rec, *a, *b, *c),
            }
        }
    }
}

// Spell mask frame (07 §12.1): four black strips between half sizes a and b around the ring centre, z 0.8.
fn emit_mask_frame(rec: &mut Recorder, a: f32, b: f32, c: Vec3) {
    // +0.5: port D3D → wgpu pixel-centre offset.
    let (cx, cy) = (c[0] + 32.0 + 0.5, c[1] + 16.0 + 0.5);
    let ring = [
        (-1.0, -1.0),
        (1.0, -1.0),
        (1.0, 1.0),
        (-1.0, 1.0),
        (-1.0, -1.0),
    ];
    let white = rec.white();
    rec.set_depth_test(false);
    for k in 0..4 {
        let v = [
            (ring[k], a),
            (ring[k], b),
            (ring[k + 1], a),
            (ring[k + 1], b),
        ]
        .map(|((sx, sy), r)| {
            Vertex::screen(cx + sx * r, cy + sy * r, 0.8, [0.0, 0.0], [0, 0, 0, 255])
        });
        rec.strip(white, crate::drawlist::Blend::Alpha, true, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 13 §2.6c: right = normalise(cross(raw facing, up)), independent of the eye offset.
    #[test]
    fn camera_right_from_raw_facing() {
        let mut c = StageCamera::default();
        c.v[FACING] = [0.0, 500.0, 300.0];
        c.v[UP] = [0.0, 0.0, 1.0];
        c.eye_offset = [40.0, 0.0, 0.0];
        assert_eq!(c.right(), [1.0, 0.0, 0.0]);
    }

    fn rec(time: i32, op: i16, a: [u32; 3]) -> Record {
        Record { time, op, args: a }
    }

    fn fl(v: [f32; 3]) -> [u32; 3] {
        v.map(f32::to_bits)
    }

    fn synthetic(script: Vec<Record>) -> Stage {
        let std = Std {
            quad_count: 0,
            bgm: Default::default(),
            objects: Vec::new(),
            instances: Vec::new(),
            script,
        };
        Stage::new(0, std)
    }

    #[test]
    fn camera_interpolation_and_jump() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let mut st = synthetic(vec![
            rec(0, 5, fl([0.0, 0.0, 0.0])),
            rec(0, 6, [10, 0, 0]),
            rec(0, 5, fl([0.0, 100.0, 0.0])),
            rec(20, 4, [3, 0, 0]),
            rec(-1, 0, [0; 3]),
        ]);
        let mut anm = AnmManager::default();
        st.step(&mut anm, sys);
        assert_eq!(st.cam.v[POS], [0.0, 10.0, 0.0]);
        for _ in 0..9 {
            st.step(&mut anm, sys);
        }
        assert_eq!(st.cam.v[POS], [0.0, 100.0, 0.0]);
        for _ in 0..30 {
            st.step(&mut anm, sys);
        }
        assert_eq!(st.clock.cur, 20);
        assert_eq!(st.ip, 3);
    }

    // 06 §11.4: every ribbon (even an invisible one) sets `pv` to its normalised screen direction, used as the
    // input vector of the next projections.
    #[test]
    fn ribbon_sets_projection_vector() {
        let mut out = Vec::new();
        let anm = AnmManager::default();
        let mut c = StageCamera::default();
        c.v[FACING] = [0.0, 500.0, -100.0];
        c.v[UP] = [0.0, 0.0, 1.0];
        let vp = PLAYFIELD;
        let cam = c.camera(vp);
        let mut view = View {
            cam,
            vp,
            right: [1.0, 0.0, 0.0],
            eye: c.eye(),
            fog: FogState::default(),
            points: None,
            pv: [0.0; 3],
        };
        let mut vm = AnmVm::default();
        vm.sprite_w = 16.0;
        let mut slots = [[0; 4]; 4];
        let (a, b) = ([0.0, 300.0, 0.0], [50.0, 400.0, 0.0]);
        view.ribbon(&anm, &vm, &mut slots, a, b, 0.0, &mut out);
        let d = sub(cam.project(b, vp), cam.project(a, vp));
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        let pv = scale(d, 1.0 / len);
        assert_eq!(view.pv, pv);
        view.ribbon(&anm, &vm, &mut slots, a, b, 0.0, &mut out);
        let d = sub(cam.project(add(b, pv), vp), cam.project(add(a, pv), vp));
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        assert_eq!(view.pv, scale(d, 1.0 / len));
    }

    // 06 §5.1: stage 7 spawns the type-0x40 ring in slot 12 on its first calc; request 3 interrupts and keeps
    // the colour; request 2 (script 12) restores the pos interpolation slot 0 and the colour.
    #[test]
    fn stage_6b_ring_requests() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let mut st = synthetic(vec![rec(-1, 0, [0; 3])]);
        st.index = 7;
        let mut anm = AnmManager::default();
        st.section_request = 3;
        st.step(&mut anm, sys);
        let i = st.ring_6b.unwrap();
        assert_eq!(i, crate::effect::FIXED_BASE + 12);
        sys.fx
            .peek(i, |e| assert_eq!((e.active, e.kind), (true, 0x40)));
        // The spawn frame does not look at the request.
        assert_eq!(st.section_request, 3);
        {
            let vm = &mut sys.fx.fx[i].vm;
            vm.color = [1, 2, 3, 4];
            vm.pos_end = [5.0, 6.0, 7.0];
            vm.interp_mode[0] = 4;
        }
        st.step(&mut anm, sys);
        sys.fx.peek(i, |e| {
            assert_eq!(e.vm.pending_interrupt, 3);
            assert_eq!(e.vm.color, [1, 2, 3, 4]);
        });
        st.section_request = 2;
        st.step(&mut anm, sys);
        sys.fx.peek(i, |e| {
            assert_eq!(e.vm.pending_interrupt, 2);
            assert_eq!(
                (e.vm.color, e.vm.pos_end, e.vm.interp_mode[0]),
                ([1, 2, 3, 4], [5.0, 6.0, 7.0], 4)
            );
        });
        sys.fx.free(i);
    }

    #[test]
    fn halt_and_section_request() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let mut st = synthetic(vec![
            rec(0, 3, [0; 3]),
            rec(0, 13, [1, 0, 0]),
            rec(50, 31, [2, 0, 0]),
            rec(50, 13, [2, 0, 0]),
            rec(-1, 0, [0; 3]),
        ]);
        let mut anm = AnmManager::default();
        for _ in 0..5 {
            st.step(&mut anm, sys);
        }
        assert_eq!((st.ip, st.clock.cur, st.clear_color), (0, 0, 0));
        st.section_request = 2;
        st.step(&mut anm, sys);
        assert_eq!(
            (st.clear_color, st.section_request, st.clock.cur),
            (2, 0, 51)
        );
    }

    #[test]
    fn fog_interpolates() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let mut st = synthetic(vec![
            rec(0, 2, [4, 0, 0]),
            rec(0, 1, [0xFF40_8000, 100f32.to_bits(), 300f32.to_bits()]),
            rec(-1, 0, [0; 3]),
        ]);
        let mut anm = AnmManager::default();
        st.step(&mut anm, sys);
        assert_eq!(st.fog.near, 175.0);
        assert_eq!(st.fog.color, 0xFF10_2000);
        for _ in 0..3 {
            st.step(&mut anm, sys);
        }
        assert_eq!(
            st.fog,
            FogState {
                near: 100.0,
                far: 300.0,
                color: 0xFF40_8000
            }
        );
        assert_eq!(st.fog_dur, 0);
    }

    #[test]
    fn frozen_bullets_skip_calc() {
        let mut ts = crate::game::TestSys::with_player(0);
        let sys = &mut ts.sys();
        let mut st = synthetic(vec![rec(0, 13, [7, 0, 0]), rec(-1, 0, [0; 3])]);
        crate::pl!(sys).set_freeze_bullets(sys.flags, true);
        let mut anm = AnmManager::default();
        st.step(&mut anm, sys);
        assert_eq!((st.ip, st.frame, st.clear_color), (0, 0, 0));
        assert!(!st.sparkle_on);
        crate::pl!(sys).set_freeze_bullets(sys.flags, false);
        st.step(&mut anm, sys);
        assert_eq!((st.ip, st.frame, st.clear_color), (1, 1, 7));
        assert!(st.sparkle_on);
    }

    #[test]
    fn sparkle_frames() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let mut st = synthetic(vec![rec(-1, 0, [0; 3])]);
        let due = Stage::sparkle_due;
        st.frame = 0;
        assert!(!due(&st, *sys.flags));
        st.frame = 702;
        assert!(due(&st, *sys.flags));
        st.frame = 703;
        assert!(!due(&st, *sys.flags));
        *sys.flags = crate::gamemgr::F_SPELL_PRACTICE;
        st.frame = 0;
        assert!(due(&st, *sys.flags));
        st.frame = 1;
        assert!(!due(&st, *sys.flags));
        st.spell_state = 2;
        st.frame = 0;
        assert!(!due(&st, *sys.flags));
    }

    // 06 §10.2: layers / list B / fade rect hidden while +0xB24 ≥ 2 or the HUD cover is on.
    #[test]
    fn draw_gates() {
        let mut st = synthetic(vec![rec(-1, 0, [0; 3])]);
        assert!(!st.layers_hidden());
        st.spell_state = 1;
        assert!(!st.layers_hidden());
        st.hud_cover = true;
        assert!(st.layers_hidden());
        st.hud_cover = false;
        st.spell_state = 2;
        assert!(st.layers_hidden());
    }

    #[test]
    fn sparkle_point_v_quirk() {
        let fog = FogState {
            near: 200.0,
            far: 500.0,
            color: 0,
        };
        assert!(sparkle_point_ok(100.0, fog));
        assert!(!sparkle_point_ok(200.0, fog));
        // Past near v is the fog factor (< 1), so the test passes.
        assert!(sparkle_point_ok(400.0, fog));
        let tight = FogState { near: 0.5, ..fog };
        assert!(!sparkle_point_ok(400.0, tight));
    }

    #[test]
    fn stage_args() {
        assert_eq!(parse_stage_arg("4b"), Some(4));
        assert_eq!(parse_stage_arg("6B"), Some(7));
        assert_eq!(parse_stage_arg("stage7.std"), Some(7));
        assert_eq!(parse_stage_arg("stage7_s.std"), None);
        assert_eq!(parse_stage_arg("ex"), Some(8));
        assert_eq!(
            files(4, false),
            ("stage4b.std".into(), "stg4abg.anm".into())
        );
        assert_eq!(files(7, true), ("stage7_s.std".into(), "stg7bg.anm".into()));
    }

    #[test]
    fn all_std_scripts_run() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let Some(ar) = crate::archive::test_archive() else {
            return;
        };
        for (index, sp) in (0..9).flat_map(|i| [(i, false), (i, true)]) {
            let (std_name, bg) = files(index, sp);
            let std = stdfile::parse(&ar.read(&std_name).unwrap()).unwrap();
            let mut anm = AnmManager::default();
            anm.insert(
                ANM_SLOT,
                &bg,
                crate::anm::parse(&ar.read(&bg).unwrap()).unwrap(),
                |_| crate::drawlist::TexId(0),
            );
            let mut st = Stage::new(index, std);
            st.start(&mut anm, sys.rng);
            for f in 0..6000 {
                if f == 3000 {
                    st.section_request = 1;
                }
                st.step(&mut anm, sys);
                let cam = st.cam.camera(PLAYFIELD);
                assert!(
                    cam.view.0.iter().flatten().all(|v| v.is_finite()),
                    "{std_name} frame {f}: camera {:?}",
                    st.cam.v
                );
            }
            assert!(st.ip > 0, "{std_name}: script never advanced");
        }
    }
}
