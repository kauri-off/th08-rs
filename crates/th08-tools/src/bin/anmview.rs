//! Dumps every texture of an ANM file to PNG with sprite rects outlined (`--raw`: none); `--dis` prints the scripts.

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ar = th08_formats::archive::Archive::from_bytes(std::fs::read(&a[1])?)?;
    if a[3] == "--dis" {
        let (mut spr, mut scr) = (0, 0);
        for (i, c) in th08_formats::anm::parse(&ar.read(&a[2])?)?
            .iter()
            .enumerate()
        {
            println!(
                "entry {i} {} {}x{} sprites {spr}..{} scripts {scr}..{}",
                c.name,
                c.width,
                c.height,
                spr + c.sprites.len(),
                scr + c.scripts.len()
            );
            for s in &c.sprites {
                println!("  sprite {} ({}, {}) {}x{}", s.id, s.x, s.y, s.w, s.h);
            }
            for s in &c.scripts {
                println!(
                    "script {scr} (id {})\n{}",
                    s.id,
                    th08_core::anmvm::disasm(&s.data)
                );
                scr += 1;
            }
            spr += c.sprites.len();
        }
        return Ok(());
    }
    let out = std::path::Path::new(&a[3]);
    std::fs::create_dir_all(out)?;
    let b = ar.read(&a[2])?;
    let texs = th08_formats::anm::textures(&b)?;
    for (i, (c, t)) in th08_formats::anm::parse(&b)?.iter().zip(&texs).enumerate() {
        let Some(t) = t else { continue };
        let mut img = image::RgbaImage::from_raw(t.width, t.height, t.rgba.clone()).unwrap();
        for s in c
            .sprites
            .iter()
            .filter(|_| a.get(4).map(String::as_str) != Some("--raw"))
        {
            let (x0, y0) = (px(s.x), px(s.y));
            let (x1, y1) = (px(s.x + s.w), px(s.y + s.h));
            for x in x0..x1.min(t.width) {
                for y in [y0, y1.saturating_sub(1)] {
                    if y < t.height {
                        img.put_pixel(x, y, image::Rgba([255, 0, 255, 255]));
                    }
                }
            }
        }
        let p = out.join(format!("{}_{i}_fmt{}.png", a[2], t.format));
        img.save(&p)?;
        println!(
            "{} {}x{} fmt{} sprites={} scripts={} name={}",
            p.display(),
            t.width,
            t.height,
            t.format,
            c.sprites.len(),
            c.scripts.len(),
            c.name
        );
    }
    Ok(())
}

// A sprite-rect edge in texture pixels (≥ 0).
#[expect(clippy::cast_sign_loss, reason = "sprite rects lie inside the texture")]
fn px(v: f32) -> u32 {
    v as u32
}
