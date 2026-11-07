//! Touch drag harness: synthetic finger traces through the real `TouchSource` and stage 1, finger path vs player path.

#![expect(
    clippy::disallowed_methods,
    reason = "trace geometry and scores of a test, not game logic"
)]

use super::*;
use crate::platform::touch::{Ctl, Movement};
use th08_core::player::CLAMP;
use winit::event::TouchPhase;

// Landscape 20:9 phone: 2400×1080 px at 2.625 px per dp; the picture is 1440×1080 (scale 2.25).
const WIN: (u32, u32, f64) = (2400, 1080, 2.625);
const MM: f64 = 160.0 / 25.4 * WIN.2;
const START: [f32; 2] = [192.0, 300.0];
const FINGER0: [f64; 2] = [500.0, 540.0];

// One finger stroke: down at `t0`, up at `t1` (s from the case start), position (mm from FINGER0) at time t.
struct Stroke {
    t0: f64,
    t1: f64,
    at: Box<dyn Fn(f64) -> [f64; 2]>,
}

struct Case {
    name: String,
    strokes: Vec<Stroke>,
    // Hold the FOCUS button for the whole case.
    focus: bool,
}

#[derive(Clone, Copy)]
struct Feed {
    // MotionEvent rate (Hz) and the timing jitter as a fraction of its interval.
    rate: f64,
    jitter: f64,
    // Draws per second (logic frames run in batches at each draw; events arrive before the draw).
    draws: f64,
    sens: f32,
}

#[derive(Default, Clone, Copy)]
struct Score {
    // Final error (game px), RMS path error, lag (ms), movement lost along the finger's direction (game px),
    // direction error of the whole displacement (°), frames stepping against the finger on an axis.
    end: f64,
    rms: f64,
    lag: f64,
    lost: f64,
    dir: f64,
    wrong: u32,
}

fn line(t0: f64, dur: f64, deg: f64, mm: f64) -> Stroke {
    let (s, c) = deg.to_radians().sin_cos();
    Stroke {
        t0,
        t1: t0 + dur,
        at: Box::new(move |t| {
            let u = ((t - t0) / dur).clamp(0.0, 1.0);
            // Eased like a real swipe: slow start and end.
            let e = u * u * (3.0 - 2.0 * u);
            [c * mm * e, -s * mm * e]
        }),
    }
}

fn cases() -> Vec<Case> {
    let mut v = Vec::new();
    let one = |name: String, s: Stroke| Case {
        name,
        strokes: vec![s],
        focus: false,
    };
    for deg in [
        0.0, 10.0, 20.0, 30.0, 45.0, 60.0, 80.0, 100.0, 165.0, 200.0, 250.0, 290.0, 340.0,
    ] {
        v.push(one(format!("slow {deg:>3}°"), line(0.0, 1.5, deg, 18.0)));
        v.push(one(format!("fast {deg:>3}°"), line(0.0, 0.2, deg, 18.0)));
    }
    for deg in [15.0, 200.0] {
        v.push(one(format!("crawl {deg:>3}°"), line(0.0, 3.0, deg, 3.0)));
        v.push(Case {
            name: format!("focus {deg:>3}°"),
            strokes: vec![line(0.0, 1.0, deg, 10.0)],
            focus: true,
        });
    }
    v.push(one(
        "circle".into(),
        Stroke {
            t0: 0.0,
            t1: 2.0,
            at: Box::new(|t| {
                let a = t * std::f64::consts::TAU / 1.0;
                [8.0 * a.cos() - 8.0, 8.0 * a.sin()]
            }),
        },
    ));
    // Stop-and-go: four short moves with pauses, finger held.
    v.push(one(
        "stop-go".into(),
        Stroke {
            t0: 0.0,
            t1: 2.4,
            at: Box::new(|t| {
                let seg = (t / 0.6).floor().min(3.0);
                let u = ((t - seg * 0.6) / 0.25).clamp(0.0, 1.0);
                let dirs = [[4.0, 0.0], [0.0, -3.0], [-2.0, 1.0], [3.0, 3.0]];
                let mut p = [0.0; 2];
                for (i, d) in dirs.iter().enumerate() {
                    let k = if (i as f64) < seg {
                        1.0
                    } else if i as f64 == seg {
                        u
                    } else {
                        0.0
                    };
                    p = [p[0] + d[0] * k, p[1] + d[1] * k];
                }
                p
            }),
        },
    ));
    // Lift and re-place: two strokes to the right, the finger goes back while lifted.
    v.push(Case {
        name: "relift".into(),
        strokes: vec![line(0.0, 0.5, 0.0, 8.0), line(0.7, 0.5, 0.0, 8.0)],
        focus: false,
    });
    // Into the left wall and back out: the player leaves the wall at once.
    v.push(one(
        "wall".into(),
        Stroke {
            t0: 0.0,
            t1: 2.5,
            at: Box::new(|t| {
                let out = if t < 1.2 {
                    -60.0 * (t / 1.2)
                } else {
                    -60.0 + 10.0 * ((t - 1.2) / 0.6).min(1.0)
                };
                [out, 0.0]
            }),
        },
    ));
    v
}

