//! Window safe area (port UI, ARCH_D_MOBILE §3.5 "Safe areas"): Android system bars, display cutout and rounded
//! corners as per-edge insets, plus the `screen_margin` setting. Desktop / web: the margin only.

pub use th08_render::gfx::SafeArea;
use winit::window::Window;

// Insets of the window plus `margin_dp` (dp, × the scale factor) on every edge.
#[expect(
    clippy::cast_sign_loss,
    reason = "margin and scale factor are positive"
)]
pub fn read(window: &Window, margin_dp: u32) -> SafeArea {
    let m = (f64::from(margin_dp) * window.scale_factor()).round() as u32;
    let base = platform_insets(window);
    SafeArea {
        left: base.left + m,
        top: base.top + m,
        right: base.right + m,
        bottom: base.bottom + m,
    }
}

#[cfg(not(target_os = "android"))]
fn platform_insets(_: &Window) -> SafeArea {
    SafeArea::default()
}

#[cfg(target_os = "android")]
fn platform_insets(window: &Window) -> SafeArea {
    let size = window.inner_size();
    match android::insets() {
        Some((edges, radii)) => fold_corners(edges, radii),
        None => android::content_rect_insets(size.width, size.height),
    }
}

// Grows edge insets `[left, top, right, bottom]` so the safe rect's corners clear rounded display corners of radius
// `radii` `[top-left, top-right, bottom-right, bottom-left]` (px). Per corner the cheapest of: grow the horizontal
// edge only, the vertical edge only, or both to the arc's 45° point r·(1 − 1/√2).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
#[expect(clippy::cast_sign_loss, reason = "insets grow from u32 edges, ≥ 0")]
pub fn fold_corners(edges: [u32; 4], radii: [u32; 4]) -> SafeArea {
    let [mut l, mut t, mut r, mut b] = edges.map(|e| e as f32);
    for (i, &rad) in radii.iter().enumerate() {
        let rad = rad as f32;
        let (h, v) = match i {
            0 => (&mut l, &mut t),
            1 => (&mut r, &mut t),
            2 => (&mut r, &mut b),
            _ => (&mut l, &mut b),
        };
        let (fh, fv) = corner_fix(*h, *v, rad);
        *h = fh;
        *v = fv;
    }
    let up = |x: f32| x.ceil() as u32;
    SafeArea {
        left: up(l),
        top: up(t),
        right: up(r),
        bottom: up(b),
    }
}

// Insets (a along one edge, b along the other) such that the point (a, b) from the corner lies on or inside the
// arc of radius `rad` centred at (rad, rad).
fn corner_fix(a: f32, b: f32, rad: f32) -> (f32, f32) {
    let inside =
        |a: f32, b: f32| a >= rad || b >= rad || (rad - a).powi(2) + (rad - b).powi(2) <= rad * rad;
    if rad <= 0.0 || inside(a, b) {
        return (a, b);
    }
    // The other inset needed for a fixed inset x < rad: rad − √(rad² − (rad − x)²).
    let need = |x: f32| rad - (rad * rad - (rad - x).powi(2)).max(0.0).sqrt();
    let d = rad * (1.0 - std::f32::consts::FRAC_1_SQRT_2);
    let opts = [(a, need(a)), (need(b), b), (a.max(d), b.max(d))];
    opts.into_iter()
        .filter(|&(x, y)| inside(x + 1e-3, y + 1e-3))
        .min_by(|p, q| (p.0 + p.1).total_cmp(&(q.0 + q.1)))
        .unwrap_or((a.max(rad), b))
}

#[cfg(target_os = "android")]
mod android {
    use super::SafeArea;
    use jni::objects::JObject;
    use jni::refs::Reference;
    use jni::{Env, JValue, JavaVM, jni_sig, jni_str};
    use std::sync::OnceLock;
    use winit::platform::android::activity::AndroidApp;

    static APP: OnceLock<AndroidApp> = OnceLock::new();

    pub fn set_app(app: &AndroidApp) {
        let _ = APP.set(app.clone());
    }

