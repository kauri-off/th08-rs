//! The only place the scene renderer makes GPU objects: every creation is counted, including ones dropped at once.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Made {
    pub pipelines: u32,
    pub shaders: u32,
    pub textures: u32,
    pub bind_groups: u32,
    pub buffers: u32,
}

pub struct Factory {
    pub device: wgpu::Device,
    pub made: Made,
}

impl Factory {
    pub fn shader(&mut self, desc: wgpu::ShaderModuleDescriptor<'_>) -> wgpu::ShaderModule {
        self.made.shaders += 1;
        self.device.create_shader_module(desc)
    }

    pub fn pipeline(&mut self, desc: &wgpu::RenderPipelineDescriptor<'_>) -> wgpu::RenderPipeline {
        self.made.pipelines += 1;
        self.device.create_render_pipeline(desc)
    }

    pub fn texture(&mut self, desc: &wgpu::TextureDescriptor<'_>) -> wgpu::Texture {
        self.made.textures += 1;
        self.device.create_texture(desc)
    }

    pub fn bind_group(&mut self, desc: &wgpu::BindGroupDescriptor<'_>) -> wgpu::BindGroup {
        self.made.bind_groups += 1;
        self.device.create_bind_group(desc)
    }

    pub fn buffer(&mut self, desc: &wgpu::BufferDescriptor<'_>) -> wgpu::Buffer {
        self.made.buffers += 1;
        self.device.create_buffer(desc)
    }
}

#[cfg(test)]
mod tests {
    use super::Made;
    use crate::gfx::Gfx;
    use th08_ui::scene::{Arrow, Color, Dir, Face, Rect, Ring, Scene, Text};

    fn gate(before: Made, after: Made, atlas_growths: u32) -> Result<(), String> {
        let (p, s, t) = (
            after.pipelines - before.pipelines,
            after.shaders - before.shaders,
            after.textures - before.textures,
        );
        if p == 0 && s == 0 && t == atlas_growths {
            Ok(())
        } else {
            Err(format!("{p} pipelines, {s} shaders, {t} textures"))
        }
    }

    const CREATIONS: [&str; 5] = [
        ".create_render_pipeline(",
        ".create_shader_module(",
        ".create_texture(",
        ".create_bind_group(",
        ".create_buffer(",
    ];

