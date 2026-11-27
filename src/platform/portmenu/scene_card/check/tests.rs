use super::*;
use crate::menu_model::{Nav, Page, VOLUME};
use winit::event::{ElementState, MouseButton};

fn rig(touch: bool, quit: bool) -> Rig {
    let s = Settings::default();
    let shot = Shot {
        touch,
        quit,
        density: if touch { 2.75 } else { 1.0 },
        speed: Some((1, 1)),
        ..Shot::default()
    };
    Rig::new(&s, (2400, 1080), SafeArea::default(), &shot)
}

#[test]
fn every_card_item_is_reachable_by_keys_pad_mouse_and_touch() {
    for pad in [false, true] {
        let mut r = rig(true, true);
        let (down, activate) = if pad {
            (Nav::of_button(13), Nav::of_button(0))
        } else {
            use winit::keyboard::KeyCode;
            (
                Nav::of_key(KeyCode::ArrowDown, false),
                Nav::of_key(KeyCode::Enter, false),
            )
        };
        r.model.apply(Input::Nav(down.unwrap()));
        assert!(r.model.bar());
        for it in r.model.items() {
            assert_eq!(r.model.item(), it);
            if it.danger() {
                r.model.apply(Input::Nav(activate.unwrap()));
                assert_eq!(
                    r.model.ask(),
                    Some(if it == Item::Quit {
                        Ask::Quit
                    } else {
                        Ask::Retry
                    })
                );
                assert!(!r.model.yes());
                assert!(r.model.apply(Input::Nav(activate.unwrap())).is_empty());
                assert!(!r.asking());
            }
            r.model.apply(Input::Nav(down.unwrap()));
        }
    }
    for touch_input in [false, true] {
        for it in rig(touch_input, true).model.items() {
            let mut r = rig(touch_input, true);
            let scene = r.scene();
            let mut at = center(&scene, tag(it, ROW));
            if scene
                .hit_at(at.x as f32 / scene.scale, at.y as f32 / scene.scale)
                .unwrap()
                .tag
                != tag(it, ROW)
            {
                let row = scene.hits.iter().find(|h| h.tag == tag(it, ROW)).unwrap();
                at.x = f64::from((row.rect.x + 1.0) * scene.scale);
            }
            let effects = if touch_input {
                r.event(&touch(TouchPhase::Started, at), &scene);
                r.event(&touch(TouchPhase::Ended, at), &scene)
            } else {
                r.event(
                    &WindowEvent::CursorMoved {
                        device_id: DeviceId::dummy(),
                        position: at,
                    },
                    &scene,
                );
                assert_eq!(r.model.hovered(), Some(Hit::Item(it)));
                r.event(&mouse(ElementState::Pressed), &scene);
                r.event(&mouse(ElementState::Released), &scene)
            };
            assert_eq!(r.model.item(), it);
            match it {
                Item::Resume => assert!(effects.contains(&Effect::Resume)),
                Item::Settings => assert_eq!(r.model.page(), Page::Settings),
                Item::Screenshot => assert!(effects.contains(&Effect::Screenshot)),
                Item::TouchLayout => assert!(effects.contains(&Effect::EditLayout)),
                Item::Retry | Item::Quit => {
                    assert!(r.asking());
                    assert!(!r.model.yes());
                    assert!(
                        r.tap_answer(false)
                            .iter()
                            .all(|&e| e == Effect::HoverChanged)
                    );
                    assert!(!r.asking());
                }
                _ => {}
            }
        }
    }
}

fn mouse(state: ElementState) -> WindowEvent {
    WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        state,
        button: MouseButton::Left,
    }
}

#[test]
fn one_tap_asks_for_a_frame_and_confirms_never_leak_to_the_card() {
    let r = rig(true, false);
    let s = r.scene();
    for it in [Item::Retry, Item::Quit] {
        assert!(s.hits.iter().all(|h| h.tag != tag(it, ROW)));
        assert!(!r.model.items().contains(&it));
    }

    let mut r = rig(true, true);
    r.model.take_frame();
    r.tap_item(Item::Settings);
    assert!(r.model.take_frame());
    assert!(!r.model.take_frame());
    assert!(r.model.back());
    for (it, effect) in [(Item::Retry, Effect::Retry), (Item::Quit, Effect::Quit)] {
        r.model.take_frame();
        let mut out = r.tap_item(it);
        assert!(r.model.take_frame());
        let confirm = r.scene();
        assert_eq!(
            confirm.hits.iter().map(|h| h.tag).collect::<Vec<_>>(),
            [KEEP, ACCEPT]
        );
        assert!(!r.model.yes());
        out.extend(r.tap_answer(true));
        assert_eq!(out, [effect]);
        assert!(r.model.take_frame());
        assert!(!r.asking());
    }
}

