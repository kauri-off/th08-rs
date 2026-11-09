//! Draws a port UI Scene in the game's pass: a solid and a glyph pipeline, and one atlas texture.
use crate::gfx::OverlayTarget;
use crate::scene_gpu::{Factory, Made};
use th08_ui::prepare::{Pipe, Prepare, Vertex};
use th08_ui::scene::Scene;

const SHADER: &str = "
@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var near: sampler;
struct V { @builtin(position) p: vec4f, @location(0) uv: vec2f, @location(1) c: vec4f }
@vertex fn vs(@location(0) p: vec2f, @location(1) uv: vec2f, @location(2) c: vec4f) -> V {
  return V(vec4f(p, 0.0, 1.0), uv, c);
}
@fragment fn solid(v: V) -> @location(0) vec4f {
  return vec4f(v.c.rgb * v.c.a, v.c.a);
}
@fragment fn glyph(v: V) -> @location(0) vec4f {
  let a = v.c.a * textureSampleLevel(atlas, near, v.uv, 0.0).r;
  return vec4f(v.c.rgb * a, a);
}
";

const VERTS_START: u64 = 1 << 16;

fn vertex_layout() -> [wgpu::VertexAttribute; 3] {
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Unorm8x4]
}

struct AtlasGpu {
    texture: wgpu::Texture,
    bind: wgpu::BindGroup,
}

pub struct SceneRenderer {
    factory: Factory,
    solid: wgpu::RenderPipeline,
    glyph: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    atlas: AtlasGpu,
    verts: wgpu::Buffer,
    prep: Prepare,
    #[cfg(test)]
    pub waste_a_pipeline_each_frame: bool,
    #[cfg(test)]
    format: wgpu::TextureFormat,
    #[cfg(test)]
    shader: wgpu::ShaderModule,
    #[cfg(test)]
    empty_layout: wgpu::PipelineLayout,
}

fn make_pipeline(
    f: &mut Factory,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    entry: &str,
) -> wgpu::RenderPipeline {
    let attrs = vertex_layout();
    f.pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("scene"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attrs,
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn make_atlas(
    f: &mut Factory,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    side: u32,
) -> AtlasGpu {
    let texture = f.texture(&wgpu::TextureDescriptor {
        label: Some("scene atlas"),
        size: wgpu::Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind = f.bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scene atlas"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    AtlasGpu { texture, bind }
}

fn make_verts(f: &mut Factory, count: u64) -> wgpu::Buffer {
    f.buffer(&wgpu::BufferDescriptor {
        label: Some("scene vertices"),
        size: count * size_of::<Vertex>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl SceneRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let mut f = Factory {
            device: device.clone(),
            made: Made::default(),
        };
        let shader = f.shader(wgpu::ShaderModuleDescriptor {
            label: Some("scene"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene atlas"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
        let glyph_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene glyph"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let empty_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene solid"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let solid = make_pipeline(&mut f, &empty_layout, &shader, format, "solid");
        let glyph = make_pipeline(&mut f, &glyph_layout, &shader, format, "glyph");
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        let prep = Prepare::new(device.limits().max_texture_dimension_2d);
        let atlas = make_atlas(&mut f, &layout, &sampler, prep.atlas.size());
        let verts = make_verts(&mut f, VERTS_START);
        Self {
            factory: f,
            solid,
            glyph,
            layout,
            sampler,
            atlas,
            verts,
            prep,
            #[cfg(test)]
            waste_a_pipeline_each_frame: false,
            #[cfg(test)]
            format,
            #[cfg(test)]
            shader,
            #[cfg(test)]
            empty_layout,
        }
    }

    // GPU objects made so far, dropped ones included.
    pub fn made(&self) -> Made {
        self.factory.made
    }

    pub fn atlas_side(&self) -> u32 {
        self.prep.atlas.size()
    }

    pub fn dropped_glyphs(&self) -> u32 {
        self.prep.dropped
    }

    // The shaper a layout measures with; the widths match what is drawn.
    pub fn measure(&self) -> &th08_ui::text::Shaper {
        &self.prep.shaper
    }

    fn upload(&mut self, queue: &wgpu::Queue) {
        if self.atlas.texture.width() != self.prep.atlas.size() {
            let side = self.prep.atlas.size();
            self.atlas = make_atlas(&mut self.factory, &self.layout, &self.sampler, side);
        }
        for u in self.prep.atlas.uploads.drain(..) {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: u.x,
                        y: u.y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &u.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(u.w),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: u.w,
                    height: u.h,
                    depth_or_array_layers: 1,
                },
            );
        }
        let need = self.prep.verts.len() as u64;
        if need * size_of::<Vertex>() as u64 > self.verts.size() {
            self.verts = make_verts(&mut self.factory, need.next_power_of_two());
        }
        queue.write_buffer(&self.verts, 0, bytemuck::cast_slice(&self.prep.verts));
    }

    pub fn draw(&mut self, scene: &Scene, t: &OverlayTarget<'_>, enc: &mut wgpu::CommandEncoder) {
        self.prep.run(scene, t.size);
        self.upload(t.queue);
        #[cfg(test)]
        if self.waste_a_pipeline_each_frame {
            make_pipeline(
                &mut self.factory,
                &self.empty_layout,
                &self.shader,
                self.format,
                "solid",
            );
        }
        if self.prep.batches.is_empty() {
            return;
        }
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: t.view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_vertex_buffer(0, self.verts.slice(..));
        pass.set_bind_group(0, &self.atlas.bind, &[]);
        let mut bound = None;
        for b in &self.prep.batches {
            if bound != Some(b.pipe) {
                pass.set_pipeline(match b.pipe {
                    Pipe::Solid => &self.solid,
                    Pipe::Glyph => &self.glyph,
                });
                bound = Some(b.pipe);
            }
            let [x, y, w, h] = b.scissor;
            pass.set_scissor_rect(x, y, w, h);
            pass.draw(b.first..b.first + b.count, 0..1);
        }
    }
}
