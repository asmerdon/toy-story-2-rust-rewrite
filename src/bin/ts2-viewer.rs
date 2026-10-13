//! Prototype viewer: renders a level `.ngn` from the user's install with a fly camera, and can
//! spawn any creature from that level and play its animations.
//!
//!   ts2-viewer <path\to\levelNN\level.ngn> [creature]
//!
//! Controls:
//!   right mouse + move   look          WASD          move
//!   Space / Ctrl         up / down     Shift         move faster
//!   P                    place creature in front of camera
//!   Tab                  next creature in this level
//!   [ / ]                previous / next animation        , / .  slower / faster
//!   C                    cycle vertex colour mode (white, bytes 1-3, bytes 3-1)
//!   T                    toggle black-is-transparent
//!   Esc                  quit

use glam::{Mat4, Vec3};
use std::sync::Arc;
use std::time::Instant;
use std::{env, fs, process};
use ts2::bmp;
use ts2::formats::{anim, ngn};
use ts2::scene::{self, Batch, GpuVertex};
use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// Game units per metre-ish; matches the OBJ exports.
const WORLD_SCALE: f32 = 1.0 / 256.0;
/// Animation playback rate in frames per second. The original rate is not confirmed yet.
const ANIM_FPS: f32 = 30.0;

const SHADER: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    // x: colour mode, y: alpha test on, z: world scale
    params: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VOut {
    var o: VOut;
    // Game space is +Y down; flip to +Y up.
    let p = vec3<f32>(pos.x, -pos.y, pos.z) * u.params.z;
    o.pos = u.view_proj * vec4<f32>(p, 1.0);
    o.uv = uv;
    o.color = color;
    return o;
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    if (u.params.y > 0.5 && t.a < 0.5) { discard; }
    var shade = vec3<f32>(1.0);
    let mode = u32(u.params.x);
    // PS1 convention: 0x80 = 1.0, so double the 0..1 value.
    if (mode == 1u) { shade = i.color.yzw * 2.0; }
    if (mode == 2u) { shade = i.color.wzy * 2.0; }
    return vec4<f32>(t.rgb * shade, 1.0);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    params: [f32; 4],
}

struct GpuBatch {
    texture: usize,
    buffer: wgpu::Buffer,
    count: u32,
}

struct Camera {
    pos: Vec3,
    yaw: f32,
    pitch: f32,
}

impl Camera {
    fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin() * self.pitch.cos(), self.pitch.sin(), -self.yaw.cos() * self.pitch.cos())
    }
    fn view(&self) -> Mat4 {
        Mat4::look_to_rh(self.pos, self.forward(), Vec3::Y)
    }
}

struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,
    /// One bind group per level texture, plus a white one at the end for untextured geometry.
    texture_bgs: Vec<wgpu::BindGroup>,
    depth: wgpu::TextureView,
    level: Vec<GpuBatch>,
    creature: Vec<GpuBatch>,
}

struct Viewer {
    ngn: ngn::Ngn,
    anims: Vec<Vec<anim::Animation>>,
    gfx: Option<Gfx>,
    cam: Camera,
    keys: std::collections::HashSet<KeyCode>,
    looking: bool,
    last: Instant,
    creature_idx: usize,
    creature_pos: Option<[f32; 3]>,
    anim_idx: usize,
    anim_time: f32,
    speed: f32,
    color_mode: u32,
    alpha_test: bool,
}

