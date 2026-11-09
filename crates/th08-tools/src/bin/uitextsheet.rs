//! Blind comparison sheets of the same UI strings drawn by the Scene renderer, Slint and egui.
use image::{Rgba, RgbaImage, imageops};
use th08::slintui::check::{SheetText, sheet_egui, sheet_slint};
use th08_render::gfx::{Gfx, OverlayTarget};
use th08_render::scene_draw::SceneRenderer;
use th08_ui::scene::{Color, Face, Ring, Scene, Text};
use th08_ui::text::{Measure, Shaper};

const STRINGS: [&str; 9] = [
    "Master volume",
    "Resume game",
    "Reset this page…",
    "Громкость музыки",
    "Вернуться в игру",
    "Сбросить настройки…",
    "音量の設定",
    "タイトルに戻る",
    "ゲーム本来のBGM音量はそのままです。",
];
const COLORS: [[u8; 3]; 3] = [[0xfc, 0xfc, 0xfc], [0xc4, 0xbc, 0xcb], [0x9a, 0x93, 0xa6]];
const LOGICAL_W: f32 = 1240.0;
const COLUMN: f32 = 620.0;

struct Variant {
    name: &'static str,
    scale: f32,
    sizes: &'static [f32],
}

const VARIANTS: [Variant; 3] = [
    Variant {
        name: "phone_1080",
        scale: 1.0,
        sizes: &[30.0, 28.0, 24.0, 19.0],
    },
    Variant {
        name: "phone_720",
        scale: 2.0 / 3.0,
        sizes: &[30.0, 28.0, 24.0, 19.0],
    },
    Variant {
        name: "desktop",
        scale: 1.0,
        sizes: &[24.0, 19.0, 16.0],
    },
];

#[expect(clippy::cast_sign_loss, reason = "sheet sizes are positive")]
fn px(v: f32) -> u32 {
    v as u32
}

fn lines(v: &Variant) -> (Vec<SheetText>, f32) {
    let mut out = Vec::new();
    let mut cy = 8.0;
    for &size in v.sizes {
        for (i, s) in STRINGS.iter().enumerate() {
            cy += size * 0.8;
            for x in [16.0, COLUMN + 16.0] {
                out.push(SheetText {
                    text: th08_ui::wrap::latin_dots(s).into_owned(),
                    x,
                    cy,
                    size,
                    serif: true,
                    color: COLORS[i % 3],
                });
            }
            cy += size * 0.8;
        }
        cy += 12.0;
    }
    (out, cy + 8.0)
}

// Left half: the menu panel. Right half: bright game-like stripes, where the ring has to hold the text.
fn backdrop(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        if x < w / 2 {
            return Rgba([0x17, 0x0c, 0x0d, 255]);
        }
        let band = (x + y) / 9 % 7;
        let v = [40, 230, 90, 180, 250, 60, 140][band as usize];
        Rgba([v, (v / 2 + y as u8 / 4), 255 - v / 2, 255])
    })
}

fn scene_image(gfx: &Gfx, frame: &RgbaImage, list: &[SheetText], scale: f32) -> RgbaImage {
    let mut r = SceneRenderer::new(&gfx.device, gfx.surface_format());
    let shaper = Shaper::new();
    let mut s = Scene::new(scale);
    for l in list {
        let m = shaper.line(Face::Serif, l.size);
        let [cr, cg, cb] = l.color;
        s.text(Text {
            x: l.x,
            baseline: l.cy + (m.ascent - m.descent) / 2.0,
            text: l.text.clone(),
            face: Face::Serif,
            size: l.size,
            color: Color::rgb(cr, cg, cb),
            ring: Some(Ring::BLACK_1PX),
        });
    }
    with_target(gfx, frame, |t, enc| r.draw(&s, t, enc))
}

fn with_target(
    gfx: &Gfx,
    frame: &RgbaImage,
    paint: impl FnOnce(&OverlayTarget<'_>, &mut wgpu::CommandEncoder),
) -> RgbaImage {
    let (w, h) = frame.dimensions();
    let tex = gfx.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gfx.surface_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut px = frame.clone().into_raw();
    px.as_chunks_mut::<4>()
        .0
        .iter_mut()
        .for_each(|p| p.swap(0, 2));
    gfx.queue.write_texture(
        tex.as_image_copy(),
        &px,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        tex.size(),
    );
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let t = OverlayTarget {
        device: &gfx.device,
        queue: &gfx.queue,
        view: &view,
        format: gfx.surface_format(),
        size: (w, h),
        frame: gfx.frame_rect(w, h),
    };
    let mut enc = gfx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    paint(&t, &mut enc);
    gfx.read_texture(enc, &tex)
}

fn stack(panels: &[&RgbaImage]) -> RgbaImage {
    let (w, h) = panels[0].dimensions();
    let n = panels.len() as u32;
    let mut out = RgbaImage::from_pixel(w, h * n + 6 * (n - 1), Rgba([255, 255, 255, 255]));
    for (i, p) in panels.iter().enumerate() {
        imageops::replace(&mut out, *p, 0, i64::from((h + 6) * i as u32));
    }
    out
}

fn main() -> anyhow::Result<()> {
    let dir = std::env::args().nth(1).ok_or_else(|| {
        anyhow::anyhow!("usage: uitextsheet OUT_DIR — writes sheet_*.png, zoom_*.png and KEY.txt")
    })?;
    std::fs::create_dir_all(&dir)?;
    let gfx = pollster::block_on(Gfx::headless())?;
    let mut key = String::new();
    let mut order = 0usize;
    for v in &VARIANTS {
        let (list, logical_h) = lines(v);
        let (w, h) = (px(LOGICAL_W * v.scale), px(logical_h * v.scale));
        let frame = backdrop(w, h);
        let own = scene_image(&gfx, &frame, &list, v.scale);
        let slint = sheet_slint(&gfx, &frame, &list, v.scale)?;
        let egui = sheet_egui(&gfx, &frame, &list, v.scale);
        // A fixed rotation per variant: the owner judges without knowing which is which.
        let mut named = [("scene", &own), ("slint", &slint), ("egui", &egui)];
        named.rotate_left(order % 3);
        order += 2;
        stack(&named.map(|n| n.1)).save(format!("{dir}/sheet_{}.png", v.name))?;
        let crop = |img: &RgbaImage| {
            let cw = (w / 2).min(520);
            let ch = px((v.sizes[v.sizes.len() - 1] * 1.6 * 9.0 + 12.0) * v.scale);
            let y = h.saturating_sub(ch + px(8.0 * v.scale));
            let c = imageops::crop_imm(img, w / 2, y, cw, ch).to_image();
            imageops::resize(&c, cw * 3, ch * 3, imageops::FilterType::Nearest)
        };
        let zoomed = named.map(|n| crop(n.1));
        stack(&[&zoomed[0], &zoomed[1], &zoomed[2]]).save(format!("{dir}/zoom_{}.png", v.name))?;
        key += &format!(
            "{}: top={} middle={} bottom={}\n",
            v.name, named[0].0, named[1].0, named[2].0
        );
    }
    std::fs::write(format!("{dir}/KEY.txt"), key)?;
    Ok(())
}
