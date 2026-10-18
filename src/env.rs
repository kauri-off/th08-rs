// Debug settings: env vars natively; on the web `?stage_frame=600` is read as TH08_STAGE_FRAME.
#[cfg(not(target_arch = "wasm32"))]
pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

#[cfg(target_arch = "wasm32")]
pub fn var(name: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
    params.get(
        &name
            .strip_prefix("TH08_")
            .unwrap_or(name)
            .to_ascii_lowercase(),
    )
}
