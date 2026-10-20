//! Native launcher flow, not a port of the original: finds the game folder (command line → TH08_DIR → port.cfg
//! `game_dir` → a folder dialog) and remembers a picked folder in port.cfg.

use crate::settings::{self, Origin, Settings};
use std::path::{Path, PathBuf};

// A game folder holds th08.dat (any letter case).
pub fn is_game_dir(dir: &Path) -> bool {
    crate::assets::find_file(dir, "th08.dat").is_file()
}

// The game folder to run; Ok(None) = dialog cancelled, Err = the folder given on the command line / TH08_DIR is invalid.
pub fn resolve_game_dir(s: &Settings) -> Result<Option<PathBuf>, String> {
    if let Some(d) = &s.game_dir {
        if is_game_dir(d) {
            return Ok(Some(d.clone()));
        }
        let msg = format!("{}: no th08.dat in this folder", d.display());
        if s.origin("game_dir") > Origin::PortCfg {
            return Err(msg);
        }
        log::warn!("port.cfg {msg}; asking for the folder");
    }
    Ok(pick_game_dir())
}

// Folder dialog until a valid folder is picked (saved as port.cfg `game_dir`) or the dialog is cancelled.
fn pick_game_dir() -> Option<PathBuf> {
    loop {
        let Some(dir) = rfd::FileDialog::new()
            .set_title("Select the Touhou 8 (Imperishable Night) game folder")
            .pick_folder()
        else {
            log::info!("folder dialog cancelled or unavailable");
            eprintln!("no game folder selected (usage: th08 <game dir>, see --help)");
            return None;
        };
        if is_game_dir(&dir) {
            settings::save_port_cfg("game_dir", &dir.to_string_lossy());
            return Some(dir);
        }
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("th08")
            .set_description(format!(
                "{} does not contain th08.dat.\nSelect the Imperishable Night folder.",
                dir.display()
            ))
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
}
