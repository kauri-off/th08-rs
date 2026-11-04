//! Android gamepads: the connected InputDevice list (hot-plug, the active device's name) via JNI.

#[cfg(target_os = "android")]
pub use android::devices;

#[cfg(target_os = "android")]
mod android {
    use jni::objects::{JIntArray, JString};
    use jni::{Env, JValue, JavaVM, jni_sig, jni_str};

    // InputDevice.SOURCE_GAMEPAD / SOURCE_JOYSTICK.
    const SOURCE_GAMEPAD: i32 = 0x0000_0401;
    const SOURCE_JOYSTICK: i32 = 0x0100_0010;

    // Names of the connected non-virtual gamepads / joysticks; None when JNI fails.
    pub fn devices() -> Option<Vec<String>> {
        static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let vm_ptr = ndk_context::android_context().vm();
        if vm_ptr.is_null() {
            return None;
        }
        // SAFETY: the process's JavaVM from ndk-context, valid for the lifetime of the process.
        let vm = unsafe { JavaVM::from_raw(vm_ptr.cast()) };
        let r = vm.attach_current_thread(|env| -> jni::errors::Result<Vec<String>> {
            let r = list(env);
            if env.exception_check() {
                env.exception_clear();
            }
            r
        });
        r.map_err(|e| {
            if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                log::warn!("gamepad list via JNI failed: {e}");
            }
        })
        .ok()
    }

    fn list(env: &mut Env) -> jni::errors::Result<Vec<String>> {
        let class = jni_str!("android/view/InputDevice");
        let ids = env
            .call_static_method(class, jni_str!("getDeviceIds"), jni_sig!("()[I"), &[])?
            .l()?;
        let ids = JIntArray::cast_local(env, ids)?;
        let mut buf = vec![0; ids.len(env)?];
        ids.get_region(env, 0, &mut buf)?;
        let mut out = Vec::new();
        for id in buf {
            let dev = env
                .call_static_method(
                    class,
                    jni_str!("getDevice"),
                    jni_sig!("(I)Landroid/view/InputDevice;"),
                    &[JValue::Int(id)],
                )?
                .l()?;
            if dev.is_null() {
                continue;
            }
            let sources = env
                .call_method(&dev, jni_str!("getSources"), jni_sig!("()I"), &[])?
                .i()?;
            let virt = env
                .call_method(&dev, jni_str!("isVirtual"), jni_sig!("()Z"), &[])?
                .z()?;
            let pad = sources & SOURCE_GAMEPAD == SOURCE_GAMEPAD
                || sources & SOURCE_JOYSTICK == SOURCE_JOYSTICK;
            if !pad || virt {
                continue;
            }
            let name = env
                .call_method(
                    &dev,
                    jni_str!("getName"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )?
                .l()?;
            let name = JString::cast_local(env, name)?.try_to_string(env)?;
            out.push(name);
        }
        Ok(out)
    }
}
