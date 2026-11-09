//! Injected input per modality on the Slint settings page: every row reachable, a drag changes nothing, a tap activates.
#![cfg(not(target_arch = "wasm32"))]

use th08::platform::portmenu::Do;
use th08::platform::portmenu::{Nav, Shot};
use th08::settings::{Origin, PlatformDirs, Settings};
use th08::slintui::check::{CardHarness, Harness, R, Reach};
use th08::slintui::menu::View;
use th08_render::gfx::{Gfx, SafeArea};

const PHONE: (u32, u32) = (2400, 1080);

fn settings(dir: &std::path::Path) -> Settings {
    let dirs = PlatformDirs {
        config: Some(dir.to_path_buf()),
        save: Some(dir.to_path_buf()),
        soundfonts: Vec::new(),
    };
    let mut s = Settings::from_args(dirs, Vec::<String>::new()).unwrap_or_else(|e| e.exit());
    s.set_from(Origin::Cli, "language", "en").expect("language");
    s
}

fn phone_safe() -> SafeArea {
    SafeArea {
        left: 110,
        top: 0,
        right: 48,
        bottom: 0,
    }
}

fn shot(tab: usize, touch: bool) -> Shot {
    Shot {
        settings: Some(tab),
        touch,
        density: 2.75,
        ..Shot::default()
    }
}

fn values(h: &Harness<'_>) -> Vec<Option<String>> {
    h.model
        .live_keys()
        .iter()
        .map(|k| h.model.value_text(k))
        .collect()
}

