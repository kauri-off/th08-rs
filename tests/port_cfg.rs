//! Tests and tools ignore the user's port.cfg and save dir; only the game entry points read them.
#![cfg(not(target_arch = "wasm32"))]

use th08::settings::{self, Origin, Settings};

// Own process (integration test): the env changes cannot race the unit tests.
#[test]
fn tools_ignore_port_cfg() {
    let home = std::env::temp_dir().join(format!("th08-port-cfg-test-{}", std::process::id()));
    std::fs::create_dir_all(home.join("th08")).unwrap();
    std::fs::write(home.join("th08/port.cfg"), "invuln = on\nscale = 3\n").unwrap();
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &home);
        std::env::remove_var("TH08_INVULN");
        std::env::remove_var("TH08_SCALE");
    }
    let dirs = th08::launcher::desktop_dirs();
    assert_eq!(dirs.port_cfg(), Some(home.join("th08/port.cfg")));
    assert_eq!(dirs.save, Some(home.join("th08")));

    let game = Settings::game_environment(dirs.clone());
    assert!(!game.invuln && game.scale == 3);
    assert_eq!(game.origin("scale"), Origin::PortCfg);
    let play = Settings::game_from_args(dirs.clone(), ["--stage".into(), "1".into()]).unwrap();
    assert!(play.scale == 3 && !play.hermetic);
    let shot = Settings::game_from_args(dirs.clone(), ["--shot".into(), "9".into()]).unwrap();
    assert!(shot.scale == 1 && shot.hermetic && shot.storage_dir().is_none());

    for s in [
        Settings::from_environment(dirs.clone()),
        Settings::from_args(dirs.clone(), ["--stage".into(), "1".into()]).unwrap(),
        settings::get().clone(),
    ] {
        assert!(!s.invuln && s.scale == Settings::default().scale);
        assert_eq!(s.origin("invuln"), Origin::Default);
        assert!(s.hermetic);
    }
    assert_eq!(settings::get().dirs.config, None);
    assert_eq!(settings::get().storage_dir(), None);
    let _ = std::fs::remove_dir_all(&home);
}
