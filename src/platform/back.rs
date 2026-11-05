//! Android OnBackInvokedCallback: the game activity's Back (gesture, button) as a callback into the event loop.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static PENDING: AtomicU32 = AtomicU32::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);

// Binds org.th08.port.BackBridge.nativeBack (UI thread): each call counts one Back and wakes the event loop.
#[cfg(target_os = "android")]
pub fn bind(app: &winit::platform::android::activity::AndroidApp) {
    if android::WAKER.set(app.create_waker()).is_ok() && android::register().is_none() {
        log::warn!("back callback: nativeBack not bound, KEYCODE_BACK only");
    }
}

// The callback is registered (API 33+) and bound: KEYCODE_BACK reaches the game through it, not as a key event.
pub fn callback_active() -> bool {
    ACTIVE.load(Ordering::Acquire)
}

// The Back presses since the last call.
pub fn take() -> u32 {
    PENDING.swap(0, Ordering::AcqRel)
}

#[cfg(target_os = "android")]
mod android {
    use jni::objects::JClassLoader;
    use jni::refs::LoaderContext;
    use jni::{NativeMethod, jni_sig, jni_str};
    use std::sync::OnceLock;

    use winit::platform::android::activity::AndroidAppWaker;

    pub static WAKER: OnceLock<AndroidAppWaker> = OnceLock::new();

    extern "system" fn native_back(_: *mut jni::sys::JNIEnv, _: jni::sys::jclass) {
        super::PENDING.fetch_add(1, super::Ordering::AcqRel);
        if let Some(w) = WAKER.get() {
            w.wake();
        }
    }

    pub fn register() -> Option<()> {
        crate::platform::safearea::with_activity(|env, act| {
            let loader = env
                .call_method(
                    act,
                    jni_str!("getClassLoader"),
                    jni_sig!("()Ljava/lang/ClassLoader;"),
                    &[],
                )?
                .l()?;
            let loader = env.cast_local::<JClassLoader>(loader)?;
            let class = LoaderContext::Loader(&loader).load_class(
                env,
                jni_str!("org.th08.port.BackBridge"),
                false,
            )?;
            // SAFETY: `native_back` is `static void nativeBack()`: (JNIEnv*, jclass) -> void.
            let m = unsafe {
                NativeMethod::from_raw_parts(
                    jni_str!("nativeBack"),
                    jni_str!("()V"),
                    native_back as *mut std::ffi::c_void,
                )
            };
            unsafe { env.register_native_methods(&class, &[m]) }?;
            let sdk = env
                .get_static_field(
                    jni_str!("android/os/Build$VERSION"),
                    jni_str!("SDK_INT"),
                    jni_sig!(jint),
                )?
                .i()?;
            // GameActivity.kt registers the callback from API 33 (TIRAMISU).
            super::ACTIVE.store(sdk >= 33, super::Ordering::Release);
            log::info!("back callback: bound (sdk {sdk})");
            Ok(())
        })
    }
}
