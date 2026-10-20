//! Tests and tools ignore the user's port.cfg; only the game entry points read it (port infrastructure).
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
    assert_eq!(settings::port_cfg_path(), Some(home.join("th08/port.cfg")));

    let game = Settings::game_environment();
    assert!(game.invuln && game.scale == 3);
    assert_eq!(game.origin("invuln"), Origin::PortCfg);
    assert!(
        Settings::game_from_args(["--stage".into(), "1".into()])
            .unwrap()
            .invuln
    );

    for s in [
        Settings::from_environment(),
        Settings::from_args(["--stage".into(), "1".into()]).unwrap(),
        settings::get().clone(),
    ] {
        assert!(!s.invuln && s.scale == Settings::default().scale);
        assert_eq!(s.origin("invuln"), Origin::Default);
    }
    let _ = std::fs::remove_dir_all(&home);
}