// A stage 1 `Running` (invulnerable) with the touch overlay on, drag movement at `sens`, in the window `WIN`.
fn stage(sens: f32) -> Option<Running> {
    let dir = crate::settings::test_game_dir()?;
    let src = crate::sources::native(dir);
    let mut s = crate::settings::get().clone();
    for (k, v) in [
        ("stage", "1"),
        ("invuln", "on"),
        ("touch", "on"),
        ("touch_move", "drag"),
    ] {
        s.set_from(crate::settings::Origin::Cli, k, v).unwrap();
    }
    s.touch.sens = sens;
    s.touch.autofire = false;
    s.save_dir = None;
    let mut io = s.tool_io(th08_core::drawlist::NullRenderer, &src);
    io.host = s.app_host();
    let ctx = pollster::block_on(Ctx::load_with(src.clone(), io, s.core(), true)).unwrap();
    let mut r = Running {
        ctx,
        pacer: Pacer::new(0),
        vsync_epoch: Instant::now(),
        settings: s,
        hidden_overlay: None,
        replay: ReplayTransport::default(),
        toast: None,
        snapshot: None,
        wake: None,
        held_dirty: false,
        slow: 0,
        started: None,
        titles: Vec::new(),
        now_playing: None,
        discord: crate::discord::Rpc::off(),
        stats: None,
        volume: None,
        quit: None,
    };
    touch(&mut r).set_window(WIN.0, WIN.1, WIN.2, th08_render::gfx::SafeArea::default());
    for _ in 0..400 {
        frame(&mut r);
    }
    assert_eq!(r.ctx.w.game.scene, th08_core::types::SceneId::Game);
    Some(r)
}

fn touch(r: &mut Running) -> &mut crate::platform::TouchSource {
    &mut r
        .ctx
        .host_as::<crate::platform::AppHost>()
        .unwrap()
        .input
        .touch
}

fn frame(r: &mut Running) {
    assert!(logic_frames(r, 1, false, true).is_none());
}

fn pos(r: &Running) -> [f32; 2] {
    let p = r.ctx.w.player.as_deref().unwrap();
    [p.pos[0], p.pos[1]]
}

