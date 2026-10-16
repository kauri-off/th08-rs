// Dumps every texture of an ANM file to PNG, with sprite rectangles outlined.
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ar = th08::archive::Archive::from_bytes(std::fs::read(&a[1])?)?;
    let out = std::path::Path::new(&a[3]);
    std::fs::create_dir_all(out)?;
    for (i, c) in th08::anm::parse(&ar.read(&a[2])?)?.iter().enumerate() {
        let Some(t) = &c.texture else { continue };
        let mut img = image::RgbaImage::from_raw(t.width, t.height, t.rgba.clone()).unwrap();
        for s in &c.sprites {
            let (x0, y0) = (s.x as u32, s.y as u32);
            let (x1, y1) = ((s.x + s.w) as u32, (s.y + s.h) as u32);
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
        println!("{} {}x{} fmt{} sprites={} scripts={} name={}", p.display(), t.width, t.height, t.format, c.sprites.len(), c.scripts.len(), c.name);
    }
    Ok(())
}