fn depth_view(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: config.width.max(1), height: config.height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn upload(device: &wgpu::Device, batches: &[Batch], white: usize) -> Vec<GpuBatch> {
    batches
        .iter()
        .filter(|b| !b.vertices.is_empty())
        .map(|b| GpuBatch {
            texture: b.texture.unwrap_or(white),
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&b.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            count: b.vertices.len() as u32,
        })
        .collect()
}

impl Gfx {
    fn new(window: Arc<Window>, ngn: &ngn::Ngn, level: &[Batch]) -> Gfx {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone()).expect("create surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("no suitable GPU adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).expect("request device");

        let size = window.inner_size();
        let mut config = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).expect("surface config");
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ts2"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&uniform_layout, &texture_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ts2"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Unorm8x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            // The original data has no consistent winding, so draw both sides.
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buf.as_entire_binding() }],
        });

        // Nearest filtering for the PS1 look.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let white = bmp::Image { width: 1, height: 1, rgba: vec![255; 4] };
        let images: Vec<bmp::Image> = ngn
            .textures
            .iter()
            .map(|t| bmp::decode(&t.bmp, true).unwrap_or_else(|| white.clone()))
            .chain(std::iter::once(white.clone()))
            .collect();
        let texture_bgs = images
            .iter()
            .map(|img| {
                let size = wgpu::Extent3d { width: img.width, height: img.height, depth_or_array_layers: 1 };
                let tex = device.create_texture_with_data(
                    &queue,
                    &wgpu::TextureDescriptor {
                        label: None,
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    &img.rgba,
                );
                let view = tex.create_view(&Default::default());
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &texture_layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                    ],
                })
            })
            .collect::<Vec<_>>();

        let depth = depth_view(&device, &config);
        let level = upload(&device, level, texture_bgs.len() - 1);
        Gfx { window, surface, device, queue, config, pipeline, uniform_buf, uniform_bg, texture_bgs, depth, level, creature: Vec::new() }
    }

    fn resize(&mut self, w: u32, h: u32) {
        self.config.width = w.max(1);
        self.config.height = h.max(1);
        self.surface.configure(&self.device, &self.config);
        self.depth = depth_view(&self.device, &self.config);
    }

    fn render(&mut self, uniforms: &Uniforms) {
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(e) => {
                eprintln!("surface error: {e}");
                return;
            }
        };
        self.queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(uniforms));
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.06, b: 0.1, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.uniform_bg, &[]);
            for b in self.level.iter().chain(&self.creature) {
                pass.set_bind_group(1, &self.texture_bgs[b.texture], &[]);
                pass.set_vertex_buffer(0, b.buffer.slice(..));
                pass.draw(0..b.count, 0..1);
            }
        }
        self.queue.submit([enc.finish()]);
        frame.present();
    }
}

impl Viewer {
    fn creature(&self) -> Option<&ngn::Creature> {
        self.ngn.creatures.get(self.creature_idx)
    }

    fn status(&self) {
        let Some(c) = self.creature() else { return };
        let anim = self.anims[self.creature_idx].get(self.anim_idx);
        println!(
            "creature {} (id {})  anim {}/{}  frames {}  colour mode {}  alpha test {}",
            c.name,
            c.id,
            self.anim_idx,
            self.anims[self.creature_idx].len(),
            anim.map_or(0, |a| a.frame_count),
            self.color_mode,
            self.alpha_test
        );
    }

    fn update(&mut self, dt: f32) {
        let mut v = Vec3::ZERO;
        let fwd = self.cam.forward();
        let right = fwd.cross(Vec3::Y).normalize_or_zero();
        for (k, d) in [
            (KeyCode::KeyW, fwd),
            (KeyCode::KeyS, -fwd),
            (KeyCode::KeyD, right),
            (KeyCode::KeyA, -right),
            (KeyCode::Space, Vec3::Y),
            (KeyCode::ControlLeft, -Vec3::Y),
        ] {
            if self.keys.contains(&k) {
                v += d;
            }
        }
        let speed = if self.keys.contains(&KeyCode::ShiftLeft) { 40.0 } else { 8.0 };
        self.cam.pos += v.normalize_or_zero() * speed * dt;

        self.anim_time += dt * ANIM_FPS * self.speed;
        let (Some(pos), Some(gfx)) = (self.creature_pos, self.gfx.as_mut()) else { return };
        let Some(c) = self.ngn.creatures.get(self.creature_idx) else { return };
        let anim = self.anims[self.creature_idx].get(self.anim_idx);
        let t = anim.map_or(0.0, |a| self.anim_time % (a.frame_count.max(2) - 1) as f32);
        let batches = scene::creature_batches(&self.ngn, c, pos, anim.map(|a| (a, t)));
        gfx.creature = upload(&gfx.device, &batches, gfx.texture_bgs.len() - 1);
    }

    fn place_creature(&mut self) {
        let p = self.cam.pos + self.cam.forward() * 4.0;
        // Back to game units and axes (+Y down).
        self.creature_pos = Some([p.x / WORLD_SCALE, -p.y / WORLD_SCALE, p.z / WORLD_SCALE]);
        self.status();
    }