    // Fallback without JNI: the NativeActivity content rect (window px).
    pub fn content_rect_insets(w: u32, h: u32) -> SafeArea {
        let Some(app) = APP.get() else {
            return SafeArea::default();
        };
        let r = app.content_rect();
        if r.right <= r.left || r.bottom <= r.top {
            return SafeArea::default();
        }
        let c = |v: i32| v.max(0).cast_unsigned();
        SafeArea {
            left: c(r.left),
            top: c(r.top),
            right: c(w.cast_signed() - r.right),
            bottom: c(h.cast_signed() - r.bottom),
        }
    }

    // `[left, top, right, bottom]` of systemBars | displayCutout and the rounded-corner radii `[tl, tr, br, bl]` of
    // the decor view's root WindowInsets (px); None when JNI fails or the view has no insets yet.
    pub fn insets() -> Option<([u32; 4], [u32; 4])> {
        let r = with_activity(|env, act| {
            let sdk = env
                .get_static_field(
                    jni_str!("android/os/Build$VERSION"),
                    jni_str!("SDK_INT"),
                    jni_sig!(jint),
                )?
                .i()?;
            let win = env
                .call_method(
                    act,
                    jni_str!("getWindow"),
                    jni_sig!("()Landroid/view/Window;"),
                    &[],
                )?
                .l()?;
            let decor = env
                .call_method(
                    &win,
                    jni_str!("getDecorView"),
                    jni_sig!("()Landroid/view/View;"),
                    &[],
                )?
                .l()?;
            let wi = env
                .call_method(
                    &decor,
                    jni_str!("getRootWindowInsets"),
                    jni_sig!("()Landroid/view/WindowInsets;"),
                    &[],
                )?
                .l()?;
            if wi.is_null() {
                return Ok(None);
            }
            let mut edges = [0i32; 4];
            if sdk >= 30 {
                let ty = jni_str!("android/view/WindowInsets$Type");
                let bars = env
                    .call_static_method(ty, jni_str!("systemBars"), jni_sig!("()I"), &[])?
                    .i()?;
                let cut = env
                    .call_static_method(ty, jni_str!("displayCutout"), jni_sig!("()I"), &[])?
                    .i()?;
                let ins = env
                    .call_method(
                        &wi,
                        jni_str!("getInsets"),
                        jni_sig!("(I)Landroid/graphics/Insets;"),
                        &[JValue::Int(bars | cut)],
                    )?
                    .l()?;
                edges = [
                    env.get_field(&ins, jni_str!("left"), jni_sig!(jint))?.i()?,
                    env.get_field(&ins, jni_str!("top"), jni_sig!(jint))?.i()?,
                    env.get_field(&ins, jni_str!("right"), jni_sig!(jint))?
                        .i()?,
                    env.get_field(&ins, jni_str!("bottom"), jni_sig!(jint))?
                        .i()?,
                ];
            } else {
                let names = [
                    jni_str!("getSystemWindowInsetLeft"),
                    jni_str!("getSystemWindowInsetTop"),
                    jni_str!("getSystemWindowInsetRight"),
                    jni_str!("getSystemWindowInsetBottom"),
                ];
                for (e, n) in edges.iter_mut().zip(names) {
                    *e = env.call_method(&wi, n, jni_sig!("()I"), &[])?.i()?;
                }
                if sdk >= 28 {
                    let dc = env
                        .call_method(
                            &wi,
                            jni_str!("getDisplayCutout"),
                            jni_sig!("()Landroid/view/DisplayCutout;"),
                            &[],
                        )?
                        .l()?;
                    if !dc.is_null() {
                        let names = [
                            jni_str!("getSafeInsetLeft"),
                            jni_str!("getSafeInsetTop"),
                            jni_str!("getSafeInsetRight"),
                            jni_str!("getSafeInsetBottom"),
                        ];
                        for (e, n) in edges.iter_mut().zip(names) {
                            *e = (*e).max(env.call_method(&dc, n, jni_sig!("()I"), &[])?.i()?);
                        }
                    }
                }
            }
            let mut radii = [0i32; 4];
            if sdk >= 31 {
                // RoundedCorner.POSITION_TOP_LEFT 0, TOP_RIGHT 1, BOTTOM_RIGHT 2, BOTTOM_LEFT 3.
                for (pos, r) in (0i32..).zip(radii.iter_mut()) {
                    let rc = env
                        .call_method(
                            &wi,
                            jni_str!("getRoundedCorner"),
                            jni_sig!("(I)Landroid/view/RoundedCorner;"),
                            &[JValue::Int(pos)],
                        )?
                        .l()?;
                    if !rc.is_null() {
                        *r = env
                            .call_method(&rc, jni_str!("getRadius"), jni_sig!("()I"), &[])?
                            .i()?;
                    }
                }
            }
            Ok(Some((
                edges.map(|v| v.max(0).cast_unsigned()),
                radii.map(|v| v.max(0).cast_unsigned()),
            )))
        });
        r.flatten()
    }