    fn bypasses_factory(source: &str) -> Vec<&'static str> {
        CREATIONS
            .into_iter()
            .filter(|c| source.contains(c))
            .collect()
    }

    fn live(gfx: &Gfx) -> Option<[usize; 3]> {
        let r = gfx.instance().generate_report()?;
        Some([
            r.hub.render_pipelines.num_allocated,
            r.hub.shader_modules.num_allocated,
            r.hub.textures.num_allocated,
        ])
    }

    fn scene(n: usize) -> Scene {
        let lang = ["Master volume", "Громкость музыки", "ゲーム本来のBGM音量"];
        let mut s = Scene::new([1.0, 0.67, 1.5][n % 3]);
        s.rect(Rect::new(0.0, 0.0, 400.0, 300.0), Color([23, 12, 13, 200]));
        s.clipped(Rect::new(5.0, 5.0, 300.0 + n as f32, 200.0), |s| {
            for i in 0..8 {
                s.text(Text {
                    x: 10.0 + n as f32 / 8.0,
                    baseline: 30.0 + 30.0 * i as f32,
                    text: format!("{} {}", lang[(n + i) % 3], n * i),
                    face: if i % 2 == 0 { Face::Serif } else { Face::Sans },
                    size: 19.0 + (n % 5) as f32,
                    color: Color::WHITE,
                    ring: Some(Ring::BLACK_1PX),
                });
            }
            s.arrow(Arrow {
                x: 300.0,
                y: 10.0,
                size: 14.0,
                dir: Dir::Right,
                color: Color::WHITE,
                ring: None,
            });
        });
        s
    }

    fn target(gfx: &Gfx) -> wgpu::Texture {
        gfx.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 640,
                height: 480,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: gfx.surface_format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    }

    fn frame(gfx: &mut Gfx, view: &wgpu::TextureView, s: &Scene) {
        let mut enc = gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        gfx.draw_scene(s, view, (640, 480), &mut enc);
        gfx.queue.submit([enc.finish()]);
    }

    fn adapter() -> Option<Gfx> {
        let g = pollster::block_on(Gfx::headless()).ok();
        if g.is_none() {
            eprintln!("no GPU adapter: skipped");
        }
        g
    }

    fn run(gfx: &mut Gfx, frames: usize) -> (Made, Made, Option<[usize; 3]>, Option<[usize; 3]>) {
        let t = target(gfx);
        let view = t.create_view(&wgpu::TextureViewDescriptor::default());
        frame(gfx, &view, &scene(0));
        let before = gfx.scene_renderer().expect("renderer").made();
        let live0 = live(gfx);
        for n in 1..frames {
            frame(gfx, &view, &scene(n));
        }
        (
            before,
            gfx.scene_renderer().expect("renderer").made(),
            live0,
            live(gfx),
        )
    }

    #[test]
    fn changing_scenes_make_no_gpu_objects() {
        let Some(mut gfx) = adapter() else { return };
        let made_at_creation = gfx.scene_renderer().expect("renderer").made();
        assert_eq!(
            (made_at_creation.pipelines, made_at_creation.shaders),
            (2, 1)
        );
        let (before, after, l0, l1) = run(&mut gfx, 300);
        assert_eq!(gate(before, after, 0), Ok(()));
        assert_eq!(before, after, "buffers and bind groups too");
        assert_eq!(l0, l1, "live objects in wgpu's own report");
    }

    #[test]
    fn the_gate_fails_on_a_pipeline_per_frame() {
        let Some(mut gfx) = adapter() else { return };
        gfx.scene_renderer()
            .expect("renderer")
            .waste_a_pipeline_each_frame = true;
        let (before, after, _, _) = run(&mut gfx, 10);
        assert_eq!(
            gate(before, after, 0),
            Err("9 pipelines, 0 shaders, 0 textures".to_string())
        );
    }

    #[test]
    fn atlas_growth_is_one_texture() {
        let Some(mut gfx) = adapter() else { return };
        let t = target(&gfx);
        let view = t.create_view(&wgpu::TextureViewDescriptor::default());
        frame(&mut gfx, &view, &scene(0));
        let before = gfx.scene_renderer().expect("renderer").made();
        let f = th08_ui::text::font(Face::Serif);
        let map = f.charmap();
        let all: Vec<char> = (0x21..0x9fffu32)
            .filter_map(char::from_u32)
            .filter(|&c| map.map(c) != 0)
            .collect();
        let mut s = Scene::new(1.0);
        for (i, line) in all.chunks(25).enumerate() {
            s.text(Text {
                x: 4.0,
                baseline: 80.0 + i as f32,
                text: line.iter().collect(),
                face: Face::Serif,
                size: 72.0,
                color: Color::WHITE,
                ring: Some(Ring::BLACK_1PX),
            });
        }
        frame(&mut gfx, &view, &s);
        let r = gfx.scene_renderer().expect("renderer");
        assert_eq!(r.atlas_side(), 4096);
        assert_eq!(gate(before, r.made(), 1), Ok(()));
    }

    #[test]
    fn every_creation_in_the_renderer_goes_through_the_factory() {
        let source = include_str!("scene_draw.rs");
        assert_eq!(bypasses_factory(source), Vec::<&str>::new());
        let doctored =
            format!("{source}\nfn x(d: &wgpu::Device) {{ d.create_render_pipeline(&todo!()); }}");
        assert_eq!(bypasses_factory(&doctored), [".create_render_pipeline("]);
    }
}
