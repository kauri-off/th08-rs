//! PNG screenshots (F12, port menu): file name, encoding and where each platform keeps them.

// `th08_YYYY-MM-DD_HHMMSS.png` from local date and time parts.
pub fn file_name([y, mo, d, h, mi, s]: [u32; 6]) -> String {
    format!("th08_{y:04}-{mo:02}-{d:02}_{h:02}{mi:02}{s:02}.png")
}

#[cfg(not(target_arch = "wasm32"))]
fn now() -> [u32; 6] {
    use chrono::{Datelike, Timelike};
    let t = chrono::Local::now();
    [
        t.year().cast_unsigned(),
        t.month(),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
    ]
}

#[cfg(target_arch = "wasm32")]
fn now() -> [u32; 6] {
    let d = js_sys::Date::new_0();
    [
        d.get_full_year(),
        d.get_month() + 1,
        d.get_date(),
        d.get_hours(),
        d.get_minutes(),
        d.get_seconds(),
    ]
}

// Opaque PNG of the frame (the render target's alpha is not part of the picture).
pub fn encode(img: &image::RgbaImage) -> Result<Vec<u8>, String> {
    let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
    let mut out = std::io::Cursor::new(Vec::new());
    rgb.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

// Saves `img`; Ok = where it went, as shown in the toast. Desktop: `<save dir>/screenshots/`; web: a browser
// download; Android: Pictures/th08 through MediaStore.
pub fn save(img: &image::RgbaImage, save_dir: Option<&std::path::Path>) -> Result<String, String> {
    let png = encode(img)?;
    let name = file_name(now());
    store(&name, &png, save_dir)
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn store(name: &str, png: &[u8], save_dir: Option<&std::path::Path>) -> Result<String, String> {
    let dir = save_dir.ok_or("no save folder")?.join("screenshots");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(name);
    std::fs::write(&path, png).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path.display().to_string())
}

#[cfg(target_arch = "wasm32")]
fn store(name: &str, png: &[u8], _: Option<&std::path::Path>) -> Result<String, String> {
    crate::saves::download(name, "image/png", png)?;
    Ok(name.to_string())
}

#[cfg(target_os = "android")]
fn store(name: &str, png: &[u8], save_dir: Option<&std::path::Path>) -> Result<String, String> {
    if let Some(r) = android::media_store(name, png) {
        return r;
    }
    // Before Android 10 (no RELATIVE_PATH without the storage permission): the app's own Pictures folder.
    let dir = android::pictures_dir()
        .or_else(|| save_dir.map(|d| d.join("screenshots")))
        .ok_or("no pictures folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(name);
    std::fs::write(&path, png).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path.display().to_string())
}

#[cfg(target_os = "android")]
mod android {
    use crate::platform::safearea::with_activity;
    use jni::objects::JString;
    use jni::{JValue, jni_sig, jni_str};

    // MediaStore insert into Pictures/th08 (Android 10+); None below API 29 or when JNI fails.
    pub fn media_store(name: &str, png: &[u8]) -> Option<Result<String, String>> {
        with_activity(|env, act| {
            let sdk = env
                .get_static_field(
                    jni_str!("android/os/Build$VERSION"),
                    jni_str!("SDK_INT"),
                    jni_sig!(jint),
                )?
                .i()?;
            if sdk < 29 {
                return Ok(None);
            }
            let resolver = env
                .call_method(
                    act,
                    jni_str!("getContentResolver"),
                    jni_sig!("()Landroid/content/ContentResolver;"),
                    &[],
                )?
                .l()?;
            let values = env.new_object(
                jni_str!("android/content/ContentValues"),
                jni_sig!("()V"),
                &[],
            )?;
            for (k, v) in [
                ("_display_name", name),
                ("mime_type", "image/png"),
                ("relative_path", "Pictures/th08"),
            ] {
                let (k, v) = (env.new_string(k)?, env.new_string(v)?);
                env.call_method(
                    &values,
                    jni_str!("put"),
                    jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"),
                    &[JValue::from(&k), JValue::from(&v)],
                )?;
            }
            let ext = env
                .get_static_field(
                    jni_str!("android/provider/MediaStore$Images$Media"),
                    jni_str!("EXTERNAL_CONTENT_URI"),
                    jni_sig!("Landroid/net/Uri;"),
                )?
                .l()?;
            let uri = env
                .call_method(
                    &resolver,
                    jni_str!("insert"),
                    jni_sig!("(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;"),
                    &[JValue::from(&ext), JValue::from(&values)],
                )?
                .l()?;
            if uri.is_null() {
                return Ok(Some(Err("MediaStore refused the picture".to_string())));
            }
            let os = env
                .call_method(
                    &resolver,
                    jni_str!("openOutputStream"),
                    jni_sig!("(Landroid/net/Uri;)Ljava/io/OutputStream;"),
                    &[JValue::from(&uri)],
                )?
                .l()?;
            if os.is_null() {
                return Ok(Some(Err("MediaStore gave no output stream".to_string())));
            }
            let arr = env.byte_array_from_slice(png)?;
            env.call_method(
                &os,
                jni_str!("write"),
                jni_sig!("([B)V"),
                &[JValue::from(&arr)],
            )?;
            env.call_method(&os, jni_str!("close"), jni_sig!("()V"), &[])?;
            Ok(Some(Ok(format!("Pictures/th08/{name}"))))
        })
        .flatten()
    }

    // `getExternalFilesDir(DIRECTORY_PICTURES)`.
    pub fn pictures_dir() -> Option<std::path::PathBuf> {
        with_activity(|env, act| {
            let ty = env.new_string("Pictures")?;
            let f = env
                .call_method(
                    act,
                    jni_str!("getExternalFilesDir"),
                    jni_sig!("(Ljava/lang/String;)Ljava/io/File;"),
                    &[JValue::from(&ty)],
                )?
                .l()?;
            if f.is_null() {
                return Ok(None);
            }
            let p = env
                .call_method(
                    &f,
                    jni_str!("getAbsolutePath"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )?
                .l()?;
            let p: JString = env.cast_local::<JString>(p)?;
            Ok(Some(std::path::PathBuf::from(p.try_to_string(env)?)))
        })
        .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_and_png() {
        assert_eq!(
            file_name([2026, 11, 6, 9, 5, 7]),
            "th08_2026-11-06_090507.png"
        );
        let img = image::RgbaImage::from_pixel(640, 480, image::Rgba([10, 20, 30, 0]));
        let png = encode(&img).unwrap();
        let back = image::load_from_memory(&png).unwrap();
        assert_eq!((back.width(), back.height()), (640, 480));
        assert_eq!(back.to_rgba8().get_pixel(3, 3).0, [10, 20, 30, 255]);
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn saved_under_screenshots() {
        let dir = std::env::temp_dir().join(format!("th08_shot_test_{}", std::process::id()));
        let img = image::RgbaImage::new(4, 4);
        let p = save(&img, Some(&dir)).unwrap();
        assert!(p.starts_with(&dir.join("screenshots").display().to_string()));
        assert!(std::path::Path::new(&p).is_file());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(save(&img, None).is_err());
    }
}