#[test]
fn touch_track_locks_to_the_first_axis_and_other_drags_do_not_activate() {
    let mut r = rig(true, true);
    let scene = r.scene();
    let at = center(&scene, tag(Item::Volume, TRACK_TAG));
    let moved = |dx: f64, dy: f64| PhysicalPosition::new(at.x + dx, at.y + dy);
    let before = r.model.store.value_text(VOLUME);
    r.event(&touch(TouchPhase::Started, at), &scene);
    r.event(&touch(TouchPhase::Moved, moved(0.0, 30.0)), &scene);
    r.event(&touch(TouchPhase::Ended, moved(-70.0, 30.0)), &scene);
    assert_eq!(r.model.store.value_text(VOLUME), before);
    r.event(&touch(TouchPhase::Started, at), &scene);
    assert!(
        r.event(&touch(TouchPhase::Moved, moved(-40.0, 0.0)), &scene)
            .contains(&Effect::SettingsChanged)
    );
    r.event(&touch(TouchPhase::Ended, moved(-40.0, 0.0)), &scene);
    assert_ne!(r.model.store.value_text(VOLUME), before);
    let at = center(&scene, tag(Item::Retry, ROW));
    for (phase, p) in [
        (TouchPhase::Started, at),
        (TouchPhase::Moved, PhysicalPosition::new(at.x, at.y - 25.0)),
        (TouchPhase::Ended, at),
    ] {
        r.event(&touch(phase, p), &scene);
    }
    assert!(!r.asking());
}

#[test]
fn card_confirm_card_makes_no_gpu_objects() {
    let Ok(mut gfx) = pollster::block_on(Gfx::headless()) else {
        assert!(
            std::env::var_os("TH08_REQUIRE_GPU").is_none(),
            "no GPU adapter"
        );
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let size = (1920, 1080);
    let tex = gfx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Scene card transition gate"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gfx.surface_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let before = gfx.scene_renderer().unwrap().made();
    let mut frames = 0;
    for lang in ["en", "ja", "ru"] {
        let mut cfg = Settings::default();
        cfg.set_from(crate::settings::Origin::Cli, "language", lang)
            .unwrap();
        for (window, touch, density) in [
            (size, false, 1.0),
            ((1280, 960), false, 2.0),
            ((1600, 720), true, 2.0),
        ] {
            let shot = Shot {
                quit: true,
                touch,
                density,
                pad: (!touch).then(|| "Pad 手柄 🎮".into()),
                ..Shot::default()
            };
            let mut r = Rig::new(&cfg, window, SafeArea::default(), &shot);
            for (ask, item, effect) in [
                (Ask::Retry, Item::Retry, Effect::Retry),
                (Ask::Quit, Item::Quit, Effect::Quit),
            ] {
                for confirm in [false, true, false] {
                    if confirm {
                        assert!(r.tap_item(item).iter().all(|&e| e == Effect::HoverChanged));
                        assert_eq!(r.model.ask(), Some(ask));
                    } else if r.asking() {
                        assert!(
                            r.tap_answer(false)
                                .iter()
                                .all(|&e| e == Effect::HoverChanged)
                        );
                    }
                    let scene = r.scene();
                    let mut enc = gfx
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                    gfx.draw_scene(&scene, &view, size, &mut enc);
                    gfx.queue.submit([enc.finish()]);
                    assert_eq!(gfx.scene_renderer().unwrap().dropped_glyphs(), 0);
                    frames += 1;
                }
                r.tap_item(item);
                assert!(r.tap_answer(true).contains(&effect));
            }
        }
    }
    gfx.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(gfx.scene_renderer().unwrap().made(), before);
    eprintln!("Scene card GPU gate: {frames} frames, zero creations, zero dropped glyphs");
}
