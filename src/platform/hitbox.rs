//! Hitbox viewer: the collision shapes of the player, bullets, lasers and enemies, drawn after the blit.

use super::overlay::Pass;
use egui::{Color32, Pos2, Rect, Shape as EShape, Stroke, pos2};
use th08_core::bullet::{Bullet, Laser, LaserFlags};
use th08_core::drawlist::PLAYFIELD;
use th08_core::enemy::{Enemy, EnemyFlags};
use th08_core::game::Ctx;
use th08_core::player::Player;
use th08_core::world::World;
use th08_render::gfx::{Gfx, Overlay, OverlayTarget};
use winit::keyboard::KeyCode;

// Desktop key that cycles the mode in the game (never bindable, `keymap.rs`).
pub const KEY: KeyCode = KeyCode::F3;

// The `hitboxes` setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hitboxes {
    #[default]
    Off,
    Player,
    All,
}

impl Hitboxes {
    pub const IDS: [&'static str; 3] = ["off", "player", "all"];

    pub fn id(self) -> &'static str {
        Self::IDS[self as usize]
    }

    pub fn parse(v: &str) -> Option<Self> {
        match v.trim() {
            "off" => Some(Self::Off),
            "player" => Some(Self::Player),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Player,
            Self::Player => Self::All,
            Self::All => Self::Off,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    PlayerHit,
    Graze,
    ItemBox,
    Bullet,
    Laser,
    EnemyHurt,
    EnemyShot,
    EnemyContact,
}

// A convex quad in playfield px, corners in order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub kind: Kind,
    pub pts: [[f32; 2]; 4],
}

fn rect(kind: Kind, [x0, y0, x1, y1]: [f32; 4]) -> Shape {
    Shape {
        kind,
        pts: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
    }
}

// Box `c` ∓ size / 2, the object box of the player tests (08 §4.1 step 6).
fn centred(kind: Kind, c: [f32; 3], s: [f32; 2]) -> Shape {
    let h = [s[0] / 2.0, s[1] / 2.0];
    rect(kind, [c[0] - h[0], c[1] - h[1], h[0] + c[0], h[1] + c[1]])
}

// The player's stored boxes (09 §3: written by `Player_Move`). The graze region is the graze box grown by the
// 20 px the bullet graze test adds to the bullet box (08 §4.1 step 6); the item box is 08 §8.4's collect box.
pub fn player_shapes(p: &Player, items: bool, out: &mut Vec<Shape>) {
    let g = p.graze_box;
    out.push(rect(
        Kind::Graze,
        [g[0] - 20.0, g[1] - 20.0, g[2] + 20.0, g[3] + 20.0],
    ));
    if items {
        out.push(rect(Kind::ItemBox, p.item_box));
    }
    out.push(rect(Kind::PlayerHit, p.hit_box));
}

// A state-1 tangible bullet's square hit box (08 §4.1 step 6); states 2–4 only test bomb areas (08 §4.2).
pub fn bullet_shape(b: &Bullet) -> Option<Shape> {
    (b.state == 1 && !b.intangible).then(|| centred(Kind::Bullet, b.pos, [b.hitbox; 2]))
}

// The laser rectangle of 08 §6.3 step 2 rotated by the laser angle about its origin, with the step 4 / 6 width
// quirk (size.x := current width / 2); None while the state does not test (before collision start, after the fade
// collision frames).
pub fn laser_shape(l: &Laser) -> Option<Shape> {
    if !l.alive {
        return None;
    }
    let t = l.timer.cur;
    let len = l.head - l.tail;
    let mut s = [if l.tail > 0.0 { len * 0.7 } else { len }, l.width / 2.0];
    let width_fade = !l.flags.contains(LaserFlags::ALPHA_FADE);
    match l.state {
        0 if t >= l.hit_start => {
            if width_fade {
                s[0] = l.cur_width / 2.0;
            }
        }
        1 => {}
        2 if t < l.fade_hit => {
            if width_fade && l.fade > 0 {
                s[0] = l.cur_width / 2.0;
            }
        }
        _ => return None,
    }
    let o = [l.origin[0], l.origin[1]];
    let cx = o[0] + l.tail + len / 2.0;
    let (sn, cs) = th08_core::fmath::sin_cos_f(l.angle);
    let at = |x: f32, y: f32| {
        let (dx, dy) = (x - o[0], y - o[1]);
        [o[0] + cs * dx - sn * dy, o[1] + sn * dx + cs * dy]
    };
    let (x0, x1) = (cx - s[0] / 2.0, cx + s[0] / 2.0);
    let (y0, y1) = (o[1] - s[1] / 2.0, o[1] + s[1] / 2.0);
    Some(Shape {
        kind: Kind::Laser,
        pts: [at(x0, y0), at(x1, y0), at(x1, y1), at(x0, y1)],
    })
}

// 07 §10.1 step 6: skipped for flags A bits 4 / 5 / 11 and for bit 31 while the player bombs; contact box =
// hitbox / 1.5 (bit 2); shot damage boxes = hitbox and the extra shot box when its x > 0 (bit 6, 09 §4.5).
pub fn enemy_shapes(e: &Enemy, bombing: bool, out: &mut Vec<Shape>) {
    let f = e.a_flags();
    if !e.alive()
        || f.intersects(EnemyFlags::HIDDEN | EnemyFlags::INVISIBLE | EnemyFlags::YOUKAI)
        || (f.contains(EnemyFlags::BOMB_IMMUNE) && bombing)
    {
        return;
    }
    let (pos, hb, sb) = (e.final_pos, e.hitbox, e.shotbox);
    if f.contains(EnemyFlags::SHOT_HIT) {
        out.push(centred(Kind::EnemyHurt, pos, [hb[0], hb[1]]));
        if sb[0] > 0.0 {
            out.push(centred(Kind::EnemyShot, pos, [sb[0], sb[1]]));
        }
    }
    if f.contains(EnemyFlags::CONTACT) {
        out.push(centred(Kind::EnemyContact, pos, [hb[0] / 1.5, hb[1] / 1.5]));
    }
}

// Every shape of `mode`, drawn in this order (player last, on top).
pub fn collect(w: &World, mode: Hitboxes, out: &mut Vec<Shape>) {
    out.clear();
    let Some(p) = w.player.as_deref() else {
        return;
    };
    if mode == Hitboxes::Off {
        return;
    }
    let all = mode == Hitboxes::All;
    if all {
        if let Some(m) = w.enemies.as_deref() {
            for e in &m.enemies {
                enemy_shapes(e, p.bombing, out);
            }
        }
        if let Some(b) = w.bullets.as_deref() {
            out.extend(b.lasers.iter().filter_map(laser_shape));
            out.extend(b.bullets.iter().filter_map(bullet_shape));
        }
    }
    player_shapes(p, all, out);
}

// Read-only overlay of the shapes `update` took from the World between frames.
pub struct HitboxOverlay {
    pub mode: Hitboxes,
    shapes: Vec<Shape>,
    pass: Pass,
}

impl Default for HitboxOverlay {
    fn default() -> Self {
        Self {
            mode: Hitboxes::default(),
            shapes: Vec::new(),
            pass: Pass::new(crate::i18n::Lang::En, None),
        }
    }
}

fn colours(k: Kind) -> (Color32, Color32) {
    let c = |r, g, b, a| Color32::from_rgba_unmultiplied(r, g, b, a);
    match k {
        Kind::PlayerHit => (c(255, 255, 255, 255), c(255, 40, 40, 255)),
        Kind::Graze => (Color32::TRANSPARENT, c(255, 255, 255, 110)),
        Kind::ItemBox => (Color32::TRANSPARENT, c(80, 255, 120, 110)),
        Kind::Bullet => (c(255, 40, 40, 70), c(255, 60, 60, 230)),
        Kind::Laser => (c(255, 60, 220, 70), c(255, 80, 230, 230)),
        Kind::EnemyHurt => (Color32::TRANSPARENT, c(60, 220, 255, 220)),
        Kind::EnemyShot => (Color32::TRANSPARENT, c(80, 120, 255, 220)),
        Kind::EnemyContact => (c(255, 220, 40, 50), c(255, 220, 40, 220)),
    }
}

fn draw(painter: &egui::Painter, shapes: &[Shape], frame: [f32; 4]) {
    let (sx, sy) = (frame[2] / 640.0, frame[3] / 480.0);
    let map = |p: [f32; 2]| -> Pos2 {
        pos2(
            frame[0] + (PLAYFIELD.x + p[0]) * sx,
            frame[1] + (PLAYFIELD.y + p[1]) * sy,
        )
    };
    let clip = Rect::from_min_max(map([0.0, 0.0]), map([PLAYFIELD.w, PLAYFIELD.h]));
    let p = painter.with_clip_rect(clip);
    for s in shapes {
        let (fill, line) = colours(s.kind);
        let pts: Vec<Pos2> = s.pts.iter().map(|&q| map(q)).collect();
        p.add(EShape::convex_polygon(pts, fill, Stroke::new(1.0, line)));
        if s.kind == Kind::PlayerHit {
            // A ring around the (1–3 px) hit box so it can be found; not a collision shape.
            let c = map([
                (s.pts[0][0] + s.pts[2][0]) / 2.0,
                (s.pts[0][1] + s.pts[2][1]) / 2.0,
            ]);
            p.circle_stroke(c, 6.0 * sx.max(1.0), Stroke::new(1.0, line));
        }
    }
}

impl Overlay for HitboxOverlay {
    fn paint(&mut self, t: OverlayTarget<'_>, enc: &mut wgpu::CommandEncoder) {
        if self.mode == Hitboxes::Off || self.shapes.is_empty() {
            return;
        }
        let (shapes, frame) = (&self.shapes, t.frame);
        self.pass
            .paint(t, enc, "hitboxes", |p| draw(p, shapes, frame));
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn device_rebuilt(&mut self) {
        self.pass.device_rebuilt();
    }
}

fn overlay(g: &mut Gfx) -> Option<&mut HitboxOverlay> {
    g.overlay_top.as_mut()?.as_any_mut().downcast_mut()
}

pub fn install(g: &mut Gfx, mode: Hitboxes) {
    g.overlay_top = Some(Box::new(HitboxOverlay {
        mode,
        ..HitboxOverlay::default()
    }));
}

pub fn mode(g: &mut Gfx) -> Hitboxes {
    overlay(g).map_or(Hitboxes::Off, |o| o.mode)
}

pub fn set_mode(g: &mut Gfx, mode: Hitboxes) {
    if let Some(o) = overlay(g) {
        o.mode = mode;
        o.shapes.clear();
    }
}

// The next mode (F3 / the port menu); the shapes follow at the next `update`.
pub fn cycle(g: &mut Gfx) -> Hitboxes {
    let m = mode(g).next();
    set_mode(g, m);
    log::info!("hitboxes: {}", m.id());
    m
}

// Takes the shapes from the World (between frames, before the draw); writes nothing to it.
pub fn update(ctx: &mut Ctx) {
    let w = &ctx.w;
    if let Some(o) = ctx
        .renderer
        .as_any_mut()
        .downcast_mut::<Gfx>()
        .and_then(overlay)
    {
        let mode = o.mode;
        collect(w, mode, &mut o.shapes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use th08_core::anmmgr::AnmManager;
    use th08_core::bullet::BulletManager;
    use th08_core::game::Sys;

    // Owned stand-ins of the `Sys` fields.
    #[derive(Default)]
    struct Parts {
        sound: th08_core::sound::Queue,
        fx: th08_core::effect::Effects,
        flags: u32,
        rank: th08_core::gamemgr::Rank,
        ov: th08_core::ascii::Overlays,
        panel: th08_core::hud::Panel,
        areas: th08_core::bomb::AreaQueue,
        timer_pause: i32,
        load_frames: i32,
        gui: th08_core::gui::Queue,
        rng: th08_core::rng::Rng,
    }

    // A point player (hit box size 0) at `pos`; `debug_invuln` stops Player_Die after the hit is counted.
    fn player(pos: [f32; 2]) -> Box<Player> {
        let mut p = Box::new(Player::default());
        p.sht.hitbox = 0.0;
        p.pos = [pos[0], pos[1], 0.0];
        p.debug_invuln = true;
        p.update_boxes();
        p
    }

    // One `BulletManager::step` with the point player at `at`: whether the collision code hit it.
    fn hit(b: &BulletManager, at: [f32; 2]) -> bool {
        let mut b = b.clone();
        let mut anm = AnmManager::default();
        let mut s = Parts::default();
        let mut sys = Sys {
            sound: &mut s.sound,
            fx: &mut s.fx,
            flags: &mut s.flags,
            rank: &mut s.rank,
            ov: &mut s.ov,
            panel: &mut s.panel,
            areas: &mut s.areas,
            timer_pause: &mut s.timer_pause,
            load_frames: &mut s.load_frames,
            gui: &mut s.gui,
            rng: &mut s.rng,
            player: None,
            bullets: None,
            stage: None,
            msg: None,
        };
        let mut p = player(at);
        sys.player = Some(&mut p);
        b.step(&mut anm, &mut sys);
        p.hits > 0
    }

    fn mgr() -> BulletManager {
        BulletManager::new(
            &mut AnmManager::default(),
            &mut th08_core::rng::Rng::default(),
        )
    }

    // Points just inside / outside each edge midpoint and corner of a convex quad, in playfield px.
    fn probes(s: &Shape, d: f32) -> Vec<([f32; 2], bool)> {
        let c = [
            s.pts.iter().map(|p| p[0]).sum::<f32>() / 4.0,
            s.pts.iter().map(|p| p[1]).sum::<f32>() / 4.0,
        ];
        let towards = |p: [f32; 2], k: f32| {
            let v = [p[0] - c[0], p[1] - c[1]];
            let n = (v[0] * v[0] + v[1] * v[1]).sqrt();
            [p[0] + v[0] / n * k, p[1] + v[1] / n * k]
        };
        let mut out = vec![(c, true)];
        for i in 0..4 {
            let (a, b) = (s.pts[i], s.pts[(i + 1) % 4]);
            let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
            for p in [a, m] {
                out.push((towards(p, -d), true));
                out.push((towards(p, d), false));
            }
        }
        out
    }

    fn check(b: &BulletManager, s: &Shape) {
        for (at, inside) in probes(s, 0.05) {
            assert_eq!(hit(b, at), inside, "{:?} at {at:?}", s.kind);
        }
    }

    // The drawn shape is exactly the region where the core's hit test hits a point player.
    #[test]
    fn bullets_match_hit_test() {
        for (pos, size) in [
            ([100.0, 200.0], 6.0),
            ([250.5, 80.25], 4.0),
            ([30.0, 400.0], 16.0),
        ] {
            let mut b = mgr();
            let bl = &mut b.bullets[5];
            bl.state = 1;
            bl.pos = [pos[0], pos[1], 0.1];
            bl.hitbox = size;
            bl.grazed = true;
            let s = bullet_shape(&b.bullets[5]).expect("shape");
            assert_eq!(s.pts[0], [pos[0] - size / 2.0, pos[1] - size / 2.0]);
            check(&b, &s);
        }
        let mut intangible = Bullet::default();
        intangible.state = 1;
        intangible.intangible = true;
        assert!(bullet_shape(&intangible).is_none());
    }

    #[test]
    fn lasers_match_hit_test() {
        let laser = |angle: f32, tail: f32, head: f32, width: f32| Laser {
            origin: [192.0, 120.0, 0.0],
            angle,
            tail,
            head,
            max_len: 1000.0,
            width,
            cur_width: width,
            duration: 100,
            state: 1,
            alive: true,
            ..Laser::default()
        };
        // Tail 0 (full length), tail > 0 (the 0.7 rule), several angles.
        for l in [
            laser(0.0, 0.0, 200.0, 16.0),
            laser(std::f32::consts::FRAC_PI_2, 0.0, 150.0, 24.0),
            laser(0.7, 40.0, 240.0, 20.0),
            laser(2.4, 10.0, 90.0, 32.0),
        ] {
            let mut b = mgr();
            b.lasers[3] = l.clone();
            let s = laser_shape(&l).expect("shape");
            check(&b, &s);
        }
        // Before the collision start nothing is tested, and nothing is drawn.
        let mut l = laser(0.0, 0.0, 200.0, 16.0);
        l.state = 0;
        l.startup = 60;
        l.hit_start = 30;
        l.timer.cur = 10;
        assert!(laser_shape(&l).is_none());
    }

    // Logic state a draw-only overlay must not change: the demo-trace columns plus every bullet, laser, enemy and
    // the player position, bit for bit.
    fn fingerprint(ctx: &Ctx) -> Vec<i64> {
        let w = &ctx.w;
        let mut v = th08_core::trace::demo_sample(ctx, 0).to_vec();
        let bits = |v: &mut Vec<i64>, f: &[f32]| v.extend(f.iter().map(|x| i64::from(x.to_bits())));
        if let Some(p) = w.player.as_deref() {
            bits(&mut v, &p.pos);
        }
        if let Some(b) = w.bullets.as_deref() {
            for x in b.bullets.iter().filter(|x| !x.free()) {
                bits(&mut v, &x.pos);
                v.push(i64::from(x.state));
            }
            for l in b.lasers.iter().filter(|l| l.alive) {
                bits(&mut v, &[l.head, l.tail, l.angle, l.cur_width]);
            }
        }
        if let Some(m) = w.enemies.as_deref() {
            for e in m.enemies.iter().filter(|e| e.alive()) {
                bits(&mut v, &e.final_pos);
                v.push(i64::from(e.life));
            }
        }
        v
    }

    fn stage_ctx() -> Option<Ctx> {
        let dir = crate::settings::test_game_dir()?;
        let gfx = pollster::block_on(Gfx::headless()).ok()?;
        let src = crate::sources::native(dir);
        let s = crate::settings::get();
        let mut core = s.core();
        core.stage = th08_core::stage::parse_stage_arg("1");
        core.stage_frame = 1700;
        core.invuln = true;
        core.seed = Some(1234);
        Some(
            pollster::block_on(Ctx::load_with(
                src.clone(),
                s.tool_io(gfx, &src),
                core,
                false,
            ))
            .expect("load"),
        )
    }

    fn capture(ctx: &mut Ctx, size: Option<(u32, u32)>) -> image::RgbaImage {
        let g = ctx
            .renderer
            .as_any_mut()
            .downcast_mut::<Gfx>()
            .expect("gfx");
        g.capture(&mut ctx.rec, size)
    }

    // Two identical stage runs, one with the viewer on (all): the World stays the same every frame, the 640×480
    // frame `capture_screen` reads stays the same, and only the scaled window picture shows the shapes.
    #[test]
    fn overlay_changes_neither_world_nor_frame() {
        let (Some(mut a), Some(mut b)) = (stage_ctx(), stage_ctx()) else {
            eprintln!("no TH08_DIR or GPU adapter; skipped");
            return;
        };
        let g = b.renderer.as_any_mut().downcast_mut::<Gfx>().expect("gfx");
        install(g, Hitboxes::All);
        let (mut shown, mut differs) = (0usize, false);
        for f in 0..360 {
            assert_eq!(a.calc(), b.calc());
            update(&mut b);
            let g = b.renderer.as_any_mut().downcast_mut::<Gfx>().expect("gfx");
            shown = shown.max(overlay(g).map_or(0, |o| o.shapes.len()));
            a.draw();
            b.draw();
            match f % 60 {
                0 => assert!(capture(&mut a, None) == capture(&mut b, None), "frame {f}"),
                30 => {
                    differs |=
                        capture(&mut a, Some((960, 720))) != capture(&mut b, Some((960, 720)));
                }
                _ => {
                    a.present();
                    b.present();
                }
            }
            for c in [&mut a, &mut b] {
                th08_core::ascii::apply_pending_captures(&mut c.w.anm, &mut c.rec);
            }
            assert_eq!(fingerprint(&a), fingerprint(&b), "frame {f}");
        }
        assert!(shown > 10, "few shapes ({shown}): not a busy stage moment");
        assert!(differs, "the scaled picture never showed the overlay");
    }

    #[test]
    fn modes_cycle_and_parse() {
        let mut m = Hitboxes::Off;
        for want in [Hitboxes::Player, Hitboxes::All, Hitboxes::Off] {
            m = m.next();
            assert_eq!(m, want);
            assert_eq!(Hitboxes::parse(m.id()), Some(m));
        }
        assert_eq!(Hitboxes::parse("x"), None);
    }
}