// Deterministic jitter in [-1, 1).
fn noise(seed: &mut u64) -> f64 {
    *seed = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    (*seed >> 11) as f64 / (1u64 << 52) as f64 - 1.0
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "window px, frame counts"
)]
fn run_case(r: &mut Running, c: &Case, feed: Feed, seed: &mut u64) -> Score {
    if let Some(p) = r.ctx.w.player.as_deref_mut() {
        p.pos[0] = START[0];
        p.pos[1] = START[1];
    }
    for _ in 0..3 {
        frame(r);
    }
    let p0 = pos(r);
    // Game px per window px: the picture's letterbox scale.
    let k = f64::from(feed.sens) / (f64::from(WIN.1) / 480.0).min(f64::from(WIN.0) / 640.0);
    // Events (time, phase, window px).
    let mut ev: Vec<(f64, TouchPhase, [f64; 2], [f64; 2])> = Vec::new();
    for s in &c.strokes {
        let px = |t: f64| {
            let m = (s.at)(t);
            [FINGER0[0] + m[0] * MM, FINGER0[1] + m[1] * MM]
        };
        ev.push((s.t0, TouchPhase::Started, px(s.t0), px(s.t0)));
        let dt = 1.0 / feed.rate;
        let mut t = s.t0;
        loop {
            t += dt * (1.0 + feed.jitter * noise(seed));
            if t >= s.t1 {
                break;
            }
            let p = px(t);
            let n = [p[0] + 0.5 * noise(seed), p[1] + 0.5 * noise(seed)];
            ev.push((t, TouchPhase::Moved, n, p));
        }
        ev.push((s.t1, TouchPhase::Moved, px(s.t1), px(s.t1)));
        ev.push((s.t1, TouchPhase::Ended, px(s.t1), px(s.t1)));
    }
    let end = c.strokes.iter().map(|s| s.t1).fold(0.0, f64::max) + 1.6;
    let frames = (end * 60.0).ceil() as usize;
    if c.focus {
        let f = touch(r)
            .placed()
            .into_iter()
            .find(|p| p.ctl == Ctl::Focus)
            .unwrap()
            .centre;
        touch(r).touch(9, TouchPhase::Started, f[0], f[1]);
    }
    // Expected player: the finger's clean movement × k, re-anchored at the walls.
    let mut want = [f64::from(p0[0]), f64::from(p0[1])];
    let mut last: Option<[f64; 2]> = None;
    let (mut players, mut wants) = (Vec::new(), Vec::new());
    let mut next = 0;
    let mut i = 0;
    let mut draw = 0.0;
    let mut wrong = 0;
    while i < frames {
        draw += 1.0 / feed.draws;
        while next < ev.len() && ev[next].0 <= draw {
            let (_, ph, p, clean) = ev[next];
            touch(r).touch(1, ph, p[0] as f32, p[1] as f32);
            match ph {
                TouchPhase::Started => last = Some(clean),
                TouchPhase::Moved => {
                    if let Some(l) = last {
                        want[0] = (want[0] + (clean[0] - l[0]) * k)
                            .clamp(f64::from(CLAMP[0]), f64::from(CLAMP[2]));
                        want[1] = (want[1] + (clean[1] - l[1]) * k)
                            .clamp(f64::from(CLAMP[1]), f64::from(CLAMP[3]));
                    }
                    last = Some(clean);
                }
                _ => last = None,
            }
            next += 1;
        }
        while i < frames && (i as f64) / 60.0 < draw {
            let before = pos(r);
            frame(r);
            let after = pos(r);
            for a in 0..2 {
                let d = f64::from(after[a] - before[a]);
                let o = want[a] - f64::from(before[a]);
                if d * o < 0.0 && o.abs() > 1.0 {
                    wrong += 1;
                }
            }
            players.push([f64::from(after[0]), f64::from(after[1])]);
            wants.push(want);
            i += 1;
        }
    }
    if c.focus {
        let f = touch(r)
            .placed()
            .into_iter()
            .find(|p| p.ctl == Ctl::Focus)
            .unwrap()
            .centre;
        touch(r).touch(9, TouchPhase::Ended, f[0], f[1]);
    }
    let dist = |a: [f64; 2], b: [f64; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    let rms_at = |lag: usize| {
        let n = players.len() - lag;
        let s: f64 = (0..n)
            .map(|j| dist(players[j + lag], wants[j]).powi(2))
            .sum();
        (s / n as f64).sqrt()
    };
    let lag = (0..30)
        .min_by(|&a, &b| rms_at(a).total_cmp(&rms_at(b)))
        .unwrap();
    let pe = *players.last().unwrap();
    let we = *wants.last().unwrap();
    let s0 = [f64::from(p0[0]), f64::from(p0[1])];
    let wd = [we[0] - s0[0], we[1] - s0[1]];
    let pd = [pe[0] - s0[0], pe[1] - s0[1]];
    let wl = (wd[0] * wd[0] + wd[1] * wd[1]).sqrt();
    let (lost, dir) = if wl > 4.0 {
        let along = (pd[0] * wd[0] + pd[1] * wd[1]) / wl;
        let ang = (pd[1].atan2(pd[0]) - wd[1].atan2(wd[0])).to_degrees();
        let ang = (ang + 540.0).rem_euclid(360.0) - 180.0;
        (wl - along, ang.abs())
    } else {
        (0.0, 0.0)
    };
    Score {
        end: dist(pe, we),
        rms: rms_at(0),
        lag: lag as f64 * 1000.0 / 60.0,
        lost,
        dir,
        wrong,
    }
}

// Every case under several feeds; prints a table and returns the worst / mean scores.
fn report(feeds: &[(&str, Feed)]) -> Option<Vec<(String, Score, Score)>> {
    let mut out = Vec::new();
    for &(label, feed) in feeds {
        let mut r = stage(feed.sens)?;
        assert_eq!(touch(&mut r).cfg.movement, Movement::Drag);
        let mut seed = 0x5eed;
        let mut worst = Score::default();
        let mut sum = Score::default();
        let all = cases();
        println!(
            "== {label}: {} Hz ±{:.0} %, {} draws/s, sens {}",
            feed.rate,
            feed.jitter * 100.0,
            feed.draws,
            feed.sens
        );
        println!(
            "{:<12} {:>7} {:>7} {:>6} {:>7} {:>6} {:>5}",
            "case", "end px", "rms px", "lag ms", "lost px", "dir °", "wrong"
        );
        for c in &all {
            let s = run_case(&mut r, c, feed, &mut seed);
            println!(
                "{:<12} {:>7.2} {:>7.2} {:>6.0} {:>7.2} {:>6.1} {:>5}",
                c.name, s.end, s.rms, s.lag, s.lost, s.dir, s.wrong
            );
            worst = Score {
                end: worst.end.max(s.end),
                rms: worst.rms.max(s.rms),
                lag: worst.lag.max(s.lag),
                lost: worst.lost.max(s.lost),
                dir: worst.dir.max(s.dir),
                wrong: worst.wrong.max(s.wrong),
            };
            sum = Score {
                end: sum.end + s.end,
                rms: sum.rms + s.rms,
                lag: sum.lag + s.lag,
                lost: sum.lost + s.lost,
                dir: sum.dir + s.dir,
                wrong: sum.wrong + s.wrong,
            };
        }
        let n = all.len() as f64;
        let mean = Score {
            end: sum.end / n,
            rms: sum.rms / n,
            lag: sum.lag / n,
            lost: sum.lost / n,
            dir: sum.dir / n,
            wrong: sum.wrong,
        };
        println!(
            "{label} mean: end {:.2} rms {:.2} lag {:.0} lost {:.2} dir {:.1} wrong(total) {}",
            mean.end, mean.rms, mean.lag, mean.lost, mean.dir, mean.wrong
        );
        println!(
            "{label} worst: end {:.2} rms {:.2} lag {:.0} lost {:.2} dir {:.1} wrong {}",
            worst.end, worst.rms, worst.lag, worst.lost, worst.dir, worst.wrong
        );
        out.push((label.to_owned(), mean, worst));
    }
    Some(out)
}

// Finger traces at 60–240 Hz with jitter, 60 / 45 / 30 draws per second, sensitivity 1 and 2, through stage 1:
// the player ends where the finger says (within a step), never steps against it, loses no slow drag.
#[test]
fn drag_follows_finger_in_stage() {
    let feeds = [
        (
            "60hz",
            Feed {
                rate: 60.0,
                jitter: 0.2,
                draws: 60.0,
                sens: 1.0,
            },
        ),
        (
            "120hz",
            Feed {
                rate: 120.0,
                jitter: 0.3,
                draws: 60.0,
                sens: 1.0,
            },
        ),
        (
            "240hz",
            Feed {
                rate: 240.0,
                jitter: 0.3,
                draws: 60.0,
                sens: 1.0,
            },
        ),
        (
            "90hz-45fps",
            Feed {
                rate: 90.0,
                jitter: 0.3,
                draws: 45.0,
                sens: 1.0,
            },
        ),
        (
            "120hz-30fps",
            Feed {
                rate: 120.0,
                jitter: 0.3,
                draws: 30.0,
                sens: 1.0,
            },
        ),
        (
            "120hz-sens2",
            Feed {
                rate: 120.0,
                jitter: 0.3,
                draws: 60.0,
                sens: 2.0,
            },
        ),
    ];
    let Some(out) = report(&feeds) else {
        eprintln!("skipped: no game data (TH08_DIR unset)");
        return;
    };
    // Within about half a diagonal step of the finger, nothing lost, never against it.
    for (label, _, worst) in out {
        assert!(
            worst.end <= 3.0 && worst.lost <= 3.0,
            "{label}: end {:.2} lost {:.2}",
            worst.end,
            worst.lost
        );
        assert!(
            worst.dir <= 5.0,
            "{label}: direction off by {:.1}°",
            worst.dir
        );
        assert_eq!(worst.wrong, 0, "{label}: steps against the finger");
    }
}