    // Runs `f` with the NativeActivity object on this thread's JNI env (attached if needed); errors (and a pending
    // Java exception) are logged once per kind and give None.
    fn with_activity<R>(f: impl FnOnce(&mut Env, &JObject) -> jni::errors::Result<R>) -> Option<R> {
        // ndk-context's context is the Application; the activity (for its Window) comes from the AndroidApp.
        let vm_ptr = ndk_context::android_context().vm();
        let act_ptr = APP.get()?.activity_as_ptr();
        if vm_ptr.is_null() || act_ptr.is_null() {
            return None;
        }
        // SAFETY: the process's JavaVM (ndk-context) and android-activity's global ref to the NativeActivity object,
        // both valid for the lifetime of the process.
        let vm = unsafe { JavaVM::from_raw(vm_ptr.cast()) };
        let r = vm.attach_current_thread(|env| -> jni::errors::Result<R> {
            let act = unsafe { JObject::from_raw(env, act_ptr.cast()) };
            let r = f(env, &act);
            if env.exception_check() {
                env.exception_clear();
            }
            r
        });
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                static LOGGED: std::sync::atomic::AtomicBool =
                    std::sync::atomic::AtomicBool::new(false);
                if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    log::warn!("safe area via JNI failed ({e}); using the content rect");
                }
                None
            }
        }
    }
}

#[cfg(target_os = "android")]
pub use android::set_app;

#[cfg(test)]
mod tests {
    use super::*;

    fn sa(l: u32, t: u32, r: u32, b: u32) -> SafeArea {
        SafeArea {
            left: l,
            top: t,
            right: r,
            bottom: b,
        }
    }

    // The Poco F6 landscape case: cutout left 106, gesture bar bottom 72, radius 108 everywhere.
    #[test]
    fn phone_corners() {
        let s = fold_corners([106, 0, 0, 72], [108; 4]);
        // Left 106 alone nearly clears the top-left arc; the top-right corner takes the 45° point on both edges.
        assert_eq!(s.left, 106);
        assert_eq!(s.right, 32);
        assert!(s.top <= 32 && s.bottom >= 72, "{s:?}");
        for (a, b) in [
            (s.left, s.top),
            (s.right, s.top),
            (s.right, s.bottom),
            (s.left, s.bottom),
        ] {
            let (a, b) = (a as f32, b as f32);
            assert!(
                a >= 108.0
                    || b >= 108.0
                    || (108.0 - a).powi(2) + (108.0 - b).powi(2) <= 108.0f32.powi(2) + 1.0
            );
        }
    }

    #[test]
    fn no_corners_no_change() {
        assert_eq!(fold_corners([1, 2, 3, 4], [0; 4]), sa(1, 2, 3, 4));
        // A bar thicker than the radius already clears both corners on its edge.
        assert_eq!(
            fold_corners([0, 120, 0, 0], [100, 100, 0, 0]),
            sa(0, 120, 0, 0)
        );
    }

    #[test]
    fn safe_rect() {
        assert_eq!(sa(0, 0, 0, 0).rect(640, 480), [0.0, 0.0, 640.0, 480.0]);
        assert_eq!(
            sa(106, 0, 32, 72).rect(2712, 1220),
            [106.0, 0.0, 2574.0, 1148.0]
        );
        assert_eq!(sa(900, 0, 900, 0).rect(1000, 10), [900.0, 0.0, 1.0, 10.0]);
    }
}
