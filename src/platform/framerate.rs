//! Android: asks SurfaceFlinger for a fixed 60 Hz source on the game window (ANativeWindow_setFrameRate, API 30+).

// Called once a surface is configured on a native window; a no-op elsewhere and below API 30.
#[cfg(not(target_os = "android"))]
pub fn request_60(_: &winit::window::Window) {}

#[cfg(target_os = "android")]
pub fn request_60(window: &winit::window::Window) {
    use std::ffi::{c_char, c_int, c_void};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    type SetFrameRate = unsafe extern "C" fn(*mut c_void, f32, i8) -> i32;
    unsafe extern "C" {
        fn dlopen(name: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(lib: *mut c_void, name: *const c_char) -> *mut c_void;
    }
    const RTLD_NOW: c_int = 2;
    const FIXED_SOURCE: i8 = 1;
    let Ok(h) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AndroidNdk(h) = h.as_raw() else {
        return;
    };
    // SAFETY: libandroid.so is always loaded in an app process; the symbol, when present, has the NDK signature.
    let f = unsafe {
        let lib = dlopen(c"libandroid.so".as_ptr(), RTLD_NOW);
        if lib.is_null() {
            return;
        }
        dlsym(lib, c"ANativeWindow_setFrameRate".as_ptr())
    };
    if f.is_null() {
        log::info!("frame rate: ANativeWindow_setFrameRate unavailable (API < 30)");
        return;
    }
    // SAFETY: `f` is ANativeWindow_setFrameRate; the window pointer is the live surface winit handed out.
    let r = unsafe {
        std::mem::transmute::<*mut c_void, SetFrameRate>(f)(
            h.a_native_window.as_ptr(),
            60.0,
            FIXED_SOURCE,
        )
    };
    log::info!("frame rate: requested 60 Hz fixed source ({r})");
}
