pub mod app;
pub mod archive;
pub mod assets;
pub mod audio;
pub mod gfx;
pub mod input;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn start(files: js_sys::Array) {
    use wasm_bindgen::JsCast;
    console_error_panic_hook::set_once();
    let files = files.iter().filter_map(|f| f.dyn_into::<web_sys::File>().ok()).collect();
    app::run(assets::Source::web(files));
}