    fn key(&mut self, code: KeyCode, el: &ActiveEventLoop) {
        let n_anims = self.anims.get(self.creature_idx).map_or(0, Vec::len).max(1);
        match code {
            KeyCode::Escape => el.exit(),
            KeyCode::KeyP => self.place_creature(),
            KeyCode::Tab => {
                self.creature_idx = (self.creature_idx + 1) % self.ngn.creatures.len().max(1);
                self.anim_idx = 0;
                self.status();
            }
            KeyCode::BracketRight => {
                self.anim_idx = (self.anim_idx + 1) % n_anims;
                self.anim_time = 0.0;
                self.status();
            }
            KeyCode::BracketLeft => {
                self.anim_idx = (self.anim_idx + n_anims - 1) % n_anims;
                self.anim_time = 0.0;
                self.status();
            }
            KeyCode::Period => self.speed = (self.speed * 2.0).min(8.0),
            KeyCode::Comma => self.speed = (self.speed / 2.0).max(0.125),
            KeyCode::KeyC => {
                self.color_mode = (self.color_mode + 1) % 3;
                self.status();
            }
            KeyCode::KeyT => {
                self.alpha_test = !self.alpha_test;
                self.status();
            }
            _ => {}
        }
    }
}

impl ApplicationHandler for Viewer {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let window = Arc::new(
            el.create_window(Window::default_attributes().with_title("Toy Story 2 (Rust) - viewer").with_inner_size(winit::dpi::LogicalSize::new(1280, 720)))
                .expect("create window"),
        );
        let level = scene::level_batches(&self.ngn);
        if let Some((lo, hi)) = scene::bounds(&level) {
            let c = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);
            self.cam.pos = Vec3::new(c[0], -c[1], c[2]) * WORLD_SCALE + Vec3::new(0.0, 2.0, 0.0);
        }
        self.gfx = Some(Gfx::new(window, &self.ngn, &level));
        self.last = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => {
                if let Some(g) = self.gfx.as_mut() {
                    g.resize(s.width, s.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if event.state == ElementState::Pressed {
                        if !event.repeat {
                            self.key(code, el);
                        }
                        self.keys.insert(code);
                    } else {
                        self.keys.remove(&code);
                    }
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Right, .. } => self.looking = state == ElementState::Pressed,
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last).as_secs_f32().min(0.1);
                self.last = now;
                self.update(dt);
                if let Some(g) = self.gfx.as_mut() {
                    let aspect = g.config.width as f32 / g.config.height as f32;
                    let proj = Mat4::perspective_rh(60f32.to_radians(), aspect, 0.05, 2000.0);
                    let u = Uniforms {
                        view_proj: (proj * self.cam.view()).to_cols_array_2d(),
                        params: [self.color_mode as f32, if self.alpha_test { 1.0 } else { 0.0 }, WORLD_SCALE, 0.0],
                    };
                    g.render(&u);
                    g.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: winit::event::DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event
            && self.looking
        {
            self.cam.yaw += dx as f32 * 0.003;
            self.cam.pitch = (self.cam.pitch - dy as f32 * 0.003).clamp(-1.5, 1.5);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: ts2-viewer <path\\to\\levelNN\\level.ngn> [creature]");
        process::exit(2);
    };
    let bytes = fs::read(path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        process::exit(1);
    });
    let ngn = ngn::parse(&bytes).unwrap_or_else(|e| {
        eprintln!("cannot parse {path}: {e}");
        process::exit(1);
    });
    let anims: Vec<Vec<anim::Animation>> =
        ngn.creatures.iter().map(|c| c.animations.iter().filter_map(|a| anim::parse(a).ok()).collect()).collect();
    let creature_idx = args
        .get(2)
        .and_then(|name| ngn.creatures.iter().position(|c| c.name.eq_ignore_ascii_case(name)))
        .unwrap_or(0);
    println!("{} creatures: {:?}", ngn.creatures.len(), ngn.creatures.iter().map(|c| &c.name).collect::<Vec<_>>());
    println!("right-drag look, WASD move, P place creature, Tab next creature, [ ] animation, C colour mode, T transparency");

    let mut viewer = Viewer {
        ngn,
        anims,
        gfx: None,
        cam: Camera { pos: Vec3::ZERO, yaw: 0.0, pitch: 0.0 },
        keys: Default::default(),
        looking: false,
        last: Instant::now(),
        creature_idx,
        creature_pos: None,
        anim_idx: 0,
        anim_time: 0.0,
        speed: 1.0,
        color_mode: 1,
        alpha_test: true,
    };
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.run_app(&mut viewer).expect("run");
}
