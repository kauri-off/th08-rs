pub mod anm;
pub mod anmdebug;
pub mod anmmgr;
pub mod anmvm;
pub mod app;
pub mod archive;
pub mod assets;
pub mod audio;
pub mod bullet;
pub mod ecl;
pub mod eclvm;
pub mod enemy;
pub mod env;
pub mod game;
pub mod gfx;
pub mod input;
pub mod player;
pub mod rng;
pub mod stage;
pub mod stdfile;
pub mod supervisor;
pub mod task;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn start(files: js_sys::Array) {
    use wasm_bindgen::JsCast;
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);
    let files = files.iter().filter_map(|f| f.dyn_into::<web_sys::File>().ok()).collect();
    app::run(assets::Source::web(files));
}
