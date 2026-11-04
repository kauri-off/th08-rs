//! Android system gesture exclusion: no back swipe from the screen edges over the touch controls in play.

// Sets the exclusion rects `[left, top, right, bottom]` (window px) of the game window; repeats are skipped.
#[cfg(target_os = "android")]
pub fn set_exclusion(rects: &[[i32; 4]]) {
    use std::sync::Mutex;
    static LAST: Mutex<Vec<[i32; 4]>> = Mutex::new(Vec::new());
    let mut last = LAST.lock().unwrap();
    if last.as_slice() == rects {
        return;
    }
    log::info!("gesture exclusion: {rects:?}");
    if android::set(rects).is_some() {
        *last = rects.to_vec();
    }
}

#[cfg(not(target_os = "android"))]
pub fn set_exclusion(_: &[[i32; 4]]) {}

#[cfg(target_os = "android")]
mod android {
    use jni::objects::{JClassLoader, JValue};
    use jni::refs::LoaderContext;
    use jni::{jni_sig, jni_str};

    // org.th08.port.GestureExclusion.set(activity, rects): the decor view's rects, set on the UI thread.
    pub fn set(rects: &[[i32; 4]]) -> Option<()> {
        crate::platform::safearea::with_activity(|env, act| {
            let flat: Vec<i32> = rects.iter().flatten().copied().collect();
            let arr = env.new_int_array(flat.len())?;
            arr.set_region(env, 0, &flat)?;
            // The app's classes come from the activity's loader, not the native thread's system loader.
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
                jni_str!("org.th08.port.GestureExclusion"),
                false,
            )?;
            env.call_static_method(
                &class,
                jni_str!("set"),
                jni_sig!("(Landroid/app/Activity;[I)V"),
                &[JValue::Object(act), JValue::Object(&arr)],
            )?;
            Ok(())
        })
    }
}
