//! Prototype viewer: renders a level `.ngn` from the user's install with a fly camera, and can
//! spawn any creature from that level and play its animations.
//!
//!   ts2-viewer <path\to\levelNN\level.ngn> [creature]
//!
//! Controls:
//!   right mouse + move   look          WASD          move
//!   Space / Ctrl         up / down     Shift         move faster
//!   N                    show / hide creatures at their spawn points (from level*.raw)
//!   P                    place creature in front of camera
//!   F                    play as Buzz (drops him below the camera) / back to free camera
//!     in play mode: WASD run, Space jump, right-drag orbit camera,
//!     [ ] browse Buzz's animations, 1/2/3 assign to idle/run/jump, R turn model 90°
//!   Tab                  next creature in this level
//!   [ / ]                previous / next animation        , / .  slower / faster
//!   C                    cycle vertex colour mode (white, bytes 1-3, bytes 3-1)
//!   T                    toggle transparency (green key colour)
//!   Esc                  quit

use glam::{Mat4, Vec3};
use std::sync::Arc;
use std::time::Instant;
use std::{env, fs, process};
use ts2::bmp;
use ts2::formats::{anim, exe, ngn, packets};
use ts2::game;
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
    // Game space is +Y down, +Z forward (right-handed). Negating Y and Z together is a rotation,
    // so the level is not mirrored.
    let p = vec3<f32>(pos.x, -pos.y, -pos.z) * u.params.z;
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

/// Game space (units, +Y down, +Z forward) to viewer world space (scaled, +Y up).
fn game_to_world(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], -p[1], -p[2]) * WORLD_SCALE
}