#[test]
fn every_modality() {
    let Ok(gfx) = pollster::block_on(Gfx::headless()) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let dir = std::env::temp_dir().join(format!("th08-slint-input-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let s = settings(&dir);
    let tabs = th08::platform::portmenu::settings_tabs(&s, true);
    assert!(tabs >= 4, "{tabs} tabs");

    for tab in 0..tabs {
        for how in [Reach::Touch, Reach::Wheel, Reach::Keys] {
            let mut h =
                Harness::new(&gfx, &s, PHONE, phone_safe(), &shot(tab, true)).expect("page");
            let all = h.row_names();
            assert!(all.len() >= 2, "tab {tab}: {all:?}");
            let before = values(&h);
            let seen = h.reach(how);
            for n in &all {
                assert!(seen.contains(n), "tab {tab} {how:?}: {n} not reached");
            }
            assert_eq!(values(&h), before, "tab {tab} {how:?} changed a value");
            let issues = h.issues();
            let two: Vec<_> = issues
                .iter()
                .filter(|i| i.kind == th08::uicheck::Kind::TwoFocus)
                .collect();
            assert!(two.is_empty(), "tab {tab} {how:?}: {two:?}");
        }
    }

    drag_changes_nothing(&gfx, &s);
    tap_activates(&gfx, &s);
    keys_and_reset(&gfx, &s);
    card_every_input(&gfx, &s);
    let _ = std::fs::remove_dir_all(dir);
}

fn in_row(row: R, t: &[(String, R)]) -> Vec<(String, R)> {
    t.iter()
        .filter(|(_, r)| r.y >= row.y - 1.0 && r.y + r.h <= row.y + row.h + 1.0)
        .cloned()
        .collect()
}

// From the label, both stepper cells and the track.
fn drag_changes_nothing(gfx: &Gfx, s: &Settings) {
    let mut h = Harness::new(gfx, s, PHONE, phone_safe(), &shot(0, true)).expect("page");
    let before = values(&h);
    let row = h.row_rect("Master volume").expect("first row");
    let cells = in_row(row, &h.targets("target|step"));
    assert_eq!(cells.len(), 2, "{cells:?}");
    let track = in_row(row, &h.targets("target|track"))[0].1;
    let list = h.list_rect();
    let starts = [
        ("label", list.x + 90.0),
        ("left step", cells[0].1.x + cells[0].1.w / 2.0),
        ("right step", cells[1].1.x + cells[1].1.w / 2.0),
        ("track", track.x + track.w / 2.0),
    ];
    let y = row.y + row.h / 2.0;
    for (name, x) in starts {
        h.drag(x, y + row.h, y - row.h * 0.6);
        h.drag(x, y - row.h * 0.6, y + row.h);
        h.sloppy_drag(x, y + row.h, y - row.h * 0.6, 0, 4.0);
        h.sloppy_drag(x, y - row.h * 0.6, y + row.h, 160, 4.0);
        assert_eq!(values(&h), before, "drag from {name} changed a value");
    }
}

fn tap_activates(gfx: &Gfx, s: &Settings) {
    let mut h = Harness::new(gfx, s, PHONE, phone_safe(), &shot(0, true)).expect("page");
    let row = h.row_rect("Master volume").expect("first row");
    let left = in_row(row, &h.targets("target|step"))[0].1;
    let before = values(&h);
    h.tap(left.x + left.w / 2.0, left.y + left.h / 2.0);
    assert_ne!(
        values(&h),
        before,
        "a tap on the left cell changes the value"
    );

    let track = in_row(row, &h.targets("target|track"))[0].1;
    let before = values(&h);
    let y = track.y + track.h / 2.0;
    h.drag_xy((track.x + track.w * 0.8, y), (track.x + track.w * 0.2, y));
    assert_ne!(values(&h), before, "a sideways drag on the track moves it");

    let choices = h.targets("target|choice|");
    let (name, c) = choices
        .iter()
        .find(|(n, _)| n.ends_with("|On") || n.ends_with("|Off"))
        .expect("a toggle row")
        .clone();
    let other = choices
        .iter()
        .find(|(n, r)| *n != name && (r.y - c.y).abs() < 1.0)
        .expect("the other word")
        .1;
    let before = values(&h);
    h.tap(c.x + c.w / 2.0, c.y + c.h / 2.0);
    h.tap(other.x + other.w / 2.0, other.y + other.h / 2.0);
    let after = values(&h);
    h.tap(c.x + c.w / 2.0, c.y + c.h / 2.0);
    assert!(
        after != before || values(&h) != after,
        "a tap on a word picks it"
    );
}

fn keys_and_reset(gfx: &Gfx, s: &Settings) {
    let mut h = Harness::new(gfx, s, PHONE, phone_safe(), &shot(0, true)).expect("page");
    let cursor = h.menu.page.api().get_cursor();
    h.nav(Nav::Down);
    assert_eq!(
        h.menu.page.api().get_cursor(),
        cursor,
        "the first key only shows the bar"
    );
    h.nav(Nav::Down);
    assert_eq!(h.menu.page.api().get_cursor(), cursor + 1);
    let before = values(&h);
    h.nav(Nav::Left);
    let changed = values(&h);
    assert_ne!(changed, before, "left changes the focused row");
    for _ in 0..30 {
        h.nav(Nav::Down);
    }
    assert!(!h.menu.page.api().get_armed());
    h.nav(Nav::Activate);
    assert!(h.menu.page.api().get_armed(), "first press asks");
    assert_eq!(values(&h), changed, "the question changes nothing");
    h.nav(Nav::Up);
    assert!(!h.menu.page.api().get_armed(), "moving away cancels");
    h.nav(Nav::Down);
    h.nav(Nav::Activate);
    h.nav(Nav::Activate);
    assert_ne!(values(&h), changed, "the second press resets");
    h.nav(Nav::NextTab);
    assert_eq!(h.menu.page.tab(), 1);
    assert!(h.menu.page.api().get_scroll().abs() < 0.5);

    let kept = h.row_and_tabs();
    h.reshow_tab();
    assert!(
        h.row_and_tabs() == kept,
        "unchanged rows and tabs are not rebuilt"
    );

    // The window is kept: leaving the page and coming back gives a fresh page that takes keys.
    assert!(h.menu.enter(View::Card));
    h.tick(16);
    assert!(h.menu.enter(View::Settings));
    assert!(!h.menu.enter(View::Settings));
    h.menu.page.open(true);
    h.tick(16);
    h.nav(Nav::Down);
    h.nav(Nav::Down);
    assert_eq!(
        h.menu.page.api().get_cursor(),
        1,
        "keys reach the page again"
    );
}

// Name, window size, safe area, touch, density.
type CardShape = (&'static str, (u32, u32), SafeArea, bool, f32);

// Phone side bars, 16:9 pillar, 4:3 column.
fn card_shapes() -> [CardShape; 3] {
    [
        ("phone", PHONE, phone_safe(), true, 2.75),
        ("pillar", (1920, 1080), SafeArea::default(), false, 1.0),
        ("column", (1280, 960), SafeArea::default(), false, 1.0),
    ]
}

#[derive(Clone, Copy, PartialEq)]
enum Press {
    Touch,
    Mouse,
    Keys,
}

fn press(h: &mut CardHarness<'_>, how: Press, r: R) {
    match how {
        Press::Touch => h.tap(r),
        Press::Mouse => h.click(r),
        Press::Keys => h.nav(Nav::Activate),
    }
}

// Every item does its job by touch, pointer and keys; confirms default to the safe answer and only the action runs it.
fn card_every_input(gfx: &Gfx, s: &Settings) {
    for (shape, size, safe, phone, density) in card_shapes() {
        let shot = Shot {
            quit: true,
            touch: phone,
            density,
            speed: Some((1, 1)),
            ..Shot::default()
        };
        let names = CardHarness::new(gfx, s, size, safe, &shot)
            .expect("card")
            .rig
            .item_names();
        for how in [Press::Touch, Press::Mouse, Press::Keys] {
            for (i, name) in names.iter().enumerate() {
                let mut h = CardHarness::new(gfx, s, size, safe, &shot).expect("card");
                if how == Press::Keys {
                    h.nav(Nav::Up);
                    for _ in 0..i {
                        h.nav(Nav::Down);
                    }
                    assert_eq!(h.rig.focused_name(), *name, "{shape}: keys reach {name}");
                }
                let r = label_of(h.target(&format!("item|{i}")));
                press(&mut h, how, r);
                let done = h.take_done();
                let ctx = format!("{shape} {name}: {done:?}");
                match name.as_str() {
                    "Resume" => assert_eq!(done, [Do::Resume], "{ctx}"),
                    "Screenshot" => assert_eq!(done, [Do::Screenshot], "{ctx}"),
                    "Settings" => assert!(h.rig.on_settings_page(), "{ctx}"),
                    "TouchLayout" => assert_eq!(done, [Do::EditLayout], "{ctx}"),
                    "Hitboxes" => assert!(matches!(done[..], [Do::Hitboxes(_)]), "{ctx}"),
                    "Volume" | "Speed" => assert!(done.is_empty(), "{ctx}"),
                    "Retry" | "Quit" => confirm(&mut h, how, name, &ctx),
                    other => panic!("{other}"),
                }
            }
        }
        no_run_items(gfx, s, size, safe, &shot, shape);
        let mut h = CardHarness::new(gfx, s, size, safe, &shot).expect("card");
        let volume = names.iter().position(|n| n == "Volume").expect("volume");
        let before = h.rig.volume();
        h.nav(Nav::Up);
        for _ in 0..volume {
            h.nav(Nav::Down);
        }
        h.nav(Nav::Left);
        assert_ne!(h.rig.volume(), before, "{shape}: left changes the volume");
        if phone {
            let v = h.rig.volume();
            let step = h.target("step");
            h.tap(step);
            assert_ne!(h.rig.volume(), v, "{shape}: the cell steps the volume");
        }
    }
}

// The label end of an item's band, clear of its value control.
fn label_of(r: R) -> R {
    R {
        x: r.x,
        y: r.y + 2.0,
        w: r.w * 0.25,
        h: 6.0,
    }
}

fn confirm(h: &mut CardHarness<'_>, how: Press, name: &str, ctx: &str) {
    assert!(h.rig.asking(), "{ctx}: asks first");
    assert!(
        h.rig.on_safe_answer(),
        "{ctx}: the safe answer is the default"
    );
    let want = if name == "Retry" { Do::Retry } else { Do::Quit };
    if how == Press::Keys {
        h.nav(Nav::Activate);
        assert!(
            !h.rig.asking() && h.take_done().is_empty(),
            "{ctx}: Z on the default keeps playing"
        );
        h.nav(Nav::Activate);
        assert!(h.rig.asking(), "{ctx}: asks again");
        h.nav(Nav::Back);
        assert!(
            !h.rig.asking() && h.take_done().is_empty(),
            "{ctx}: back cancels"
        );
        h.nav(Nav::Activate);
        h.nav(Nav::Down);
        h.nav(Nav::Activate);
        assert_eq!(h.take_done(), [want], "{ctx}: the action runs");
        return;
    }
    let keep = h.target("answer|keep");
    press(h, how, keep);
    assert!(
        !h.rig.asking() && h.take_done().is_empty(),
        "{ctx}: keep changes nothing"
    );
    let names = h.rig.item_names();
    let at = names.iter().position(|n| n == name).expect("item");
    let r = label_of(h.target(&format!("item|{at}")));
    press(h, how, r);
    let yes = h.target("answer|act");
    press(h, how, yes);
    assert_eq!(h.take_done(), [want], "{ctx}: the action runs");
}

// Replays and demos: no Retry or Quit on the card.
fn no_run_items(
    gfx: &Gfx,
    s: &Settings,
    size: (u32, u32),
    safe: SafeArea,
    shot: &Shot,
    shape: &str,
) {
    let shot = Shot {
        quit: false,
        ..shot.clone()
    };
    let h = CardHarness::new(gfx, s, size, safe, &shot).expect("card");
    let names = h.rig.item_names();
    assert!(
        !names.iter().any(|n| n == "Retry" || n == "Quit"),
        "{shape}: {names:?}"
    );
    assert_eq!(h.targets("target|item|").len(), names.len(), "{shape}");
}