fn world_to_game(p: Vec3) -> [f32; 3] {
    [p.x / WORLD_SCALE, -p.y / WORLD_SCALE, -p.z / WORLD_SCALE]
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
    /// (index into ngn.creatures, position) for each spawn whose creature is in this level.
    spawns: Vec<(usize, [f32; 3])>,
    show_spawns: bool,
    /// Current animation per creature (index into `anims[creature]`).
    creature_anim: Vec<usize>,
    /// Level geometry, handed to the GPU on first resume.
    level: Vec<Batch>,
    world: game::World,
    player: Option<game::Player>,
    /// Index of BUZZ (id 0) in `ngn.creatures`, if present.
    player_creature: Option<usize>,
    /// Original player start for this level, from the user's toy2.exe.
    start: Option<exe::PlayerStart>,
    /// Animation indices for idle, run and jump. Placeholders until the real ones are identified.
    player_anims: [usize; 3],
    player_yaw_offset: f32,
    jump_pressed: bool,
    /// Animation browsed with [ ] in play mode, shown on Buzz until assigned with 1/2/3.
    anim_preview: Option<usize>,
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
            .map(|t| bmp::decode(&t.bmp, Some(bmp::PC_KEY)).unwrap_or_else(|| white.clone()))
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

    fn held(&self, k: KeyCode) -> bool {
        self.keys.contains(&k)
    }

    fn update(&mut self, dt: f32) {
        let fwd = self.cam.forward();
        let right = fwd.cross(Vec3::Y).normalize_or_zero();
        if let Some(player) = self.player.as_mut() {
            // Camera-relative movement on the ground plane, converted to game space (x, -z).
            let flat_fwd = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
            let mut m = Vec3::ZERO;
            for (k, d) in [(KeyCode::KeyW, flat_fwd), (KeyCode::KeyS, -flat_fwd), (KeyCode::KeyD, right), (KeyCode::KeyA, -right)] {
                if self.keys.contains(&k) {
                    m += d;
                }
            }
            let m = m.normalize_or_zero();
            let input = game::Input { move_xz: [m.x, -m.z], jump: std::mem::take(&mut self.jump_pressed) };
            player.update(&self.world, input, dt);
            // Follow camera: orbit behind the player at chest height.
            let target = game_to_world(player.pos.to_array()) + Vec3::Y * 1.2;
            self.cam.pos = target - fwd * 4.5;
        } else {
            let mut v = Vec3::ZERO;
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
            let speed = if self.held(KeyCode::ShiftLeft) { 40.0 } else { 8.0 };
            self.cam.pos += v.normalize_or_zero() * speed * dt;
        }

        self.anim_time += dt * ANIM_FPS * self.speed;
        if let Some(a) = self.creature_anim.get_mut(self.creature_idx) {
            *a = self.anim_idx;
        }
        let Some(gfx) = self.gfx.as_mut() else { return };
        // (creature index, position, yaw, animation)
        let mut placed: Vec<(usize, [f32; 3], f32, usize)> =
            self.creature_pos.map(|p| (self.creature_idx, p, 0.0, self.creature_anim[self.creature_idx])).into_iter().collect();
        if self.show_spawns {
            placed.extend(self.spawns.iter().map(|&(ci, p)| (ci, p, 0.0, self.creature_anim[ci])));
        }
        if let (Some(p), Some(ci)) = (&self.player, self.player_creature) {
            let anim = match (self.anim_preview, p.motion()) {
                (Some(a), _) => a,
                (None, game::Motion::Idle) => self.player_anims[0],
                (None, game::Motion::Run) => self.player_anims[1],
                (None, game::Motion::Air) => self.player_anims[2],
            };
            placed.push((ci, p.pos.to_array(), p.yaw + self.player_yaw_offset, anim));
        }
        let mut batches = Vec::new();
        for (ci, pos, yaw, ai) in placed {
            let c = &self.ngn.creatures[ci];
            let anim = self.anims[ci].get(ai);
            let t = anim.map_or(0.0, |a| self.anim_time % (a.frame_count.max(2) - 1) as f32);
            scene::merge(&mut batches, scene::creature_batches(&self.ngn, c, pos, yaw, anim.map(|a| (a, t))));
        }
        gfx.creature = upload(&gfx.device, &batches, gfx.texture_bgs.len() - 1);
    }

    fn place_creature(&mut self) {
        let p = self.cam.pos + self.cam.forward() * 4.0;
        self.creature_pos = Some(world_to_game(p));
        self.status();
    }

    /// F: put Buzz at the level's original start (or, with Shift or no start known, onto the
    /// floor below the camera). F again returns to the free camera.
    fn toggle_play(&mut self) {
        if self.player.take().is_some() {
            println!("free camera");
            return;
        }
        let Some(ci) = self.player_creature else {
            println!("this level has no BUZZ creature to play");
            return;
        };
        let (g, yaw) = match self.start {
            Some(s) if !self.held(KeyCode::ShiftLeft) => (Vec3::from(s.pos), s.yaw_radians()),
            _ => (Vec3::from(world_to_game(self.cam.pos + self.cam.forward() * 3.0)), 0.0),
        };
        // The game snaps the start height to the floor; search a little above the stored point.
        let feet = self.world.floor_below(g.x, g.z, g.y - 600.0).map_or(g, |y| Vec3::new(g.x, y, g.z));
        let mut player = game::Player::new(feet);
        player.yaw = yaw;
        self.player = Some(player);
        self.creature_idx = ci;
        self.anim_idx = self.player_anims[0];
        println!(
            "play mode: WASD run, Space jump, right-drag camera, F to exit. Animations idle/run/jump = {:?}; \
             browse with [ ] and press 1/2/3 to assign, R turns the model 90 degrees",
            self.player_anims
        );
    }

    fn key(&mut self, code: KeyCode, el: &ActiveEventLoop) {
        let n_anims = self.anims.get(self.creature_idx).map_or(0, Vec::len).max(1);
        match code {
            KeyCode::Escape => el.exit(),
            KeyCode::KeyP => self.place_creature(),
            KeyCode::KeyF => self.toggle_play(),
            KeyCode::Space if self.player.is_some() => self.jump_pressed = true,
            KeyCode::Digit1 | KeyCode::Digit2 | KeyCode::Digit3 if self.player.is_some() => {
                let slot = match code {
                    KeyCode::Digit1 => 0,
                    KeyCode::Digit2 => 1,
                    _ => 2,
                };
                self.player_anims[slot] = self.anim_idx;
                self.anim_preview = None;
                println!("player animations idle/run/jump = {:?}", self.player_anims);
            }
            KeyCode::KeyR if self.player.is_some() => {
                self.player_yaw_offset = (self.player_yaw_offset + std::f32::consts::FRAC_PI_2) % std::f32::consts::TAU;
                println!("player model turned to {:.0} degrees", self.player_yaw_offset.to_degrees());
            }
            KeyCode::KeyN => {
                self.show_spawns = !self.show_spawns;
                println!("spawns {}", if self.show_spawns { "shown" } else { "hidden" });
            }
            KeyCode::Tab => {
                self.creature_idx = (self.creature_idx + 1) % self.ngn.creatures.len().max(1);
                self.anim_idx = self.creature_anim.get(self.creature_idx).copied().unwrap_or(0);
                self.status();
            }
            KeyCode::BracketRight | KeyCode::BracketLeft => {
                let step = if code == KeyCode::BracketRight { 1 } else { n_anims - 1 };
                self.anim_idx = (self.anim_idx + step) % n_anims;
                self.anim_time = 0.0;
                if self.player.is_some() {
                    self.anim_preview = Some(self.anim_idx);
                }
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
        let level = std::mem::take(&mut self.level);
        if let Some(s) = self.start {
            // Start behind and above the original player start.
            self.cam.pos = game_to_world(s.pos) + Vec3::new(0.0, 1.5, 5.0);
        } else if let Some(&(_, p)) = self.spawns.first() {
            // Start just behind and above the first spawn.
            self.cam.pos = Vec3::new(p[0], -p[1], -p[2]) * WORLD_SCALE + Vec3::new(0.0, 1.5, 5.0);
        } else if let Some((lo, hi)) = scene::bounds(&level) {
            let c = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);
            self.cam.pos = Vec3::new(c[0], -c[1], -c[2]) * WORLD_SCALE + Vec3::new(0.0, 2.0, 0.0);
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

    // Spawns come from the packet file with the same stem (level.ngn -> level.raw).
    let raw_path = std::path::Path::new(path).with_extension("raw");
    let spawns: Vec<(usize, [f32; 3])> = match fs::read(&raw_path).map_err(|e| e.to_string()).and_then(|b| packets::parse(&b).map_err(|e| e.to_string())) {
        Ok(pk) => {
            let all: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
            let found: Vec<_> = all
                .iter()
                .filter_map(|s| ngn.creatures.iter().position(|c| c.id == s.creature as usize).map(|ci| (ci, s.pos.map(|v| v as f32))))
                .collect();
            println!("{} spawns from {} ({} placeable)", all.len(), raw_path.display(), found.len());
            found
        }
        Err(e) => {
            println!("no spawns ({}: {e})", raw_path.display());
            Vec::new()
        }
    };
    println!("right-drag look, WASD move, N spawns, P place creature, Tab next creature, [ ] animation, C colour mode, T transparency");

    let n_creatures = ngn.creatures.len();
    let level = scene::level_batches(&ngn);
    let world = game::World::from_batches(&level);
    let player_creature = ngn.creatures.iter().position(|c| c.id == 0);

    // Player start: table in the user's toy2.exe (game dir = two levels above levelNN\).
    let level_path = std::path::Path::new(path);
    let start = level_path
        .ancestors()
        .nth(3)
        .map(|game_dir| game_dir.join("toy2.exe"))
        .and_then(|exe_path| fs::read(exe_path).ok())
        .and_then(|b| exe::Exe::parse(b).ok())
        .zip(exe::level_number(level_path))
        .and_then(|(e, level)| e.player_start(level).ok());
    match start {
        Some(s) => println!("player start {:?}, facing {:.0} degrees (F to play, Shift+F to drop at camera)", s.pos, s.yaw_radians().to_degrees()),
        None => println!("no player start found (toy2.exe not next to data\\, or not a main level file)"),
    }
    let mut viewer = Viewer {
        level,
        world,
        player: None,
        player_creature,
        start,
        player_anims: [0, 1, 2],
        // Buzz's model faces -Z in game space; confirmed in play testing (two R presses).
        player_yaw_offset: std::f32::consts::PI,
        anim_preview: None,
        jump_pressed: false,
        ngn,
        anims,
        gfx: None,
        cam: Camera { pos: Vec3::ZERO, yaw: 0.0, pitch: 0.0 },
        keys: Default::default(),
        looking: false,
        last: Instant::now(),
        creature_anim: vec![0; n_creatures],
        creature_idx,
        creature_pos: None,
        spawns,
        show_spawns: true,
        anim_idx: 0,
        anim_time: 0.0,
        speed: 1.0,
        color_mode: 1,
        alpha_test: true,
    };
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.run_app(&mut viewer).expect("run");
}
