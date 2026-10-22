//! Prototype viewer: renders a level `.ngn` from the user's install with a fly camera, and can
//! spawn any creature from that level and play its animations.
//!
//!   ts2-viewer <path\to\levelNN\level.ngn> [creature]
//!
//! Controls:
//!   right mouse + move   look          WASD          move
//!   Space / Ctrl         up / down     Shift         move faster
//!   N                    show / hide creatures at their spawn points (from level*.raw)
//!   M                    mute / unmute music (sound effects stay on)
//!   P                    place creature in front of camera
//!   F                    play as Buzz (drops him below the camera) / back to free camera
//!     in play mode: WASD run, Space jump, right-drag orbit camera,
//!     animations follow the original move table; [ ] preview any animation, 0 back to the
//!     move table, R turn model 90°; coins, batteries, tokens and lives are collected as in
//!     the original and counted in the window title
//!   Tab                  next creature in this level
//!   [ / ]                previous / next animation        , / .  slower / faster
//!   C                    cycle vertex colour mode (white, bytes 1-3, bytes 3-1)
//!   T                    toggle transparency (green key colour)
//!   Esc                  quit

use glam::{Mat4, Vec3};
use rodio::Source;
use std::sync::Arc;
use std::time::Instant;
use std::{env, fs, process};
use ts2::bmp;
use ts2::formats::{anim, exe, ngn, packets};
use ts2::{buzz, game};
use ts2::scene::{self, Batch, GpuVertex};
use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// Game units per metre-ish; matches the OBJ exports.
const WORLD_SCALE: f32 = 1.0 / 256.0;
/// Offscreen screenshot size (`--shot`).
const SHOT_SIZE: [u32; 2] = [1280, 720];
/// No sound output (set for `--shot`).
static SILENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Animation playback rate in frames per second. The original rate is not confirmed yet.
const ANIM_FPS: f32 = 30.0;

const SHADER: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    // x: colour mode, y: alpha test on, z: world scale
    params: vec4<f32>,
    // x, y: view-depth window this draw keeps (the original's two-pass level draw)
    range: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) depth: f32,
};

@vertex
fn vs_main(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VOut {
    var o: VOut;
    // Game space is +Y down, +Z forward (right-handed). Negating Y and Z together is a rotation,
    // so the level is not mirrored.
    let p = vec3<f32>(pos.x, -pos.y, -pos.z) * u.params.z;
    o.pos = u.view_proj * vec4<f32>(p, 1.0);
    // range.z: a nudge towards the camera (reversed depth: larger is nearer).
    o.pos.z = o.pos.z * (1.0 + u.range.z);
    o.uv = uv;
    o.color = color;
    // Clip w of a perspective projection is the view depth.
    o.depth = o.pos.w;
    return o;
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    if (i.depth < u.range.x || i.depth > u.range.y) { discard; }
    let t = textureSample(tex, samp, i.uv);
    if (u.params.y > 0.5 && t.a < 0.5) { discard; }
    var shade = vec3<f32>(1.0);
    let mode = u32(u.params.x);
    // PS1 convention: 0x80 = 1.0, so double the 0..1 value.
    if (mode == 1u) { shade = i.color.yzw * 2.0; }
    if (mode == 2u) { shade = i.color.wzy * 2.0; }
    // Byte 0 of the vertex colour is alpha (0x80 = PS1 50% semi-transparency); only the
    // blended pipelines use it.
    return vec4<f32>(t.rgb * shade, t.a * i.color.x);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    params: [f32; 4],
    range: [f32; 4],
}

/// The original draws a level in two passes (0x4CDDD0): first the low-detail second mesh set,
/// then, after clearing depth, the detailed first set within `NEAR_TO`, so the detailed set
/// always wins and the low-detail one only fills its gaps (the attic entrance has a face only
/// in the low-detail set). The two sets overlap exactly (coplanar), so drawing both with one
/// depth buffer flickers. The exe's defaults (detail level 2 of the table at 0x508D24) also
/// give the far pass 10000 (`FAR_FROM`), read here earlier as a near clip; drawn that way the
/// gaps showed as holes, so the far pass now starts at 0. View depth in game units.
#[allow(dead_code)]
const FAR_FROM: f32 = 10000.0;
const NEAR_TO: f32 = 12000.0;

/// Game space (units, +Y down, +Z forward) to viewer world space (scaled, +Y up).
fn game_to_world(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], -p[1], -p[2]) * WORLD_SCALE
}

fn world_to_game(p: Vec3) -> [f32; 3] {
    [p.x / WORLD_SCALE, -p.y / WORLD_SCALE, -p.z / WORLD_SCALE]
}

/// Sound output: the level's samples, mixed with the original's volume and pan.
struct AudioOut {
    _stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
    bank: ts2::audio::SoundBank,
    music: Option<rodio::Sink>,
    /// Buzz's voice channel: one voice line at a time (0x49EA60 / 0x4A3E60).
    voice: Option<rodio::Sink>,
    /// DirectSound buffers per sample slot (0x47DE50 loads six copies of each sample): a free
    /// one plays; with all six busy, the one furthest through restarts.
    buffers: std::collections::HashMap<u16, Vec<(rodio::Sink, std::time::Instant)>>,
    /// Owned sounds (0x4A3980): one looping buffer per (slot, owner), stopped 16 ticks after
    /// its owner last played it (0x4A3BE0).
    owned: std::collections::HashMap<(u16, u32), (rodio::Sink, i32, Gains)>,
}

/// Ticks an owned sound keeps looping after the last time its owner played it.
const OWNED_TICKS: i32 = 0x10;
/// Owners of positional sounds: Buzz, things he moves (boxes, script objects), creatures.
const OWNER_BUZZ: u32 = 0;
const OWNER_OBJECT: u32 = 1;
const OWNER_CREATURES: u32 = 2;

const BUFFERS_PER_SAMPLE: usize = 6;

impl AudioOut {
    fn play(&mut self, id: u16, voice: bool, owner: u32, rel: Vec3, right: Vec3, forward: Vec3) {
        if voice && self.voice.as_ref().is_some_and(|v| !v.empty()) {
            return;
        }
        let Some((_, vol, _)) = self.bank.sound(id) else { return };
        let Some(slot) = self.bank.sound_slot(id) else { return };
        let (l, r) = ts2::audio::positional(vol, rel, right, forward);
        let (gl, gr) = self.bank.gains(l, r);
        if self.bank.is_owned(id) {
            let key = (slot, owner);
            // Played again: kept going, its balance following the owner (0x4A3980).
            if let Some((s, t, g)) = self.owned.get_mut(&key)
                && !s.empty()
            {
                *t = OWNED_TICKS;
                g.set(gl, gr);
                return;
            }
            let Some(sample) = self.bank.slot(slot) else { return };
            let Ok(sink) = rodio::Sink::try_new(&self.handle) else { return };
            let gains = Gains::default();
            gains.set(gl, gr);
            sink.append(Looped { data: std::sync::Arc::new(sample.data.clone()), rate: sample.rate, at: 0, gains: gains.clone() });
            self.owned.insert(key, (sink, OWNED_TICKS, gains));
            return;
        }
        if voice {
            let Some(sample) = self.bank.slot(slot) else { return };
            self.voice = self.emit(sample, gl, gr);
        } else {
            self.buffered(slot, gl, gr);
        }
    }

    /// A sound with no position (PlaySound with obj 0: 0x4A37E0), at its two table volumes.
    fn play_flat(&mut self, id: u16) {
        let Some((_, l, r)) = self.bank.sound(id) else { return };
        let Some(slot) = self.bank.sound_slot(id) else { return };
        let (gl, gr) = self.bank.gains(l, r);
        self.buffered(slot, gl, gr);
    }

    /// A sample slot played at a position (0x4A3810), as jingle steps do.
    fn play_slot(&mut self, slot: u16, vol: i32, rel: Vec3, right: Vec3, forward: Vec3) {
        let (l, r) = ts2::audio::positional(vol, rel, right, forward);
        let (gl, gr) = self.bank.gains(l, r);
        self.buffered(slot, gl, gr);
    }

    /// 0x4A3BE0 for `ticks` ticks: owned sounds not played again stop.
    fn tick_owned(&mut self, ticks: i32) {
        self.owned.retain(|_, (s, t, _)| {
            *t -= ticks;
            if *t <= 0 {
                s.stop();
            }
            *t > 0
        });
    }

    /// Plays `slot` on one of its six buffers, as 0x47DE50 picks them.
    fn buffered(&mut self, slot: u16, gl: f32, gr: f32) {
        let Some(sample) = self.bank.slot(slot) else { return };
        let Some(sink) = self.emit(sample, gl, gr) else { return };
        let now = std::time::Instant::now();
        let bufs = self.buffers.entry(slot).or_default();
        bufs.retain(|(s, _)| !s.empty());
        if bufs.len() >= BUFFERS_PER_SAMPLE
            && let Some(k) = (0..bufs.len()).min_by_key(|&k| bufs[k].1)
        {
            bufs.swap_remove(k).0.stop();
        }
        bufs.push((sink, now));
    }

    fn emit(&self, sample: &ts2::audio::Sample, gl: f32, gr: f32) -> Option<rodio::Sink> {
        if gl.max(gr) < 1e-3 {
            return None;
        }
        let stereo: Vec<i16> = sample.data.iter().flat_map(|&s| [(s as f32 * gl) as i16, (s as f32 * gr) as i16]).collect();
        let sink = rodio::Sink::try_new(&self.handle).ok()?;
        sink.append(rodio::buffer::SamplesBuffer::new(2, sample.rate, stereo));
        Some(sink)
    }
}

/// A jingle in progress (0x49E9D0): the next step plays when the timer drops below zero.
struct Jingle {
    steps: Vec<ts2::audio::JingleStep>,
    next: usize,
    timer: i32,
    pos: Vec3,
}

struct GpuBatch {
    texture: usize,
    blend: scene::Blend,
    buffer: wgpu::Buffer,
    count: u32,
}

#[derive(Clone, Copy)]
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
    /// The window and its surface; neither when rendering offscreen (`--shot`), into `target`.
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    target: Option<wgpu::Texture>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    /// Pipelines for Opaque, Alpha, Additive and Darken, in that order.
    pipelines: [wgpu::RenderPipeline; 5],
    /// Uniforms for the far level pass, the near level pass, everything else, and the HUD.
    uniform_bufs: [wgpu::Buffer; 5],
    uniform_bgs: [wgpu::BindGroup; 5],
    /// One bind group per level texture, plus a white one at the end for untextured geometry.
    texture_bgs: Vec<wgpu::BindGroup>,
    depth: wgpu::TextureView,
    /// Detailed level geometry (first mesh set) and the low-detail set drawn far away.
    level: Vec<GpuBatch>,
    level_decal: Vec<GpuBatch>,
    level_two_sided: Vec<GpuBatch>,
    level_far_two_sided: Vec<GpuBatch>,
    level_far: Vec<GpuBatch>,
    /// Billboards of the near and far sets, this frame.
    billboards: [Vec<GpuBatch>; 2],
    creature: Vec<GpuBatch>,
    /// The HUD, in screen space (0..1, y down), drawn last over everything.
    hud: Vec<GpuBatch>,
    /// The background (0x48FB70): cleared to the sky colour, the ground band over it.
    sky: Vec<GpuBatch>,
    clear: [f64; 3],
}

struct Viewer {
    ngn: ngn::Ngn,
    /// The level's background colours and horizon (level.raw packet 36).
    backdrop: Option<packets::Backdrop>,
    /// Its picture (width, height, RGBA), drawn behind everything (PS1 style; the PC version
    /// only used its two colours).
    backdrop_image: Option<(u32, u32, Vec<u8>)>,
    anims: Vec<Vec<anim::Animation>>,
    gfx: Option<Gfx>,
    cam: Camera,
    /// The original follow camera (play mode) and whether it drives the view (V toggles).
    orig_cam: Option<ts2::camera::Camera>,
    /// The abilities the level's gates were last set up for (0x4A2080).
    abilities_shown: u8,
    use_orig_cam: bool,
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
    /// Level geometry (detailed and far sets), handed to the GPU on first resume.
    level: Vec<Batch>,
    level_decal: Vec<Batch>,
    level_two_sided: Vec<Batch>,
    level_far_two_sided: Vec<Batch>,
    level_far: Vec<Batch>,
    /// Camera-facing quads of the near and far sets, rebuilt each frame.
    billboards: [Vec<scene::Billboard>; 2],
    world: game::World,
    player: Option<game::Player>,
    /// Index of BUZZ (id 0) in `ngn.creatures`, if present.
    player_creature: Option<usize>,
    /// Original player start for this level, from the user's toy2.exe.
    start: Option<exe::PlayerStart>,
    /// Animation indices for idle, run and jump. Placeholders until the real ones are identified.
    /// Buzz's move table from toy2.exe; drives his animations in play mode.
    buzz: Option<buzz::BuzzAnimator>,
    audio: Option<AudioOut>,
    player_yaw_offset: f32,
    jump_pressed: bool,
    collision_batch: Option<scene::Batch>,
    random_table: Option<Vec<u8>>,
    poles: Vec<ts2::formats::leveldat::Pole>,
    ledges: (Vec<[[i32; 3]; 2]>, Vec<[[i32; 3]; 2]>),
    show_collision: bool,
    stomp_pressed: bool,
    /// Animation browsed with [ ] in play mode, shown on Buzz until 0 is pressed.
    anim_preview: Option<usize>,
    anim_idx: usize,
    anim_time: f32,
    speed: f32,
    color_mode: u32,
    alpha_test: bool,
    /// Coins, tokens, batteries and power-ups (level.dat list 63 + special objects).
    pickups: Option<ts2::pickups::Pickups>,
    /// Special objects (moved, turned and hidden by pickups and level scripts), drawn each frame.
    specials: ts2::specials::Specials,
    boxes: Option<ts2::boxes::Boxes>,
    script: Option<Box<dyn ts2::script::Script>>,
    creatures: Option<ts2::creatures::Creatures>,
    jingles: Vec<Jingle>,
    hud: String,
    /// Left mouse button: the wrist laser (Square), pressed since the last update and held.
    fire_pressed: bool,
    fire_held: bool,
    lasers: ts2::laser::Lasers,
    /// Sprite 9 (the beam): texture, texture size, cell size and cells 0–2.
    beam_sprite: Option<(usize, [f32; 2], [f32; 2], [[f32; 2]; 3])>,
    /// 0x4DF294: the laser arm pose's frame table.
    laser_pose: Vec<u8>,
    /// The HUD (0x49FD40), its sprite table, and last values (changes show their element).
    hud_state: ts2::hud::Hud,
    hud_sprites: Vec<Option<ts2::pickups::SpriteDef>>,
    hud_last: [i32; 3],
    particles: ts2::particles::Particles,
    /// List 58's sparkles (0x49FB40).
    rewards: ts2::rewards::Rewards,
    /// First-person look (Q = L1, X = R1).
    look: ts2::look::FirstPerson,
    look_pressed: bool,
    r1_pressed: bool,
    /// Presses meant for the speech box, kept until a tick reads them; and a jump held over
    /// from it, ignored until Space is let go.
    talk_jump: bool,
    talk_square: bool,
    jump_blocked: bool,
    /// Mouse motion waiting for first person's next tick (turn, tilt).
    fp_mouse: [f32; 2],
    /// The token sequence (0x830CC4: 1 stop, 2 wait for him to stand, 9 menu, 3 / 4 victory),
    /// the menu choice (0x830C94), a timer (0x52F2DC), menu presses and a frame count.
    token_seq: u8,
    token_choice: u8,
    /// Mr Potato Head's missing part: hidden at the start (0x4A2080), shown once he has it
    /// back (0x4A2480). The part number while hidden, -1 once done.
    potato_part: i32,
    token_timer: i32,
    menu_nav: i32,
    hud_frames: u32,
    /// The front end (title, menus, level select), when started without a level.
    front: Option<Front>,
    /// The scene's level.dat lists.
    level_lists: Vec<ts2::formats::leveldat::PointList>,
    /// Escape in a level, and the level's end, for the front end to act on.
    quit_level: bool,
    level_ended: bool,
    out_of_lives: bool,
    /// The game folder, the level's own music track (its campaign index) and the track playing.
    game_dir: Option<std::path::PathBuf>,
    level_track: Option<usize>,
    music_track: Option<usize>,
}

/// The front end and what it keeps across scene loads.
struct Front {
    state: ts2::frontend::FrontEnd,
    game: std::path::PathBuf,
    /// The scene's sprite table (0x447D40).
    sprites: Vec<Option<ts2::pickups::SpriteDef>>,
    /// The internal id and variant of the scene loaded.
    scene: (usize, u8),
    /// Time not yet run as 60 Hz ticks.
    acc: f32,
    /// The music track playing.
    track: Option<usize>,
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

/// Level geometry: static batches and billboards of the detailed (first) and low-detail
/// (second) mesh sets.
struct LevelSets {
    near: Vec<Batch>,
    /// Detailed-set materials drawn from both sides (mode bit 8: fences, boards, cut-outs).
    two_sided: Vec<Batch>,
    /// Detailed-set instances drawn a hair nearer (`DECAL`).
    decal: Vec<Batch>,
    far: Vec<Batch>,
    far_two_sided: Vec<Batch>,
    near_bb: Vec<scene::Billboard>,
    far_bb: Vec<scene::Billboard>,
}

/// Special objects have their own draw pass. Zone 0 also contains ordinary scenery.
///
/// Culling follows the materials, as the original's does (0x4B70E0 → 0x4B6760): back faces
/// are culled except for materials with mode bit 8, which are drawn from both sides, in both
/// mesh sets. Of those, faces baked pure black stay culled: they're the data's back faces,
/// which would fight a lit twin in the same plane.
fn level_sets(ngn: &ngn::Ngn, specials: &ts2::specials::Specials) -> LevelSets {
    let dynamic: std::collections::HashSet<(usize, usize)> = specials.instances().collect();
    let keep = |l: usize, n: usize, _i: &ngn::Instance| !dynamic.contains(&(l, n));
    // (one-sided, two-sided, billboards) of the instances `pick` takes.
    let parts = |pick: &dyn Fn(usize, &ngn::Instance) -> bool| {
        let (mut one, _) = scene::level_parts_sided(ngn, |l, n, i| pick(l, i) && keep(l, n, i), Some(false));
        let (sheets, bb) = scene::level_parts_sided(ngn, |l, n, i| pick(l, i) && keep(l, n, i), Some(true));
        let mut two = Vec::new();
        for b in sheets {
            for t in b.vertices.chunks_exact(3) {
                let black = t.iter().all(|v| v.color[1] == 0 && v.color[2] == 0 && v.color[3] == 0);
                scene::push(if black { &mut one } else { &mut two }, b.texture, b.blend, [t[0], t[1], t[2]]);
            }
        }
        (one, two, bb)
    };
    let (near, mut two_sided, mut near_bb) = parts(&|l, i| l == 0 && i.flags & DECAL == 0);
    let (decal, decal_two, mut decal_bb) = parts(&|l, i| l == 0 && i.flags & DECAL != 0);
    let (far, far_two_sided, far_bb) = parts(&|l, _| l > 0);
    scene::merge(&mut two_sided, decal_two);
    near_bb.append(&mut decal_bb);
    LevelSets { near, two_sided, decal, far, far_two_sided, near_bb, far_bb }
}

/// Instance flag 0x80000 (meaning unknown; 288 ordinary instances in level 1). They're kept in
/// their own list with a depth nudge, tried against the attic boxes' flicker; that turned out
/// to be black back faces fighting the lit fronts, now culled, so the nudge is 0.
const DECAL: u32 = 0x80000;
/// The nudge: clip depth × (1 + this), with reversed depth (larger is nearer).
const DECAL_BIAS: f32 = 0.0;

fn upload(device: &wgpu::Device, batches: &[Batch], white: usize) -> Vec<GpuBatch> {
    batches
        .iter()
        .filter(|b| !b.vertices.is_empty())
        .map(|b| GpuBatch {
            texture: b.texture.unwrap_or(white),
            blend: b.blend,
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
    fn new(window: Option<Arc<Window>>, ngn: &ngn::Ngn, level: &[Batch], level_far: &[Batch], backdrop: Option<&(u32, u32, Vec<u8>)>) -> Gfx {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = window.as_ref().map(|w| instance.create_surface(w.clone()).expect("create surface"));
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: surface.as_ref(),
            force_fallback_adapter: false,
        }))
        .expect("no suitable GPU adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).expect("request device");

        let (config, target) = match (&surface, &window) {
            (Some(surface), Some(window)) => {
                let size = window.inner_size();
                let mut config = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).expect("surface config");
                config.present_mode = wgpu::PresentMode::AutoVsync;
                surface.configure(&device, &config);
                (config, None)
            }
            _ => {
                let config = wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    width: SHOT_SIZE[0],
                    height: SHOT_SIZE[1],
                    present_mode: wgpu::PresentMode::Fifo,
                    desired_maximum_frame_latency: 2,
                    alpha_mode: wgpu::CompositeAlphaMode::Auto,
                    view_formats: Vec::new(),
                };
                let target = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("shot"),
                    size: wgpu::Extent3d { width: SHOT_SIZE[0], height: SHOT_SIZE[1], depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: config.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                (config, Some(target))
            }
        };

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
        // Blend modes as the exe's render-state cache sets them (0x4B6320): opaque writes depth;
        // the blended modes test depth without writing it.
        use wgpu::{BlendComponent as C, BlendFactor as F, BlendOperation as Op, BlendState};
        let comp = |src, dst| C { src_factor: src, dst_factor: dst, operation: Op::Add };
        // The fifth: opaque with back faces culled, for level geometry (its winding is consistent:
        // 97% of triangles on walkable floor wind upwards, and the back faces are mostly baked
        // black, so drawing both sides showed black walls and box insides).
        let modes: [(Option<BlendState>, bool, bool); 5] = [
            (Some(BlendState::REPLACE), true, false),
            (Some(BlendState::ALPHA_BLENDING), false, false),
            (Some(BlendState { color: comp(F::SrcAlpha, F::One), alpha: comp(F::Zero, F::One) }), false, false),
            (Some(BlendState { color: comp(F::Zero, F::OneMinusSrc), alpha: comp(F::Zero, F::One) }), false, false),
            (Some(BlendState::REPLACE), true, true),
        ];
        let pipelines = modes.map(|(blend, depth_write, cull)| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState { cull_mode: cull.then_some(wgpu::Face::Back), front_face: wgpu::FrontFace::Ccw, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: depth_write,
                depth_compare: wgpu::CompareFunction::Greater,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        }));

        let uniform_bufs: [wgpu::Buffer; 5] = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("uniforms"),
                size: std::mem::size_of::<Uniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let uniform_bgs: [wgpu::BindGroup; 5] = std::array::from_fn(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &uniform_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_bufs[i].as_entire_binding() }],
            })
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
            .chain(backdrop.map(|(w, h, rgba)| bmp::Image { width: *w, height: *h, rgba: rgba.clone() }))
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
        let level_far = upload(&device, level_far, texture_bgs.len() - 1);
        Gfx { window, surface, target, device, queue, config, pipelines, uniform_bufs, uniform_bgs, texture_bgs, depth, level, level_decal: Vec::new(), level_two_sided: Vec::new(), level_far_two_sided: Vec::new(), level_far, billboards: [Vec::new(), Vec::new()], creature: Vec::new(), hud: Vec::new(), sky: Vec::new(), clear: [0.05, 0.06, 0.1] }
    }

    fn resize(&mut self, w: u32, h: u32) {
        self.config.width = w.max(1);
        self.config.height = h.max(1);
        if let Some(s) = &self.surface {
            s.configure(&self.device, &self.config);
        }
        self.depth = depth_view(&self.device, &self.config);
    }

    /// The offscreen target's pixels (RGB, rows top first) after a render.
    fn capture(&self) -> Option<Vec<u8>> {
        let target = self.target.as_ref()?;
        let [w, h] = [self.config.width, self.config.height];
        let row = (w * 4).div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);
        let data = buf.slice(..).get_mapped_range();
        Some((0..h as usize).flat_map(|y| data[y * row as usize..][..w as usize * 4].chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect::<Vec<_>>()).collect())
    }

    fn render(&mut self, uniforms: &Uniforms) {
        let frame = match &self.surface {
            Some(surface) => match surface.get_current_texture() {
                Ok(f) => Some(f),
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    surface.configure(&self.device, &self.config);
                    return;
                }
                Err(e) => {
                    eprintln!("surface error: {e}");
                    return;
                }
            },
            None => None,
        };
        // Far set beyond FAR_FROM, detailed set within NEAR_TO, everything else unclipped.
        let s = uniforms.params[2];
        let ranges = [[0.0, f32::MAX], [0.0, NEAR_TO * s], [0.0, f32::MAX]];
        for (buf, r) in self.uniform_bufs.iter().zip(ranges) {
            let u = Uniforms { range: [r[0], r[1], 0.0, 0.0], ..*uniforms };
            self.queue.write_buffer(buf, 0, bytemuck::bytes_of(&u));
        }
        let decal = Uniforms { range: [0.0, NEAR_TO * s, DECAL_BIAS, 0.0], ..*uniforms };
        self.queue.write_buffer(&self.uniform_bufs[4], 0, bytemuck::bytes_of(&decal));
        // The HUD: screen (x, y) in 0..1, y down, straight to clip space (the shader negates y).
        let hud = Uniforms {
            view_proj: Mat4::from_cols(glam::Vec4::new(2.0, 0.0, 0.0, 0.0), glam::Vec4::new(0.0, 2.0, 0.0, 0.0), glam::Vec4::ZERO, glam::Vec4::new(-1.0, 1.0, 0.5, 1.0)).to_cols_array_2d(),
            params: [1.0, 0.0, 1.0, 0.0],
            range: [0.0, f32::MAX, 0.0, 0.0],
        };
        self.queue.write_buffer(&self.uniform_bufs[3], 0, bytemuck::bytes_of(&hud));
        let view = match (&frame, &self.target) {
            (Some(f), _) => f.texture.create_view(&Default::default()),
            (None, Some(t)) => t.create_view(&Default::default()),
            (None, None) => return,
        };
        let mut enc = self.device.create_command_encoder(&Default::default());
        let order = [scene::Blend::Opaque, scene::Blend::Alpha, scene::Blend::Additive, scene::Blend::Darken];
        // Two passes, depth cleared in between, as 0x4CDDD0 does.
        // (uniforms, batches, level geometry: opaque drawn with back faces culled)
        let passes: [(&[(usize, &[GpuBatch], bool)], bool); 4] = [
            (&[(3, &self.sky, false)], true),
            (&[(0, &self.level_far, true), (0, &self.level_far_two_sided, false), (0, &self.billboards[1], false)], false),
            (&[(1, &self.level, true), (1, &self.level_two_sided, false), (4, &self.level_decal, true), (1, &self.billboards[0], false), (2, &self.creature, false)], false),
            (&[(3, &self.hud, false)], false),
        ];
        for (draws, first) in passes {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if first { wgpu::LoadOp::Clear(wgpu::Color { r: self.clear[0], g: self.clear[1], b: self.clear[2], a: 1.0 }) } else { wgpu::LoadOp::Load },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            // Opaque first, then the blended modes (which test but don't write depth).
            for (i, mode) in order.iter().enumerate() {
                for &(u, batches, level) in draws {
                    pass.set_pipeline(&self.pipelines[if i == 0 && level { 4 } else { i }]);
                    pass.set_bind_group(0, &self.uniform_bgs[u], &[]);
                    for b in batches.iter().filter(|b| b.blend == *mode) {
                        pass.set_bind_group(1, &self.texture_bgs[b.texture], &[]);
                        pass.set_vertex_buffer(0, b.buffer.slice(..));
                        pass.draw(0..b.count, 0..1);
                    }
                }
            }
        }
        self.queue.submit([enc.finish()]);
        if let Some(f) = frame {
            f.present();
        }
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
        if std::mem::take(&mut self.quit_level) {
            self.finish_level(true);
        }
        if std::mem::take(&mut self.level_ended) {
            self.finish_level(false);
        }
        if std::mem::take(&mut self.out_of_lives)
            && let Some(f) = self.front.as_mut()
        {
            f.state.game_over();
            self.player = None;
        }
        self.front_update(dt);
        let fwd = self.cam.forward();
        let right = fwd.cross(Vec3::Y).normalize_or_zero();
        if let Some(player) = self.player.as_mut() {
            // The keys as d-pad bits; the heading from them and the camera-to-Buzz angle as
            // the original reads a keyboard (0x414AF0 → 0x433F40).
            let dpad = [(KeyCode::KeyW, game::pad::UP), (KeyCode::KeyS, game::pad::DOWN), (KeyCode::KeyA, game::pad::LEFT), (KeyCode::KeyD, game::pad::RIGHT)]
                .iter()
                .filter(|(k, _)| self.keys.contains(k))
                .fold(0, |acc, (_, b)| acc | b);
            // 0x52F3AC is the camera's ideal orbit position (+0xC), so the reference is its
            // orbit yaw; the free camera uses its own position.
            let to_buzz = match self.orig_cam.as_ref().filter(|_| self.use_orig_cam) {
                Some(c) => c.yaw & 0xFFF,
                None => {
                    let cam_g = Vec3::from(world_to_game(self.cam.pos));
                    ts2::boxes::angle12(player.pos.x - cam_g.x, player.pos.z - cam_g.z)
                }
            };
            let move_xz = game::dpad_heading(dpad, to_buzz).map_or([0.0; 2], |a| {
                let r = a as f32 / 4096.0 * std::f32::consts::TAU;
                [r.sin(), r.cos()]
            });
            // In a conversation Buzz stands still; jump and Square go to the speech box.
            let talking = self.script.as_ref().is_some_and(|l| l.cutscene.is_some() || l.look.is_some());
            if talking {
                self.talk_jump |= std::mem::take(&mut self.jump_pressed);
                self.talk_square |= std::mem::take(&mut self.fire_pressed);
                std::mem::take(&mut self.stomp_pressed);
                // Space used on the speech box mustn't make Buzz jump when it closes.
                self.jump_blocked = true;
            }
            if !self.keys.contains(&KeyCode::Space) {
                self.jump_blocked = false;
            }
            let move_xz = if talking { [0.0; 2] } else { move_xz };
            let free = !talking && !self.jump_blocked;
            let input = game::Input {
                move_xz,
                jump: free && std::mem::take(&mut self.jump_pressed),
                jump_held: free && self.keys.contains(&KeyCode::Space),
                stomp: !talking && std::mem::take(&mut self.stomp_pressed),
                stomp_held: !talking && self.keys.contains(&KeyCode::KeyE),
                fire: !talking && std::mem::take(&mut self.fire_pressed),
                fire_held: !talking && self.fire_held,
                look: !talking && player.first_person == 0 && self.look_pressed,
                dpad: [(KeyCode::KeyW, game::pad::UP), (KeyCode::KeyS, game::pad::DOWN), (KeyCode::KeyA, game::pad::LEFT), (KeyCode::KeyD, game::pad::RIGHT)]
                    .iter()
                    .filter(|(k, _)| self.keys.contains(k))
                    .fold(0, |acc, (_, b)| acc | b),
            };
            // Boxes and the level script run inside each tick, just before Buzz moves (0x433700
            // is called from 0x436220 there).
            let (boxes, specials, script) = (&mut self.boxes, &mut self.specials, &mut self.script);
            let ticks = player.update_with(&self.world, input, dt, &mut |p, w| {
                if let Some(b) = boxes.as_mut() {
                    b.tick(p, w, specials);
                    if let Some(l) = script.as_mut() {
                        l.step(w, specials, p, b);
                    }
                }
            });
            // First person (0x4038E0) runs in place of the follow camera. Presses wait for a
            // frame that runs a tick, so a fast frame can't swallow them.
            let leaving_l1 = self.look_pressed && player.first_person == 4 && ticks > 0;
            let r1_pressed = self.r1_pressed && ticks > 0;
            if ticks > 0 {
                self.look_pressed = false;
                self.r1_pressed = false;
            }
            let was_fp = player.first_person != 0;
            if let Some(cr) = self.creatures.as_mut() {
                for t in 0..ticks {
                    let pad = ts2::look::Pad {
                        dpad: input.dpad,
                        r1: self.keys.contains(&KeyCode::KeyX),
                        r1_pressed: r1_pressed && t == 0,
                        l1_pressed: leaving_l1 && t == 0,
                        mouse: if t == 0 {
                            // Whole units now; the fraction carries over.
                            let m = self.fp_mouse.map(|v| v.trunc());
                            self.fp_mouse = [self.fp_mouse[0] - m[0], self.fp_mouse[1] - m[1]];
                            m.map(|v| v as i32)
                        } else {
                            [0; 2]
                        },
                    };
                    if player.first_person != 0 {
                        self.look.tick(player, cr, &mut self.particles, &mut self.specials, &self.world, pad);
                    }
                }
            }
            if was_fp && player.first_person == 0 {
                // Back behind him (state 5 resets the follow camera).
                self.orig_cam = Some(ts2::camera::Camera::new(player));
            }
            if let Some(a) = self.audio.as_mut() {
                for id in self.look.sounds.drain(..) {
                    a.play_flat(id);
                }
            }
            if player.first_person == 0
                && let Some(c) = self.orig_cam.as_mut()
            {
                c.focus = self.creatures.as_mut().and_then(|cr| cr.boss.focus.take());
                for _ in 0..ticks {
                    c.tick(player, &self.world);
                }
            }
            // Sounds come from Buzz; the listener is the camera.
            let sounds: Vec<game::Sound> = player.sounds.drain(..).collect();
            let sounds_at: Vec<(game::Sound, Vec3)> = player.sounds_at.drain(..).collect();
            if let Some(audio) = self.audio.as_mut() {
                let to_game = |v: Vec3| Vec3::new(v.x, -v.y, -v.z);
                let listener = Vec3::from(world_to_game(self.cam.pos));
                let (right_g, fwd_g) = (to_game(right).normalize_or_zero(), to_game(Vec3::new(fwd.x, 0.0, fwd.z)).normalize_or_zero());
                for s in sounds {
                    audio.play(s.id, s.voice, OWNER_BUZZ, player.pos - listener, right_g, fwd_g);
                }
                for (s, at) in sounds_at {
                    audio.play(s.id, s.voice, OWNER_OBJECT, at - listener, right_g, fwd_g);
                }
                audio.tick_owned(ticks as i32);
            }
            // Follow camera: orbit behind the player at chest height.
            let target = game_to_world(player.pos.to_array()) + Vec3::Y * 1.2;
            match self.orig_cam.as_ref().filter(|_| self.use_orig_cam && player.first_person == 0) {
                _ if self.script.as_ref().is_some_and(|l| l.look.is_some()) => {
                    let lk = self.script.as_ref().and_then(|l| l.look).unwrap();
                    let p = Vec3::from(lk.from.map(|v| v as f32 / 32.0));
                    let q = Vec3::from(lk.at.map(|v| v as f32 / 32.0));
                    self.cam.pos = game_to_world(p.to_array());
                    let g = (q - p).normalize_or_zero();
                    let w = Vec3::new(g.x, -g.y, -g.z).normalize_or_zero();
                    self.cam.yaw = w.x.atan2(-w.z);
                    self.cam.pitch = w.y.clamp(-1.0, 1.0).asin();
                }
                _ if self.script.as_ref().is_some_and(|l| l.cutscene.is_some()) => {
                    let (pos, look) = self.script.as_ref().and_then(|l| l.cutscene.as_ref()).map(|c| c.camera()).unwrap();
                    let p = Vec3::from(pos.map(|v| v as f32 / 32.0));
                    let q = Vec3::from(look.map(|v| v as f32 / 32.0));
                    self.cam.pos = game_to_world(p.to_array());
                    let g = (q - p).normalize_or_zero();
                    let w = Vec3::new(g.x, -g.y, -g.z).normalize_or_zero();
                    self.cam.yaw = w.x.atan2(-w.z);
                    self.cam.pitch = w.y.clamp(-1.0, 1.0).asin();
                }
                _ if player.first_person != 0 => {
                    let e = ts2::look::FirstPerson::eye(player);
                    self.cam.pos = game_to_world(e.map(|v| v as f32 / 32.0));
                    let g = self.look.forward();
                    let w = Vec3::new(g.x, -g.y, -g.z).normalize_or_zero();
                    self.cam.yaw = w.x.atan2(-w.z);
                    self.cam.pitch = w.y.clamp(-1.0, 1.0).asin();
                }
                Some(c) => {
                    self.cam.pos = game_to_world(c.position().to_array());
                    let g = c.forward();
                    let w = Vec3::new(g.x, -g.y, -g.z).normalize_or_zero();
                    self.cam.yaw = w.x.atan2(-w.z);
                    self.cam.pitch = w.y.clamp(-1.0, 1.0).asin();
                }
                None => self.cam.pos = target - fwd * 4.5,
            }
            // Laser shots fired this frame hit before the creatures move (0x434990 runs in
            // Buzz's update, before 0x4086F0).
            let cam_game = world_to_game(self.cam.pos);
            let shots: Vec<game::Shot> = player.shots.drain(..).collect();
            if let Some(cr) = self.creatures.as_mut() {
                let cg = Vec3::from(cam_game);
                let f = self.cam.forward();
                let cf = Vec3::new(f.x, -f.y, -f.z);
                let mut sounds = Vec::new();
                for shot in shots {
                    // The arm's tip: ahead of his chest along the shot.
                    let a = shot.yaw as f32 / 4096.0 * std::f32::consts::TAU;
                    let muzzle = [shot.origin[0] + (a.sin() * 90.0 * 32.0) as i32, shot.origin[1], shot.origin[2] + (a.cos() * 90.0 * 32.0) as i32];
                    let slots: Vec<Vec3> = cr.slots.iter().map(|c| Vec3::from(c.pos.map(|v| v as f32 / 32.0))).collect();
                    let visible = |i: usize| {
                        let d = slots[i] - cg;
                        d.dot(cf) > d.length() * 0.6
                    };
                    for ev in self.lasers.fire(shot, muzzle, player, &self.world, cr, &visible) {
                        match ev {
                            ts2::laser::Event::Sound(id, at) => sounds.push((id, at)),
                            // 0x4A5D30's impact: a flash by kind, smoke off walls, five sparks.
                            ts2::laser::Event::Impact { at, kind, wall, first_person } => {
                                let (t, m) = match kind {
                                    0 if first_person => (0xB, 5),
                                    0 => (1, 2),
                                    1 => (10, 5),
                                    _ => (0x49, 2),
                                };
                                self.particles.spawn(at, t, m);
                                if wall {
                                    self.particles.spawn(at, 5, 2);
                                }
                                for _ in 0..5 {
                                    if let Some(k) = self.particles.spawn(at, 4, 4) {
                                        let r = self.particles.rand() as i32;
                                        self.particles.slots[k].life = (r & 0xF) * 2 + 0x18;
                                    }
                                }
                            }
                        }
                    }
                }
                cr.sounds.extend(sounds);
            }
            for (at, t, m, angle) in player.effects.drain(..) {
                if let Some(k) = self.particles.spawn(at, t, m) {
                    self.particles.slots[k].angle = angle & 0xFFF;
                }
            }
            self.particles.camera = cam_game.map(|v| (v * 32.0) as i32);
            for _ in 0..ticks {
                self.particles.tick(&self.world, (player.pos_obj(), player.facing));
                self.rewards.tick(self.particles.camera, &mut self.particles);
                if let Some(b) = self.boxes.as_mut() {
                    for k in b.put_out.drain(..) {
                        self.rewards.put_out(k, 0, &mut self.particles);
                    }
                }
                if let Some(l) = self.script.as_mut() {
                    for (k, g) in l.put_out.drain(..) {
                        self.rewards.put_out(k, g, &mut self.particles);
                    }
                    for t in l.pop.drain(..) {
                        for p in self.particles.slots.iter_mut().filter(|p| p.life > 0 && p.typ == t) {
                            p.life = 1;
                        }
                    }
                    if let Some(pk) = self.pickups.as_mut() {
                        if std::mem::take(&mut l.collect_reset) {
                            pk.collectables = 0;
                        }
                        if let Some(shown) = l.show_collectables.take() {
                            pk.show_collectables(shown);
                        }
                        l.collected = pk.collectables;
                    }
                    for (i, shown) in l.pickups.drain(..) {
                        if let Some(pk) = self.pickups.as_mut() {
                            pk.show_item(i, shown);
                        }
                    }
                }
                self.particles.touch(player.pos_obj(), player.spin > 0x14 || player.super_spinning());
                self.lasers.tick();
                let coins = self.pickups.as_ref().map_or(0, |p| p.coins);
                let now = [player.lives, player.health, coins];
                for (k, el) in [ts2::hud::LIVES, ts2::hud::HEALTH, ts2::hud::COINS].into_iter().enumerate() {
                    if now[k] != self.hud_last[k] {
                        self.hud_state.show(el, ts2::hud::SHOW_TICKS);
                    }
                }
                self.hud_last = now;
                self.hud_state.tick(&ts2::hud::Status {
                    lives: player.lives,
                    health: player.health,
                    coins,
                    spin_charge: player.charge,
                    laser: player.laser,
                    laser_charge: player.laser_charge,
                    power: player.power,
                    sheep: self.creatures.as_ref().map_or(0, |c| c.sheep),
                    race: self.script.as_ref().map_or(0, |l| l.race),
                    laps: self.script.as_ref().map_or(0, |l| l.laps),
                    boss: self.script.as_ref().and_then(|l| l.boss_bar).unwrap_or_else(|| self.creatures.as_ref().map_or(0, |c| c.boss.bar)),
                    boss_on: self.script.as_ref().is_some_and(|l| l.boss_hud),
                });
                // 0x49EAC0: the miniboss theme while the boss bar shows, the race theme while
                // the laps show with a race on (0x52F2F8 ≥ 2), else the level's own.
                if let Some(level_track) = self.level_track {
                    let race = self.script.as_ref().map_or(0, |l| l.race);
                    let want = ts2::music::theme(level_track, self.hud_state.showing(ts2::hud::BOSS), self.hud_state.showing(ts2::hud::LAPS), race);
                    if self.music_track != Some(want)
                        && let (Some(a), Some(game)) = (self.audio.as_mut(), self.game_dir.as_ref())
                    {
                        self.music_track = Some(want);
                        play_music(a, game, want);
                    }
                }
            }
            // Creatures (0x4086F0) near the camera.
            if let Some(cr) = self.creatures.as_mut() {
                let cam_obj = cam_game.map(|v| (v * 32.0) as i32);
                for _ in 0..ticks {
                    cr.tick(&self.world, cam_obj, player.pos_obj());
                    cr.contact(player);
                }
                for r in cr.particles.drain(..) {
                    self.particles.create(r.pos, r.vel, r.gravity, r.angle, r.spin, r.typ);
                }
                // Routines that hurt Buzz, and beams they draw this frame.
                for (angle, mode) in cr.hurts.drain(..) {
                    player.hurt(angle, mode);
                }
                if ticks > 0 {
                    self.lasers.ribbons = cr
                        .ribbons
                        .drain(..)
                        .map(|(start, end, color, width)| ts2::laser::Beam { start, end, step: [0; 3], life: 0x20, steps: 1, color, width })
                        .collect();
                    // The grappling hook's rope (0x44E100 type 0x22: brown, 0x1E wide).
                    if let Some((start, end)) = player.rope {
                        self.lasers.ribbons.push(ts2::laser::Beam { start, end, step: [0; 3], life: 0x20, steps: 1, color: [0x80, 0x60, 0x20], width: 0x1E });
                    }
                }
                if let Some(l) = self.script.as_mut() {
                    for r in l.particles.drain(..) {
                        self.particles.create(r.pos, r.vel, r.gravity, r.angle, r.spin, r.typ);
                    }
                    for (at, t, m, spin, life) in l.spawns.drain(..) {
                        if let Some(k) = self.particles.spawn(at, t, m) {
                            self.particles.slots[k].spin = spin;
                            if let Some(life) = life {
                                self.particles.slots[k].life = life;
                            }
                        }
                    }
                }
                for (at, t, m, spin) in cr.spawns.drain(..) {
                    if let Some(k) = self.particles.spawn(at, t, m) {
                        self.particles.slots[k].spin = spin;
                    }
                }
                if let Some(l) = self.script.as_mut() {
                    let coins = self.pickups.as_ref().map_or(0, |p| p.coins);
                    // Mr Potato Head (creature 9) is missing his part until it's handed over: the
                    // level's item number, and for item 1 parts 0xB, 0xC and 0xF too.
                    let item = l.item;
                    let set = |ngn: &mut ngn::Ngn, part: i32, hide: bool| {
                        if let Some(c) = ngn.creatures.iter_mut().find(|c| c.id == 9) {
                            let mut parts = vec![part];
                            if part == 1 {
                                parts.extend([0xB, 0xC, 0xF]);
                            }
                            for p in parts {
                                if let Some(n) = c.nodes.get_mut(p as usize) {
                                    n.hidden = hide;
                                }
                            }
                        }
                    };
                    if self.potato_part == 0 && item > 0 {
                        self.potato_part = item;
                        set(&mut self.ngn, item, true);
                    } else if self.potato_part > 0 && item == 0 {
                        set(&mut self.ngn, self.potato_part, false);
                        self.potato_part = -1;
                    }
                    l.tokens = self.pickups.as_ref().map_or(0, |p| p.tokens);
                    for t in 0..ticks {
                        l.step_creatures(&self.world, &mut self.specials, player, cr, coins);
                        if !std::mem::take(&mut l.look_ticked) {
                            l.tick_look();
                            l.look_ticked = false;
                        }
                        // A conversation starting: Buzz and the character take their places.
                        if let Some((at, facing, slot)) = l.staging.take() {
                            let g = Vec3::from(at[0].map(|v| v as f32 / 32.0));
                            player.pos = self.world.floor_below(g.x, g.z, g.y - 600.0).map_or(g, |y| Vec3::new(g.x, y, g.z));
                            player.vel = [0; 3];
                            player.set_facing(facing[0]);
                            if let Some(c) = cr.slots.get_mut(slot) {
                                c.pos = at[1];
                                c.angles[1] = facing[1];
                                c.target_facing = facing[1];
                            }
                            if let Some(a) = self.audio.as_mut() {
                                a.play_flat(0x1D);
                            }
                        }
                        if let Some(cs) = l.cutscene.as_mut() {
                            let (j, q) = if t == 0 { (std::mem::take(&mut self.talk_jump), std::mem::take(&mut self.talk_square)) } else { (false, false) };
                            let evs = cs.tick(j, q);
                            for ev in evs {
                                match ev {
                                    ts2::cutscene::Event::Sound(id) => {
                                        if let Some(a) = self.audio.as_mut() {
                                            a.play_flat(id);
                                        }
                                    }
                                    ts2::cutscene::Event::Ended(token) => {
                                        l.cutscene = None;
                                        if let Some(k) = token {
                                            l.given[k] = true;
                                            if let Some(at) = self.pickups.as_mut().and_then(|pk| pk.reveal_token_animated(k)) {
                                                l.look_at(at.map(|v| v * 32), player.pos_obj(), 0xB4, 0x10);
                                            }
                                        }
                                        // The follow camera picks up behind him again.
                                        self.orig_cam = Some(ts2::camera::Camera::new(player));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    cr.sounds.append(&mut l.sounds);
                    // Mr Potato Head's gift: the shield's cage opens.
                    if l.abilities != self.abilities_shown
                        && let Some(pk) = self.pickups.as_mut()
                    {
                        // 0x4A27A0: the gates of the abilities now held open.
                        for (cage, mover) in pk.open_gates(l.abilities) {
                            self.specials.set_scale(cage, [0; 3]);
                            self.world.set_mover(mover, false);
                        }
                        self.abilities_shown = l.abilities;
                    }
                    for k in l.reveals.drain(..).collect::<Vec<_>>() {
                        if let Some(at) = self.pickups.as_mut().and_then(|pk| pk.reveal_token_animated(k)) {
                            l.look_at(at.map(|v| v * 32), player.pos_obj(), 0xB4, 0x10);
                        }
                    }
                    if let Some(pk) = self.pickups.as_mut() {
                        for (k, at) in l.token_moves.drain(..) {
                            pk.move_token(k, at);
                        }
                        for k in l.hides.drain(..) {
                            pk.hide_token(k);
                        }
                        for (k, touchable) in l.token_touch.drain(..) {
                            pk.set_token_touchable(k, touchable);
                        }
                    }
                    let (voices, slots): (Vec<u16>, Vec<(u16, i32, [i32; 3])>) = (l.voices.drain(..).collect(), l.slots.drain(..).collect());
                    if let Some(a) = self.audio.as_mut() {
                        let listener = Vec3::from(world_to_game(self.cam.pos));
                        let f = self.cam.forward();
                        let g = |v: Vec3| Vec3::new(v.x, -v.y, -v.z);
                        let (rg, fg) = (g(f.cross(Vec3::Y)).normalize_or_zero(), g(f).normalize_or_zero());
                        for id in voices {
                            a.play(id, true, OWNER_BUZZ, Vec3::ZERO, rg, fg);
                        }
                        for (slot, vol, at) in slots {
                            a.play_slot(slot, vol, Vec3::from(at.map(|v| v as f32 / 32.0)) - listener, rg, fg);
                        }
                    }
                }
                for ev in cr.events.drain(..) {
                    let ts2::creatures::Event::Died { pos, coin, centre, burst, .. } = ev;
                    // 0x405D20's burst: fireballs, or sparkles and smoke.
                    for n in 0..burst.unsigned_abs() as i32 {
                        if burst > 0 {
                            if let Some(k) = self.particles.spawn(centre, 0x23, 0xE) {
                                let r = self.particles.rand() as i32;
                                self.particles.slots[k].spin = r - 0x80;
                            }
                        } else {
                            if let Some(k) = self.particles.spawn(centre, 99, (n as usize & 1) * 10 + 4) {
                                let (a, b, c, d) = (self.particles.rand() as i32, self.particles.rand() as i32, self.particles.rand(), self.particles.rand() as i32);
                                let p = &mut self.particles.slots[k];
                                p.vel[1] -= 0x100;
                                p.life = (a & 0x1F) + 0x78;
                                p.rgb = [(b & 0x7F) as u8 + 0x40; 3];
                                p.spin = if c & 1 == 0 { -0x40 - (d & 0x3F) } else { (d & 0x3F) + 0x40 };
                            }
                            self.particles.spawn(centre, 0x11, 4);
                        }
                    }
                    // The coin is a particle (type 0x3D) that bounces and is picked up by touch.
                    if coin {
                        self.particles.create([pos[0], pos[1] - 0x1000, pos[2]], [0, -0x800, 0], 0x80, 0, 0, 0x3D);
                    }
                }
            }
            // Death (0x52B7DC = 2): after the countdown, back at the start with a life fewer.
            if player.health < 0 && player.dying == 0 && player.lives == 0 && self.front.is_some() {
                // No lives left: game over (the front end takes it from here).
                self.out_of_lives = true;
            } else if player.health < 0 && player.dying == 0 {
                let (at, facing) = match self.start {
                    Some(s) => {
                        let g = Vec3::from(s.pos);
                        (self.world.floor_below(g.x, g.z, g.y - 600.0).map_or(g, |y| Vec3::new(g.x, y, g.z)), s.angle as i32)
                    }
                    None => (player.spawn, 0),
                };
                player.respawn(at, facing);
                println!("Buzz lost a life: {} left", player.lives);
            }
            // Pickups and jingles, one 60 Hz tick at a time (0x440F70 / 0x4A0F80 / 0x49E9D0).
            let to_game = |v: Vec3| Vec3::new(v.x, -v.y, -v.z);
            let f = self.cam.forward();
            let (right_g, fwd_g) = (to_game(f.cross(Vec3::Y)).normalize_or_zero(), to_game(f).normalize_or_zero());
            let listener = Vec3::from(cam_game);
            let mut creature_sounds: Vec<(u16, [i32; 3])> = self.creatures.as_mut().map(|c| c.sounds.drain(..).collect()).unwrap_or_default();
            let owned: Vec<(u16, [i32; 3], usize)> = self.creatures.as_mut().map(|c| c.owned_sounds.drain(..).collect()).unwrap_or_default();
            if let Some(a) = self.audio.as_mut() {
                for (id, at, slot) in owned {
                    a.play(id, false, OWNER_CREATURES + 1 + slot as u32, Vec3::from(at.map(|v| v as f32 / 32.0)) - listener, right_g, fwd_g);
                }
            }
            if let (Some(cr), Some(a)) = (self.creatures.as_mut(), self.audio.as_ref()) {
                for (id, at) in cr.boss.jingles.drain(..) {
                    let pos = Vec3::from(at.map(|v| v as f32 / 32.0));
                    self.jingles.push(Jingle { steps: a.bank.jingle(id).to_vec(), next: 0, timer: 0, pos });
                }
            }
            for ev in self.particles.events.drain(..) {
                match ev {
                    ts2::particles::Event::Sound(id, at) => creature_sounds.push((id, at)),
                    ts2::particles::Event::Coin(at) => {
                        if let Some(pk) = self.pickups.as_mut() {
                            pk.coins = (pk.coins + 1).min(99);
                            if pk.coins == 50
                                && let Some(a) = self.audio.as_mut()
                            {
                                a.play_flat(0x4F);
                            }
                        }
                        if let Some(a) = self.audio.as_ref() {
                            let pos = Vec3::from(at.map(|v| v as f32 / 32.0));
                            self.jingles.push(Jingle { steps: a.bank.jingle(-1).iter().take(1).copied().collect(), next: 0, timer: 0, pos });
                        }
                    }
                    ts2::particles::Event::Hurt(at) => {
                        let b = player.pos_obj();
                        player.hurt(ts2::boxes::angle12((b[0] - at[0]) as f32, (b[2] - at[2]) as f32), 3);
                    }
                }
            }
            if let Some(a) = self.audio.as_mut() {
                for (id, at) in creature_sounds {
                    // A negative id (as scripts pass to PlaySound, 0x49E660) starts a jingle.
                    if id >= 0x8000 {
                        let pos = Vec3::from(at.map(|v| v as f32 / 32.0));
                        self.jingles.push(Jingle { steps: a.bank.jingle(id as i16 as i32).to_vec(), next: 0, timer: 0, pos });
                        continue;
                    }
                    a.play(id, false, OWNER_CREATURES, Vec3::from(at.map(|v| v as f32 / 32.0)) - listener, right_g, fwd_g);
                }
            }
            let mut bursts: Vec<[i32; 3]> = Vec::new();
            for _ in 0..ticks {
                if let Some(pk) = self.pickups.as_mut() {
                    // Health, lives and the shield live on Buzz; pickups add to them.
                    pk.health = player.health.max(0);
                    pk.lives = player.lives;
                    pk.shield = player.shield;
                    if player.rocket == 0 {
                        pk.end_launch();
                    }
                    // Hooks (0x882938) are spent by Buzz and picked up here.
                    pk.ammo_a = player.hook_ammo;
                    let evs = pk.tick(player.pos.to_array(), cam_game);
                    let speed = (player.vel[0] as f32).hypot(player.vel[2] as f32) as i32;
                    pk.follow_shield(player.pos_obj(), cam_game.map(|v| (v * 32.0) as i32), player.facing, speed);
                    if player.dying == 0 {
                        player.health = pk.health;
                    }
                    player.lives = pk.lives;
                    player.shield = pk.shield;
                    player.hook_ammo = pk.ammo_a;
                    for ev in evs {
                        match ev {
                            ts2::pickups::Event::TokenBurst(at) => {
                                let at = at.map(|v| v * 32);
                                bursts.push(at);
                            }
                            ts2::pickups::Event::Collected { index, kind, pos } => {
                                // Info pickups stay; touching one plays its scene (0x402610), and
                                // while it runs (or without one) touching does nothing.
                                if kind == ts2::pickups::Kind::Cutscene {
                                    let id = pk.items.get(index).map(|i| i.id);
                                    if !self.script.as_mut().zip(id).is_some_and(|(l, id)| l.info(id)) {
                                        continue;
                                    }
                                }
                                println!("picked up {kind:?}: {}", pk.summary());
                                // Every pickup plays jingle −1 (PICKUP5 with echoes) from where it was.
                                if let Some(a) = self.audio.as_ref() {
                                    let pos = Vec3::from(pos.map(|v| v as f32));
                                    self.jingles.retain(|j| j.pos != pos);
                                    // The exe plays jingle −1 (PICKUP5 six times, at PS1 pitches the PC
                                    // ignores); the running PC game is heard playing it once (user's
                                    // comparison), so only its first note plays here.
                                    self.jingles.push(Jingle { steps: a.bank.jingle(-1).iter().take(1).copied().collect(), next: 0, timer: 0, pos });
                                }
                                // The green power-up (0x4A0F80 case 10): 0x53C824 = 1200.
                                if kind == ts2::pickups::Kind::Timer {
                                    player.power = 1200;
                                }
                                match kind {
                                    ts2::pickups::Kind::Launch => player.start_rocket(),
                                    ts2::pickups::Kind::HoverBoots => player.start_hover(),
                                    _ => {}
                                }
                                if kind == ts2::pickups::Kind::Other
                                    && let Some(l) = self.script.as_mut()
                                    && l.item > 0
                                {
                                    l.item = -l.item;
                                    println!("you found Mr Potato Head's missing part: take it back to him");
                                }
                                if kind == ts2::pickups::Kind::Token {
                                    // 0x4A0F80 case 2: the token sequence (0x830CC4 = 1).
                                    if self.token_seq == 0 {
                                        self.token_seq = 1;
                                    }
                                }
                            }
                            ts2::pickups::Event::FiftyCoins => {
                                if let Some(a) = self.audio.as_mut() {
                                    a.play_flat(0x4F);
                                }
                            }
                        }
                    }
                }
                for j in &mut self.jingles {
                    j.timer -= 1;
                    if j.timer < 0
                        && let Some(s) = j.steps.get(j.next)
                    {
                        // A note on the sample the last one played is only a pitch change on the
                        // PS1, which the PC ignores: the PC game is heard playing such runs once
                        // (the user's comparison: pickups, level 4's paint), so it's skipped.
                        let repeat = j.next > 0 && j.steps[j.next - 1].slot == s.slot;
                        if !repeat && let Some(a) = self.audio.as_mut() {
                            a.play_slot(s.slot, s.volume, j.pos - listener, right_g, fwd_g);
                        }
                        j.timer = s.delay;
                        j.next += 1;
                    }
                }
                self.jingles.retain(|j| j.next < j.steps.len());
            }
            // A token appearing: sound 0x33 and sixteen sparkles (0x2B) from the burst table.
            for at in bursts {
                if let Some(a) = self.audio.as_mut() {
                    a.play(0x33, false, OWNER_OBJECT, Vec3::from(at.map(|v| v as f32 / 32.0)) - listener, right_g, fwd_g);
                }
                let table = self.pickups.as_ref().map(|p| p.burst.clone()).unwrap_or_default();
                for v in table {
                    let spin = self.particles.rand() as i32 - 0x80;
                    self.particles.create(at, v.map(|c| c >> 2), 0x20, 0, spin, 0x2B);
                }
            }
            // The token sequence (0x4A0F80 / 0x49FD40, 0x830CC4).
            let boss_level = self.pickups.as_ref().is_some_and(|p| p.token_ids.is_empty());
            if self.script.as_mut().is_some_and(|l| std::mem::take(&mut l.done)) && self.token_seq == 0 {
                self.token_seq = 1;
            }
            for _ in 0..ticks {
                // 0x4A0F80: in the menu and the victory he turns to face the camera (by a
                // sixteenth a tick); once he's jumped, the camera keeps looking at him.
                if let Some(look) = self.script.as_mut().and_then(|l| l.look.as_mut()) {
                    if matches!(self.token_seq, 3 | 9) {
                        let cam_yaw = ts2::boxes::angle12((look.at[0] - look.from[0]) as f32, (look.at[2] - look.from[2]) as f32);
                        let mut d = (cam_yaw - player.facing - 0x800) & 0xFFF;
                        if d > 0x7FF {
                            d -= 0x1000;
                        }
                        player.facing = (player.facing + d / 16) & 0xFFF;
                        player.target = player.facing;
                    } else if self.token_seq == 4 {
                        let b = player.pos_obj();
                        look.at = [b[0], b[1] - 0x3000, b[2]];
                    }
                }
                match self.token_seq {
                    1 => {
                        // Buzz stops where he is.
                        player.frozen = true;
                        player.vel[0] = 0;
                        player.vel[2] = 0;
                        self.token_seq = 2;
                    }
                    2 => {
                        if player.on_floor && player.vel[0] == 0 && player.vel[2] == 0 {
                            // 0x402290(1000): the camera close in front of him.
                            let b = player.pos_obj();
                            let c = world_to_game(self.cam.pos).map(|v| (v * 32.0) as i32);
                            let a = ts2::boxes::angle12((b[0] - c[0]) as f32, (b[2] - c[2]) as f32);
                            let sin = |x: i32| ((x & 0xFFF) as f32 / 4096.0 * std::f32::consts::TAU).sin() * 16384.0;
                            let at = [b[0], b[1] - 0x3000, b[2]];
                            let from = [at[0] + sin(a - 0x800) as i32, at[1], at[2] + sin(a - 0x400) as i32];
                            if let Some(l) = self.script.as_mut() {
                                l.look = Some(ts2::script::Look { from, at, ticks: 1000 });
                            }
                            self.token_choice = 0;
                            if boss_level {
                                self.token_seq = 3;
                                self.token_timer = 1000;
                                if let Some(a) = self.audio.as_mut() {
                                    a.play_flat(0xA9);
                                }
                            } else {
                                self.token_seq = 9;
                            }
                        }
                    }
                    9 => {
                        // "keep on playing / exit level": up / down choose, jump picks.
                        let nav = std::mem::take(&mut self.menu_nav);
                        if nav != 0 {
                            let choice = if nav > 0 { 1 } else { 0 };
                            if choice != self.token_choice {
                                self.token_choice = choice;
                                if let Some(a) = self.audio.as_mut() {
                                    a.play_flat(0x3F);
                                }
                            }
                        }
                        // The menu holds the camera, so the press arrives as a speech-box press.
                        if std::mem::take(&mut self.jump_pressed) | std::mem::take(&mut self.talk_jump) {
                            if let Some(a) = self.audio.as_mut() {
                                a.play_flat(0x3E);
                            }
                            if self.token_choice == 1 {
                                self.token_seq = 3;
                                self.token_timer = 1000;
                                if let Some(a) = self.audio.as_mut() {
                                    a.play_flat(0xA9);
                                }
                            } else {
                                self.token_seq = 0;
                                player.frozen = false;
                                if let Some(l) = self.script.as_mut() {
                                    l.look = None;
                                }
                                self.jump_blocked = true;
                                self.orig_cam = Some(ts2::camera::Camera::new(player));
                            }
                        }
                    }
                    3 => {
                        // The victory: he turns to the camera, then jumps for joy.
                        self.token_timer -= 1;
                        if self.token_timer < 0x37A {
                            player.frozen = false;
                            player.start_jump(-0xC00, game::JumpPhase::DoubleReleased);
                            player.frozen = true;
                            if let Some(a) = self.audio.as_mut() {
                                a.play_flat(0x33);
                            }
                            self.token_seq = 4;
                            self.token_timer = 0x44;
                        }
                    }
                    4 => {
                        self.token_timer -= 1;
                        if self.token_timer < 0 && self.front.is_some() {
                            // The level is over (0x52B7DC): the front end takes it from here.
                            self.token_seq = 0;
                            self.level_ended = true;
                        } else if self.token_timer < 0 {
                            println!("level complete (the original leaves the level here; started on one level, play carries on)");
                            self.token_seq = 0;
                            player.frozen = false;
                            if let Some(l) = self.script.as_mut() {
                                l.look = None;
                            }
                            self.orig_cam = Some(ts2::camera::Camera::new(player));
                        }
                    }
                    _ => {}
                }
            }
        } else if self.front.is_none() {
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
        let mut batches = Vec::new();
        if self.show_spawns {
            match (&self.creatures, self.player.is_some()) {
                // Play mode: the creatures as their scripts move and animate them.
                (Some(cr), true) => {
                    let cam = Vec3::from(world_to_game(self.cam.pos));
                    for c in cr.slots.iter().filter(|c| c.id != 0 && c.health > 0) {
                        let Some(ci) = self.ngn.creatures.iter().position(|n| n.id == c.id as usize) else { continue };
                        let pos = c.pos.map(|v| v as f32 / 32.0);
                        if (Vec3::from(pos) - cam).length() > 10176.0 {
                            continue;
                        }
                        let anim = self.anims[ci].get(c.anim as usize);
                        let t = anim.map_or(0.0, |a| (c.time as f32 / 65536.0).min((a.frame_count.max(1) - 1) as f32));
                        let r = |a: i32| a as f32 / 4096.0 * std::f32::consts::TAU;
                        let angles = [r(c.angles[0]), r(c.angles[1]) + self.player_yaw_offset, r(c.angles[2])];
                        let s = c.scale as f32 / 4096.0;
                        let mut cb = match c.scale_y {
                            Some(y) => scene::creature_batches_scaled_xyz(&self.ngn, &self.ngn.creatures[ci], pos, angles, [s, y as f32 / 4096.0, s], anim.map(|a| (a, t))),
                            None if c.scale == 0x1000 => scene::creature_batches_turned(&self.ngn, &self.ngn.creatures[ci], pos, angles, anim.map(|a| (a, t))),
                            None => scene::creature_batches_scaled(&self.ngn, &self.ngn.creatures[ci], pos, angles, s, anim.map(|a| (a, t))),
                        };
                        // +0x34: lit up in the light colour (a hurt boss's flash at 0x2000, double
                        // strength; level 4's paint in its mixed colour).
                        if c.lit {
                            // Vertex colour bytes 1–3 are red, green, blue (the shader's yzw).
                            for v in cb.iter_mut().flat_map(|b| b.vertices.iter_mut()) {
                                for k in 0..3 {
                                    v.color[k + 1] = (v.color[k + 1] as i32 * c.light[k] / 0x1000).clamp(0, 255) as u8;
                                }
                            }
                        }
                        scene::merge(&mut batches, cb);
                    }
                }
                _ => placed.extend(self.spawns.iter().map(|&(ci, p)| (ci, p, 0.0, self.creature_anim[ci]))),
            }
        }
        if let (Some(p), Some(ci)) = (self.player.as_ref().filter(|p| p.first_person < 3 || p.first_person == 5), self.player_creature) {
            let yaw = p.yaw() + self.player_yaw_offset;
            match (self.anim_preview, p.anim.as_ref().and_then(|a| a.pose())) {
                // Real move table: base layer + overlay at the frame its frame list gives.
                // Blinking while invulnerable after a hit.
                (None, Some(_)) if p.hurt_timer > 0 && p.hurt_timer < 0x44 && (p.hurt_timer >> 2) & 1 == 1 => {}
                (None, Some(pose)) => {
                    let layer = |(ai, f): (usize, f32)| self.anims[ci].get(ai).map(|a| (a, f));
                    let c = &self.ngn.creatures[ci];
                    // The spin attack plays animation 9 on the upper body, the laser 0x1A
                    // (0x4011D0); not in states that animate the whole body as one.
                    let mut overlay = pose.overlay;
                    if pose.base.0 != pose.overlay.0 {
                        if p.spin > 0 {
                            overlay = (9, (0x30 - p.spin) as f32 / 2.0);
                        }
                        if let Some(f) = p.laser_pose(&self.laser_pose) {
                            overlay = (0x1A, f);
                        }
                    }
                    scene::merge(
                        &mut batches,
                        scene::creature_batches_layered(&self.ngn, c, p.pos.to_array(), yaw, layer(pose.base), layer(overlay)),
                    );
                }
                (preview, _) => placed.push((ci, p.pos.to_array(), yaw, preview.unwrap_or(0))),
            }
        }
        for (ci, pos, yaw, ai) in placed {
            let c = &self.ngn.creatures[ci];
            let anim = self.anims[ci].get(ai);
            let t = anim.map_or(0.0, |a| self.anim_time % (a.frame_count.max(2) - 1) as f32);
            scene::merge(&mut batches, scene::creature_batches(&self.ngn, c, pos, yaw, anim.map(|a| (a, t))));
        }
        if let Some(pk) = &self.pickups {
            let to_game = |v: Vec3| [v.x, -v.y, -v.z];
            let f = self.cam.forward();
            let r = f.cross(Vec3::Y).normalize_or_zero();
            let u = r.cross(f).normalize_or_zero();
            scene::merge(&mut batches, pk.sprite_batches(&self.ngn, world_to_game(self.cam.pos), to_game(r), to_game(u)));
            pk.apply(&mut self.specials);
            if pk.summary() != self.hud {
                self.hud = pk.summary();
                if let Some(w) = &gfx.window { w.set_title(&format!("Toy Story 2 (Rust) - viewer   {}", self.hud)); }
            }
        }
        // Particles (0x445980).
        {
            let ngn = &self.ngn;
            let texture = |slot: i16| {
                let name = format!("tex{slot:02}");
                let i = ngn.textures.iter().position(|t| t.name.eq_ignore_ascii_case(&name))?;
                let (w, h, _) = ngn.textures[i].dimensions()?;
                Some((i, [(w as f32).abs(), (h as f32).abs()]))
            };
            let f = self.cam.forward();
            let r = f.cross(Vec3::Y).normalize_or_zero();
            let u = r.cross(f).normalize_or_zero();
            let g = |v: Vec3| Vec3::new(v.x, -v.y, -v.z);
            scene::merge(&mut batches, self.particles.batches(&self.hud_sprites, &texture, g(r), g(u)));
        }
        // Laser beams.
        scene::merge(&mut batches, self.lasers.beams_batches(Vec3::from(world_to_game(self.cam.pos)), self.beam_sprite));
        // Special objects at their current transforms; their billboards join the near set's.
        let mut special_bb = Vec::new();
        for s in self.specials.objects.values() {
            scene::merge(&mut batches, scene::special_batches(&self.ngn, s, &mut special_bb));
        }
        if self.show_collision
            && let Some(c) = &self.collision_batch
        {
            batches.push(c.clone());
        }
        gfx.creature = upload(&gfx.device, &batches, gfx.texture_bgs.len() - 1);
        // The HUD, in play mode.
        let hud = match &self.player {
            Some(p) => {
                let status = ts2::hud::Status {
                    lives: p.lives,
                    health: p.health,
                    coins: self.pickups.as_ref().map_or(0, |pk| pk.coins),
                    spin_charge: p.charge,
                    laser: p.laser,
                    laser_charge: p.laser_charge,
                    power: p.power,
                    sheep: self.creatures.as_ref().map_or(0, |c| c.sheep),
                    race: self.script.as_ref().map_or(0, |l| l.race),
                    laps: self.script.as_ref().map_or(0, |l| l.laps),
                    boss: self.script.as_ref().and_then(|l| l.boss_bar).unwrap_or_else(|| self.creatures.as_ref().map_or(0, |c| c.boss.bar)),
                    boss_on: self.script.as_ref().is_some_and(|l| l.boss_hud),
                };
                let ngn = &self.ngn;
                let texture = |slot: i16| {
                    let name = format!("tex{slot:02}");
                    let i = ngn.textures.iter().position(|t| t.name.eq_ignore_ascii_case(&name))?;
                    let (w, h, _) = ngn.textures[i].dimensions()?;
                    Some((i, [(w as f32).abs(), (h as f32).abs()]))
                };
                let mut q = self.hud_state.quads(&status, &self.hud_sprites);
                if let Some(c) = self.script.as_ref().and_then(|l| l.cutscene.as_ref()) {
                    q.extend(c.dialog.quads());
                }
                self.hud_frames = self.hud_frames.wrapping_add(1);
                if self.token_seq == 9 {
                    // 0x49FD40's token menu: the box, the title, the two choices (the chosen one
                    // pulsing) and the hint.
                    let pulse = {
                        let u = (self.hud_frames / 2 % 64) as i32;
                        ((if u > 0x1F { 0x3F - u } else { u }) * 2 + 0x40) as u8
                    };
                    ts2::dialog::boxed(&mut q, 0x4D, 0x5B, 0x166000, 0x2D000, [0, 0, 0x80]);
                    let centred = |q: &mut Vec<ts2::hud::Quad>, y: i32, t: &str, c: [u8; 3]| ts2::dialog::text(q, (320 - 8 * t.len() as i32) / 2, y, t.as_bytes(), c);
                    centred(&mut q, 0x5E, "you have collected a token!", [0x80, 0x80, 0]);
                    let (keep, exit) = if self.token_choice == 0 { ([pulse; 3], [0x80; 3]) } else { ([0x80; 3], [pulse; 3]) };
                    centred(&mut q, 0x6A, "keep on playing", keep);
                    centred(&mut q, 0x72, "exit level", exit);
                    centred(&mut q, 0x7C, "jump to select", [0x80; 3]);
                }
                ts2::hud::batches(&q, &self.hud_sprites, &texture)
            }
            None => self.front.as_ref().map_or_else(Vec::new, |f| front_batches(f, &self.ngn)),
        };
        gfx.hud = upload(&gfx.device, &hud, gfx.texture_bgs.len() - 1);
        // Billboards (rails, spindles, plants) turn to the camera every frame.
        {
            let to_game = |v: Vec3| [v.x, -v.y, -v.z];
            let f = self.cam.forward();
            let r = f.cross(Vec3::Y).normalize_or_zero();
            let u = r.cross(f).normalize_or_zero();
            let white = gfx.texture_bgs.len() - 1;
            let mut near = scene::billboard_batches(&self.billboards[0], to_game(r), to_game(u));
            scene::merge(&mut near, scene::billboard_batches(&special_bb, to_game(r), to_game(u)));
            let far = scene::billboard_batches(&self.billboards[1], to_game(r), to_game(u));
            gfx.billboards = [upload(&gfx.device, &near, white), upload(&gfx.device, &far, white)];
        }
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
            Some(s) if !self.held(KeyCode::ShiftLeft) => (Vec3::from(s.pos), s.angle as i32),
            _ => (Vec3::from(world_to_game(self.cam.pos + self.cam.forward() * 3.0)), 0),
        };
        // The game snaps the start height to the floor; search a little above the stored point.
        let feet = self.world.floor_below(g.x, g.z, g.y - 600.0).map_or(g, |y| Vec3::new(g.x, y, g.z));
        let mut player = game::Player::new(feet);
        player.set_facing(yaw);
        player.anim = self.buzz.clone();
        if let Some(r) = &self.random_table {
            player.set_random_table(r.clone());
        }
        player.set_poles(self.poles.clone());
        player.set_ledges(self.ledges.0.clone(), self.ledges.1.clone());
        self.orig_cam = Some(ts2::camera::Camera::new(&player));
        self.hud_state.level_start();
        self.hud_last = [player.lives, player.health, self.pickups.as_ref().map_or(0, |p| p.coins)];
        self.player = Some(player);
        self.creature_idx = ci;
        self.anim_preview = None;
        println!(
            "play mode: WASD run, Space jump (again in the air: double jump), left mouse laser (hold to charge), E spin on the ground (hold to charge) or stomp in the air, Q first person (WASD look, X target, Q back), right-drag camera, F to exit. Animations come from the original \
             move table{}; [ ] previews any animation, 0 returns to the move table",
            if self.buzz.is_some() { "" } else { " (not available: toy2.exe not found)" }
        );
    }

    /// A key pressed; true to close the window.
    fn key(&mut self, code: KeyCode) -> bool {
        let n_anims = self.anims.get(self.creature_idx).map_or(0, Vec::len).max(1);
        // In the front end's menus the keys are the pad's (read each tick); Escape on the
        // title closes the window, and in a level it quits to the main menu (the pause
        // menu isn't ported).
        if let Some(f) = &self.front {
            match f.state.screen {
                ts2::frontend::Screen::Title if code == KeyCode::Escape => return true,
                ts2::frontend::Screen::Play(_) if code == KeyCode::Escape => self.quit_level = true,
                ts2::frontend::Screen::Play(_) if code == KeyCode::KeyF => {}
                ts2::frontend::Screen::Play(_) => {}
                _ => return false,
            }
            if matches!(code, KeyCode::Escape | KeyCode::KeyF) {
                return false;
            }
        }
        match code {
            KeyCode::Escape => return true,
            KeyCode::KeyP => self.place_creature(),
            KeyCode::KeyF => self.toggle_play(),
            KeyCode::Space if self.player.is_some() => self.jump_pressed = true,
            // Circle (stomp). The original's keyboard default is F, which toggles play mode here.
            KeyCode::KeyE if self.player.is_some() => self.stomp_pressed = true,
            KeyCode::KeyQ if self.player.is_some() => self.look_pressed = true,
            KeyCode::KeyX if self.player.is_some() => self.r1_pressed = true,
            // Menu navigation (the token menu).
            KeyCode::KeyW if self.player.is_some() => self.menu_nav = -1,
            KeyCode::KeyS if self.player.is_some() => self.menu_nav = 1,
            KeyCode::Digit0 if self.player.is_some() => {
                self.anim_preview = None;
                println!("Buzz animations from the original move table");
            }
            KeyCode::KeyR if self.player.is_some() => {
                self.player_yaw_offset = (self.player_yaw_offset + std::f32::consts::FRAC_PI_2) % std::f32::consts::TAU;
                println!("player model turned to {:.0} degrees", self.player_yaw_offset.to_degrees());
            }
            KeyCode::KeyM => {
                if let Some(music) = self.audio.as_ref().and_then(|a| a.music.as_ref()) {
                    let muted = music.volume() > 0.0;
                    music.set_volume(if muted { 0.0 } else { 0.5 });
                    println!("music {}", if muted { "muted" } else { "on" });
                }
            }
            KeyCode::KeyV => {
                self.use_orig_cam = !self.use_orig_cam;
                println!("{}", if self.use_orig_cam { "original camera" } else { "free follow camera (right-drag to orbit)" });
            }
            KeyCode::KeyK => {
                self.show_collision = !self.show_collision;
                println!("collision overlay {}", if self.show_collision { "shown" } else { "hidden" });
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
        false
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
        self.attach(Some(window));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        self.window_event_inner(el, event);
        if self.front.as_ref().is_some_and(|f| f.state.screen == ts2::frontend::Screen::Quit) {
            el.exit();
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: winit::event::DeviceId, event: DeviceEvent) {
        self.device_event_inner(event);
    }
}

impl Viewer {
    /// Puts the loaded scene on the GPU, drawing into `window`.
    fn attach(&mut self, window: Option<Arc<Window>>) {
        let level = std::mem::take(&mut self.level);
        let level_far = std::mem::take(&mut self.level_far);
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
        self.gfx = Some(Gfx::new(window, &self.ngn, &level, &level_far, self.backdrop_image.as_ref()));
        if let Some(g) = self.gfx.as_mut() {
            g.level_decal = upload(&g.device, &self.level_decal, g.texture_bgs.len() - 1);
            g.level_two_sided = upload(&g.device, &self.level_two_sided, g.texture_bgs.len() - 1);
            g.level_far_two_sided = upload(&g.device, &self.level_far_two_sided, g.texture_bgs.len() - 1);
        }
        self.last = Instant::now();
    }

    fn window_event_inner(&mut self, el: &ActiveEventLoop, event: WindowEvent) {
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
                            if self.key(code) {
                                el.exit();
                            }
                        }
                        self.keys.insert(code);
                    } else {
                        self.keys.remove(&code);
                    }
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Right, .. } => self.looking = state == ElementState::Pressed,
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                self.fire_held = state == ElementState::Pressed && self.player.is_some();
                if self.fire_held {
                    self.fire_pressed = true;
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last).as_secs_f32().min(0.1);
                self.last = now;
                self.update(dt);
                self.draw();
                if let Some(w) = self.gfx.as_ref().and_then(|g| g.window.as_ref()) {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }

    /// Renders the frame `update` prepared.
    fn draw(&mut self) {
        let Some(g) = self.gfx.as_mut() else { return };
        let aspect = g.config.width as f32 / g.config.height as f32;
        // Reversed depth (near 1, far 0) with an infinite far plane: a float depth
        // buffer then keeps its precision far out, so near-coplanar faces (a box lid
        // left ajar, a lamp's layered panels) don't fight.
        let proj = Mat4::perspective_infinite_reverse_rh(60f32.to_radians(), aspect, 0.05);
        g.sky = self.backdrop.map_or_else(Vec::new, |b| upload(&g.device, &sky_batch(&b, proj * self.cam.view(), self.cam.forward(), self.backdrop_image.as_ref().map(|_| self.ngn.textures.len()), aspect), g.texture_bgs.len() - 1));
        let u = Uniforms {
            view_proj: (proj * self.cam.view()).to_cols_array_2d(),
            params: [self.color_mode as f32, if self.alpha_test { 1.0 } else { 0.0 }, WORLD_SCALE, 0.0],
            range: [0.0, f32::MAX, 0.0, 0.0],
        };
        g.render(&u);
    }

    /// `--shot`: runs a script of frames and key presses at 60 Hz without a window, then
    /// writes the last frame as a PNG.
    fn run_shot(&mut self, shot: &Shot) {
        self.attach(None);
        for step in &shot.script {
            match step {
                ShotStep::Frames(n) => {
                    for _ in 0..*n {
                        self.update(1.0 / 60.0);
                        if let Some(c) = shot.cam {
                            self.cam = c;
                        }
                        self.draw();
                    }
                }
                ShotStep::Press(k) => {
                    self.key(*k);
                    self.keys.insert(*k);
                }
                ShotStep::Release(k) => {
                    self.keys.remove(k);
                }
                ShotStep::Play => self.toggle_play(),
                ShotStep::Boss => {
                    if let Some(f) = self.front.as_mut() {
                        f.state.level_done(None);
                    }
                }
                ShotStep::Results(r) => {
                    if let Some(f) = self.front.as_mut() {
                        f.state.random = self.random_table.clone().unwrap_or_default();
                        f.state.level = 1;
                        f.state.level_done(Some(*r));
                    }
                }
            }
        }
        self.update(1.0 / 60.0);
        if let Some(c) = shot.cam {
            self.cam = c;
        }
        self.draw();
        let Some(rgb) = self.gfx.as_ref().and_then(Gfx::capture) else { return };
        fs::write(&shot.out, ts2::png::encode_rgb(SHOT_SIZE[0], SHOT_SIZE[1], &rgb)).expect("write shot");
        println!("shot written to {} (camera {:.1?} yaw {:.3} pitch {:.3})", shot.out, self.cam.pos, self.cam.yaw, self.cam.pitch);
    }

    fn device_event_inner(&mut self, event: DeviceEvent) {
        // First person: the mouse turns and tilts the view (0.8 units of 4096, about 0.07°, a count).
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event
            && self.player.as_ref().is_some_and(|p| p.first_person == 4)
        {
            self.fp_mouse[0] += dx as f32 * 0.8;
            self.fp_mouse[1] += dy as f32 * 0.8;
            return;
        }
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event
            && self.looking
        {
            self.cam.yaw += dx as f32 * 0.003;
            self.cam.pitch = (self.cam.pitch - dy as f32 * 0.003).clamp(-1.5, 1.5);
        }
    }

    /// Sets up the front end for the scene just loaded: its sprites, the level select's lists,
    /// or Buzz at the start of a level.
    fn enter_front_scene(&mut self) {
        let Some(f) = self.front.as_mut() else { return };
        let exe = fs::read(f.game.join("toy2.exe")).ok().and_then(|b| exe::Exe::parse(b).ok());
        f.sprites = exe.as_ref().map(|e| ts2::hud::sprite_table(e, if f.scene.0 > ts2::campaign::HUB { ts2::campaign::FRONT } else { f.scene.0 })).unwrap_or_default();
        f.track = None;
        match f.state.screen {
            ts2::frontend::Screen::Select => f.state.begin_select(&self.level_lists, self.random_table.clone().unwrap_or_default()),
            ts2::frontend::Screen::Play(_) => {
                let (lives, abilities) = (f.state.progress.lives, f.state.progress.abilities);
                self.toggle_play();
                if let Some(p) = self.player.as_mut() {
                    p.lives = lives;
                }
                if let Some(l) = self.script.as_mut() {
                    l.abilities |= abilities;
                }
            }
            _ => {}
        }
    }

    /// Loads internal scene `id` in place of this one, keeping the window and the front end.
    fn switch_scene(&mut self, scene: (usize, u8)) {
        let Some(mut front) = self.front.take() else { return };
        let path = ts2::campaign::variant_path(&front.game, scene.0, scene.1);
        match load(&path.to_string_lossy(), None) {
            Ok(mut next) => {
                let attached = self.gfx.is_some();
                let window = self.gfx.take().and_then(|g| g.window);
                front.scene = scene;
                next.front = Some(front);
                *self = next;
                if attached {
                    self.attach(window);
                }
                self.enter_front_scene();
            }
            Err(e) => {
                eprintln!("{e}");
                self.front = Some(front);
            }
        }
    }

    /// The front end's ticks, sounds and music, scene changes and camera.
    fn front_update(&mut self, dt: f32) {
        use ts2::frontend::{Event, Pad, Screen};
        let Some(f) = self.front.as_mut() else { return };
        if matches!(f.state.screen, Screen::Play(_)) {
            return;
        }
        let bits = [
            (&[KeyCode::ArrowUp, KeyCode::KeyW][..], Pad::UP),
            (&[KeyCode::ArrowDown, KeyCode::KeyS], Pad::DOWN),
            (&[KeyCode::ArrowLeft, KeyCode::KeyA], Pad::LEFT),
            (&[KeyCode::ArrowRight, KeyCode::KeyD], Pad::RIGHT),
            (&[KeyCode::Space, KeyCode::Enter, KeyCode::NumpadEnter], Pad::JUMP),
            (&[KeyCode::Escape, KeyCode::Backspace], Pad::CANCEL),
        ];
        let pad = Pad(bits.iter().filter(|(keys, _)| keys.iter().any(|k| self.keys.contains(k))).fold(0, |a, (_, b)| a | b));
        f.acc += dt;
        while f.acc >= 1.0 / 60.0 {
            f.acc -= 1.0 / 60.0;
            f.state.tick(pad);
        }
        for ev in std::mem::take(&mut f.state.events) {
            match ev {
                Event::Sound(id) => {
                    if let Some(a) = self.audio.as_mut() {
                        a.play_flat(id);
                    }
                }
                Event::Music { track, .. } => {
                    if f.track != Some(track) {
                        f.track = Some(track);
                        if let Some(a) = self.audio.as_mut() {
                            play_music(a, &f.game, track);
                        }
                    }
                }
            }
        }
        let want = (f.state.screen.scene(&f.state.tables), f.state.screen.variant());
        if want != f.scene && f.state.screen != Screen::Quit {
            self.switch_scene(want);
            return;
        }
        let frame = f.state.frame(&f.sprites);
        // The level select's camera (0x4CE050): pitch and yaw in 12-bit units, pitch down.
        if let Some((pos, pitch, yaw)) = frame.camera {
            let a = |v: i32| (v & 0xFFF) as f32 / 4096.0 * std::f32::consts::TAU;
            let g = Vec3::new(a(yaw).sin() * a(pitch).cos(), a(pitch).sin(), a(yaw).cos() * a(pitch).cos());
            let w = Vec3::new(g.x, -g.y, -g.z).normalize_or_zero();
            self.cam.pos = game_to_world(pos.map(|v| v as f32 / 32.0));
            self.cam.yaw = w.x.atan2(-w.z);
            self.cam.pitch = w.y.clamp(-1.0, 1.0).asin();
        }
        for (id, pos, yaw) in frame.pieces {
            self.specials.set_pos(id as i32, pos);
            self.specials.set_angles(id as i32, [0, yaw, 0]);
        }
        for (id, on) in frame.shown {
            self.specials.set_scale(id as i32, [if on { 0x1000 } else { 0 }; 3]);
        }
    }

    /// Leaving a level started from the front end: its tokens count (0x52F0D7, set as they're
    /// collected), and a beaten boss level is marked 0x80. Finished: back to the level select;
    /// quit: to the main menu (0x52B7DC = 4).
    fn finish_level(&mut self, quit: bool) {
        let Some(f) = self.front.as_mut() else { return };
        let ts2::frontend::Screen::Play(n) = f.state.screen else { return };
        let id = f.state.tables.order.get(n - 1).copied().unwrap_or(0);
        let boss = self.pickups.as_ref().is_some_and(|p| p.token_ids.is_empty());
        let mut bits = self.pickups.as_ref().map_or(0, |p| p.tokens);
        if !quit && boss {
            bits |= 0x80;
        }
        let before = f.state.progress.tokens[id];
        f.state.progress.tokens[id] |= bits;
        // Lives and abilities are the game's, not the level's: they carry on.
        if let Some(p) = &self.player {
            f.state.progress.lives = p.lives;
        }
        if let Some(l) = &self.script {
            f.state.progress.abilities = l.abilities;
        }
        if quit {
            f.state.level_quit();
        } else {
            f.state.first_clear = before == 0 && f.state.progress.tokens[id] != 0;
            let result = (!boss).then(|| ts2::frontend::LevelResult {
                before,
                now: f.state.progress.tokens[id],
                coins: self.pickups.as_ref().map_or(0, |p| p.coins),
                abilities: self.script.as_ref().map_or(0, |l| l.abilities),
            });
            if f.state.random.is_empty() {
                f.state.random = self.random_table.clone().unwrap_or_default();
            }
            f.state.level_done(result);
        }
        self.player = None;
    }
}

/// The front end's screen: its picture, sprites (in the scene's `texNN` textures), texture
/// 31's letters, and the fade over them.
fn front_batches(f: &Front, ngn: &ngn::Ngn) -> Vec<Batch> {
    let frame = f.state.frame(&f.sprites);
    let named = |name: &str| ngn.textures.iter().position(|t| t.name.eq_ignore_ascii_case(name));
    let mut out = Vec::new();
    // The picture is drawn blended (fully opaque), not as an opaque batch: the HUD pass draws
    // opaque batches first, writing depth, and the sprites over it would fail the test.
    if let Some(i) = frame.picture.and_then(|p| named(&format!("bgr{p:02}")).or_else(|| named(&format!("tex{p:02}")))) {
        let c = |x: f32, y: f32| GpuVertex { pos: [x, y, 0.0], uv: [x, y], color: [0xFF, 0x80, 0x80, 0x80] };
        out.push(Batch { texture: Some(i), blend: scene::Blend::Alpha, vertices: vec![c(0.0, 0.0), c(1.0, 0.0), c(1.0, 1.0), c(0.0, 0.0), c(1.0, 1.0), c(0.0, 1.0)] });
    }
    let texture = |slot: i16| {
        let i = named(&format!("tex{slot:02}"))?;
        let (w, h, _) = ngn.textures[i].dimensions()?;
        Some((i, [(w as f32).abs(), (h as f32).abs()]))
    };
    out.extend(ts2::hud::batches(&frame.quads, &f.sprites, &texture));
    if let Some(font) = named("tex31") {
        let mut v = Vec::new();
        for g in &frame.glyphs {
            let (x0, y0) = (g.pos[0] / g.width, g.pos[1] / 256.0);
            let (x1, y1) = ((g.pos[0] + g.size[0]) / g.width, (g.pos[1] + g.size[1]) / 256.0);
            let (u0, v0) = (g.cell[0] as f32 / 255.0, g.cell[1] as f32 / 255.0);
            let (u1, v1) = ((g.cell[0] as f32 + 31.0) / 255.0, (g.cell[1] as f32 + 31.0) / 255.0);
            let c = |x: f32, y: f32, u: f32, w: f32| GpuVertex { pos: [x, y, 0.0], uv: [u, w], color: [0xFF, 0x80, 0x80, 0x80] };
            v.extend([c(x0, y0, u0, v0), c(x1, y0, u1, v0), c(x1, y1, u1, v1), c(x0, y0, u0, v0), c(x1, y1, u1, v1), c(x0, y1, u0, v1)]);
        }
        out.push(Batch { texture: Some(font), blend: scene::Blend::Alpha, vertices: v });
    }
    if frame.brightness < 0x80 {
        let a = ((0x80 - frame.brightness.max(0)) * 2).min(0xFF) as u8;
        let c = |x: f32, y: f32| GpuVertex { pos: [x, y, 0.0], uv: [0.0; 2], color: [a, 0, 0, 0] };
        out.push(Batch { texture: None, blend: scene::Blend::Alpha, vertices: vec![c(0.0, 0.0), c(1.0, 0.0), c(1.0, 1.0), c(0.0, 0.0), c(1.0, 1.0), c(0.0, 1.0)] });
    }
    out
}

/// Plays music track `index` (0x4ECCD8) on a loop, in place of what was playing.
fn play_music(a: &mut AudioOut, game: &std::path::Path, index: usize) {
    // Muted (M) stays muted.
    let volume = a.music.as_ref().map_or(0.5, |m| m.volume());
    let sink = (|| -> Result<rodio::Sink, String> {
        let exe = exe::Exe::parse(fs::read(game.join("toy2.exe")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let root = ts2::music::media_root(game).ok_or("CD music not found; set TS2_CD_DIR to the folder containing audio/")?;
        let name = ts2::music::track(&exe, index)?;
        let path = root.join("audio").join(format!("{name}.wav"));
        let wav = ts2::music::decode_pcm(&fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?)?;
        let sink = rodio::Sink::try_new(&a.handle).map_err(|e| e.to_string())?;
        sink.set_volume(volume);
        sink.append(rodio::buffer::SamplesBuffer::new(wav.channels, wav.rate, wav.samples).repeat_infinite());
        println!("music: {name}");
        Ok(sink)
    })();
    match sink {
        Ok(s) => a.music = Some(s),
        Err(e) => println!("no music: {e}"),
    }
}

/// `--shot <out.png> "<script>"`: render without a window. The script's words: a number runs
/// that many 60 Hz frames, `+Key` / `-Key` press and release a key (winit names: Space,
/// Enter, Escape, ArrowUp, KeyW, …), `play` puts Buzz at the start (F), and
/// `cam:x,y,z,yaw,pitch` holds the view there (world units, radians).
struct Shot {
    out: String,
    script: Vec<ShotStep>,
    cam: Option<Camera>,
}

enum ShotStep {
    Frames(u32),
    Press(KeyCode),
    Release(KeyCode),
    Play,
    /// `results:before,now,coins,abilities`: the front end's results screen with these numbers.
    Results(ts2::frontend::LevelResult),
    /// `boss`: the boss-defeated screen.
    Boss,
}

impl Shot {
    /// Removes `--shot` and its two values from `args`.
    fn take(args: &mut Vec<String>) -> Option<Shot> {
        let i = args.iter().position(|a| a == "--shot")?;
        let words: Vec<String> = args.drain(i..(i + 3).min(args.len())).collect();
        let out = words.get(1)?.clone();
        let mut shot = Shot { out, script: Vec::new(), cam: None };
        for w in words.get(2).map_or("", String::as_str).split_whitespace() {
            let key = |s: &str| -> Option<KeyCode> {
                Some(match s {
                    "Space" => KeyCode::Space,
                    "Enter" => KeyCode::Enter,
                    "Escape" => KeyCode::Escape,
                    "ArrowUp" => KeyCode::ArrowUp,
                    "ArrowDown" => KeyCode::ArrowDown,
                    "ArrowLeft" => KeyCode::ArrowLeft,
                    "ArrowRight" => KeyCode::ArrowRight,
                    "KeyW" => KeyCode::KeyW,
                    "KeyA" => KeyCode::KeyA,
                    "KeyS" => KeyCode::KeyS,
                    "KeyD" => KeyCode::KeyD,
                    "KeyE" => KeyCode::KeyE,
                    "KeyQ" => KeyCode::KeyQ,
                    _ => return None,
                })
            };
            if let Ok(n) = w.parse() {
                shot.script.push(ShotStep::Frames(n));
            } else if let Some(r) = w.strip_prefix("results:") {
                let v: Vec<i32> = r.split(',').filter_map(|x| x.parse().ok()).collect();
                if let [before, now, coins, abilities] = v[..] {
                    shot.script.push(ShotStep::Results(ts2::frontend::LevelResult { before: before as u8, now: now as u8, coins, abilities: abilities as u8 }));
                }
            } else if w == "boss" {
                shot.script.push(ShotStep::Boss);
            } else if w == "play" {
                shot.script.push(ShotStep::Play);
            } else if let Some(c) = w.strip_prefix("cam:") {
                let v: Vec<f32> = c.split(',').filter_map(|x| x.parse().ok()).collect();
                if let [x, y, z, yaw, pitch] = v[..] {
                    shot.cam = Some(Camera { pos: Vec3::new(x, y, z), yaw, pitch });
                }
            } else if let Some(k) = w.strip_prefix('+').and_then(key) {
                shot.script.push(ShotStep::Press(k));
            } else if let Some(k) = w.strip_prefix('-').and_then(key) {
                shot.script.push(ShotStep::Release(k));
            } else {
                eprintln!("--shot: unknown step {w}");
            }
        }
        Some(shot)
    }
}

fn run(mut viewer: Viewer, shot: Option<Shot>) {
    match shot {
        Some(s) => viewer.run_shot(&s),
        None => {
            let event_loop = EventLoop::new().expect("event loop");
            event_loop.run_app(&mut viewer).expect("run");
        }
    }
}

fn main() {
    let mut args: Vec<String> = env::args().collect();
    let shot = Shot::take(&mut args);
    SILENT.store(shot.is_some(), std::sync::atomic::Ordering::Relaxed);
    if args.get(1).is_some_and(|s| s == "--level") {
        let selected = args.get(2).and_then(|s| s.parse().ok()).and_then(ts2::campaign::level);
        let Some(selected) = selected else {
            eprintln!("--level requires a campaign number from 1 to 15");
            process::exit(2);
        };
        let game = args.get(3).map(std::path::PathBuf::from)
            .or_else(|| env::var_os("TS2_GAME_DIR").map(std::path::PathBuf::from))
            .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Program Files\Disney Interactive\Toy Story 2"));
        println!("Campaign {}: {} (internal ID {})", selected.number, selected.name, selected.internal_id);
        if !selected.script_available() {
            eprintln!("Level assets are available; this level's gameplay script is not ported yet.");
        }
        args = vec![args[0].clone(), selected.path(&game).to_string_lossy().into_owned()];
    }
    // No level given (or a game folder): the front end, from the title screen.
    let front_game = match args.get(1) {
        None => Some(env::var_os("TS2_GAME_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(r"C:\Program Files\Disney Interactive\Toy Story 2"))),
        Some(a) if std::path::Path::new(a).is_dir() => Some(std::path::PathBuf::from(a)),
        _ => None,
    };
    if let Some(game) = front_game {
        let exe = fs::read(game.join("toy2.exe")).ok().and_then(|b| exe::Exe::parse(b).ok()).unwrap_or_else(|| {
            eprintln!("toy2.exe not found in {}", game.display());
            process::exit(1);
        });
        let mut front = ts2::frontend::FrontEnd::new(ts2::frontend::Tables::read(&exe));
        // New saves start with the controls and settings of the game's slot 0.
        front.defaults = fs::read(game.join("data").join("Toy200.sav")).ok().and_then(|b| ts2::save::Save::parse(&b)).map(|s| s.state).unwrap_or_default();
        println!("saves: {}", front.save_dir.display());
        let path = ts2::campaign::scene_path(&game, ts2::campaign::FRONT);
        let mut viewer = load(&path.to_string_lossy(), None).unwrap_or_else(|e| {
            eprintln!("{e}");
            process::exit(1);
        });
        viewer.front = Some(Front { state: front, game, sprites: Vec::new(), scene: (ts2::campaign::FRONT, 0), acc: 0.0, track: None });
        viewer.enter_front_scene();
        println!("front end: arrows or WASD move, Space or Enter picks, Escape goes back");
        run(viewer, shot);
        return;
    }
    let Some(path) = args.get(1) else {
        eprintln!("usage: ts2-viewer [game_dir]\n       ts2-viewer <path\\to\\levelNN\\level.ngn> [creature]\n       ts2-viewer --level <1..15> [game_dir]");
        process::exit(2);
    };
    let viewer = load(path, args.get(2).map(String::as_str));
    let viewer = viewer.unwrap_or_else(|e| {
        eprintln!("{e}");
        process::exit(1);
    });
    run(viewer, shot);
}

/// Loads a level (or a front-end scene) from its `.ngn` and the files beside it.
fn load(path: &str, creature: Option<&str>) -> Result<Viewer, String> {
    let bytes = fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let mut ngn = ngn::parse(&bytes).map_err(|e| format!("cannot parse {path}: {e}"))?;
    // The original keeps one texture table that each scene's file only partly fills, so the
    // level-select map uses the front end's font (tex31) and pictures: borrow what it lacks.
    if exe::level_number(std::path::Path::new(path)) == Some(ts2::campaign::HUB)
        && let Some(front) = std::path::Path::new(path).ancestors().nth(2).map(|d| d.join("level00").join("level.ngn"))
        && let Some(f) = fs::read(front).ok().and_then(|b| ngn::parse(&b).ok())
    {
        for t in f.textures {
            if !ngn.textures.iter().any(|m| m.name.eq_ignore_ascii_case(&t.name)) {
                ngn.textures.push(t);
            }
        }
    }
    let anims: Vec<Vec<anim::Animation>> =
        ngn.creatures.iter().map(|c| c.animations.iter().filter_map(|a| anim::parse(a).ok()).collect()).collect();
    let creature_idx = creature
        .and_then(|name| ngn.creatures.iter().position(|c| c.name.eq_ignore_ascii_case(name)))
        .unwrap_or(0);
    println!("{} creatures: {:?}", ngn.creatures.len(), ngn.creatures.iter().map(|c| &c.name).collect::<Vec<_>>());

    // Spawns come from the packet file with the same stem (level.ngn -> level.raw).
    let raw_path = std::path::Path::new(path).with_extension("raw");
    let mut all_spawns = Vec::new();
    let mut backdrop = None;
    let mut backdrop_image = None;
    let spawns: Vec<(usize, [f32; 3])> = match fs::read(&raw_path).map_err(|e| e.to_string()).and_then(|b| packets::parse(&b).map_err(|e| e.to_string())) {
        Ok(pk) => {
            let all: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
            backdrop = pk.iter().find_map(packets::backdrop);
            backdrop_image = pk.iter().find_map(packets::backdrop_rgba);
            all_spawns = all.clone();
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
    let level_path = std::path::Path::new(path);
    let game_exe = level_path
        .ancestors()
        .nth(3)
        .map(|game_dir| game_dir.join("toy2.exe"))
        .and_then(|exe_path| fs::read(exe_path).ok())
        .and_then(|b| exe::Exe::parse(b).ok());
    // Pickups from the level's .dat file; their special objects are drawn per frame (they
    // tumble and disappear), so the static level leaves them out.
    let mut pickups = exe::level_number(level_path).and_then(|lv| {
        let dat = fs::read(level_path.with_extension("dat")).ok()?;
        match ts2::pickups::Pickups::new(lv, &dat, game_exe.as_ref()) {
            Ok(p) => Some(p),
            Err(e) => {
                println!("no pickups: {e}");
                None
            }
        }
    });
    let specials = ts2::specials::Specials::new(&ngn);
    // Camera models are specials, excluded here and controlled by first-person mode.
    // Zone 0 also includes houses, fences and the tree in the neighbourhood.
    let LevelSets { near: level, two_sided: level_two_sided, decal: level_decal, far: level_far, far_two_sided: level_far_two_sided, near_bb, far_bb } = level_sets(&ngn, &specials);
    // Collision: the original's TERRAIN.ALL / TERR1.ALL next to the level, else the render mesh.
    let terrain = ts2::formats::terrain::path_for(std::path::Path::new(path))
        .and_then(|p| std::fs::read(&p).ok().map(|b| (p, b)))
        .and_then(|(p, b)| match ts2::formats::terrain::parse(&b) {
            Ok(t) => Some(t),
            Err(e) => {
                println!("{}: {e}", p.display());
                None
            }
        });
    let world = match &terrain {
        Some(t) => {
            println!("collision from the original terrain file ({} objects; K shows it)", t.objects.len());
            game::World::from_terrain(t)
        }
        None => {
            println!("no terrain file: colliding with the render geometry");
            game::World::from_batches(&level)
        }
    };
    // The game's random bytes (data/rand.dat, next to the levelNN folders): idle fidgets.
    let random_table = std::path::Path::new(path).parent().and_then(|d| d.parent()).and_then(|d| std::fs::read(d.join("rand.dat")).ok());
    // Poles and ropes from the level's .dat file (list 61).
    let dat_lists = std::fs::read(std::path::Path::new(path).with_extension("dat")).ok().and_then(|b| ts2::formats::leveldat::parse(&b).ok()).unwrap_or_default();
    let poles = ts2::formats::leveldat::poles(&dat_lists);
    // Ledges Buzz can grab (list 60) and zip lines (list 62).
    let ledges = (ts2::formats::leveldat::segments(&dat_lists, ts2::formats::leveldat::LEDGES), ts2::formats::leveldat::segments(&dat_lists, ts2::formats::leveldat::ZIP_LINES));
    println!("{} ledges, {} zip lines", ledges.0.len(), ledges.1.len());
    println!("{} poles/ropes to climb", poles.len());
    // Collision overlay (K): the terrain triangles, nudged up so they don't z-fight the floor.
    let collision_batch = terrain.as_ref().map(|t| scene::Batch {
        vertices: t
            .static_triangles()
            .into_iter()
            .flat_map(|tri| tri.map(|p| scene::GpuVertex { pos: [p[0], p[1] - 4.0, p[2]], uv: [0.0; 2], color: [0xFF, 0x30, 0xC0, 0x30] }))
            .collect(),
        ..Default::default()
    });
    let player_creature = ngn.creatures.iter().position(|c| c.id == 0);
    // Creatures run their behaviour scripts in play mode (exe tables needed).
    let mut creatures = game_exe.as_ref().map(|e| ts2::creatures::Creatures::new(&all_spawns, Some(e), random_table.clone().unwrap_or_default()));
    // Hit shapes from each character's .all file.
    if let (Some(cr), Some(data)) = (creatures.as_mut(), std::path::Path::new(path).parent().and_then(|d| d.parent())) {
        cr.load_shapes(data);
        cr.load_animations(&ngn);
    }

    // Pushable boxes and the level script, selected by executable ID.
    let mut specials = specials;
    let level_lists = fs::read(level_path.with_extension("dat")).ok().and_then(|b| ts2::formats::leveldat::parse(&b).ok()).unwrap_or_default();
    let level_no = exe::level_number(level_path);
    let boxes = level_no.map(|lv| ts2::boxes::Boxes::new(lv, &level_lists, game_exe.as_ref(), &world, &mut specials));
    let script = level_no.and_then(|lv| ts2::script::for_level(lv, &world, &mut specials, game_exe.as_ref(), &level_lists, random_table.clone()));
    // Deviation: every creature stays in play wherever the camera is. The original (0x4086F0)
    // freezes those beyond about 3200 level units of the camera (all of them in play only in
    // level 6), hidden there by its short draw distance; drawn further here, they visibly
    // stopped mid-move, and the Tin Man's attack cycle paused whenever the camera looked away.
    if let Some(c) = creatures.as_mut() {
        c.always_active = true;
    }
    if let Some(b) = &boxes {
        println!("{} pushable objects", b.boxes.len());
    }
    if let Some(p) = pickups.as_mut() {
        p.probe_floors(&world);
        let coins = p.items.iter().filter(|i| i.is_sprite()).count();
        println!("{} pickups ({coins} coins); the window title shows {}", p.items.len(), p.summary());
    }
    // Player start: table in the user's toy2.exe (game dir = two levels above levelNN\).
    let start = game_exe.as_ref().zip(exe::level_number(level_path)).and_then(|(e, level)| e.player_start(level).ok());
    let buzz = game_exe.as_ref().and_then(|e| e.buzz_moves().ok()).map(buzz::BuzzAnimator::new);
    // Sound effects from data\sfx, as the level loads them (0x47EC20).
    // Silent when rendering screenshots (`--shot`).
    let audio = game_exe.as_ref().zip(exe::level_number(level_path)).filter(|_| !SILENT.load(std::sync::atomic::Ordering::Relaxed)).and_then(|(e, level)| {
        let sfx = level_path.parent()?.parent()?.join("sfx");
        let bank = match ts2::audio::SoundBank::load(e, &sfx, level) {
            Ok(b) => b,
            Err(err) => {
                println!("no sound: {err}");
                return None;
            }
        };
        match rodio::OutputStream::try_default() {
            Ok((stream, handle)) => {
                println!("sound: {} samples for level {level}", bank.sample_count());
                let music = (|| -> Result<rodio::Sink, String> {
                    use rodio::Source;
                    let root = ts2::music::media_root(level_path.ancestors().nth(3).unwrap())
                        .ok_or("CD music not found; set TS2_CD_DIR to the folder containing audio/")?;
                    let name = ts2::music::level_track(e, level)?;
                    let path = root.join("audio").join(format!("{name}.wav"));
                    let wav = ts2::music::decode_pcm(&fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?)?;
                    let sink = rodio::Sink::try_new(&handle).map_err(|e| e.to_string())?;
                    sink.set_volume(0.5);
                    sink.append(rodio::buffer::SamplesBuffer::new(wav.channels, wav.rate, wav.samples).repeat_infinite());
                    println!("music: {name} (M to mute)");
                    Ok(sink)
                })().map_err(|e| println!("no music: {e}")).ok();
                Some(AudioOut { _stream: stream, handle, bank, music, voice: None, buffers: Default::default(), owned: Default::default() })
            }
            Err(err) => {
                println!("no sound output: {err}");
                None
            }
        }
    });
    // The laser beam's sprite (9 in the table 0x4EC0A4) and the arm pose table.
    let beam_sprite = game_exe.as_ref().and_then(|e| ts2::pickups::sprite_def(e, 9)).and_then(|d| {
        let name = format!("tex{:02}", d.texture);
        let i = ngn.textures.iter().position(|t| t.name.eq_ignore_ascii_case(&name))?;
        let (w, h, _) = ngn.textures[i].dimensions()?;
        let c = |k: usize| d.cells.get(k).map_or([0.0; 2], |c| [c[0] as f32, c[1] as f32]);
        Some((i, [(w as f32).abs(), (h as f32).abs()], [d.size[0] as f32, d.size[1] as f32], [c(0), c(1), c(2)]))
    });
    let laser_pose = game_exe.as_ref().and_then(|e| e.at(0x4DF294, 32).ok()).map(|b| b.to_vec()).unwrap_or_default();
    let lasers = ts2::laser::Lasers::new(random_table.clone().unwrap_or_default());
    let particles = ts2::particles::Particles::new(game_exe.as_ref(), random_table.clone().unwrap_or_default());
    let hud_sprites = game_exe.as_ref().zip(exe::level_number(level_path)).map(|(e, lv)| ts2::hud::sprite_table(e, lv)).unwrap_or_default();
    match start {
        Some(s) => println!("player start {:?}, facing {:.0} degrees (F to play, Shift+F to drop at camera)", s.pos, s.yaw_radians().to_degrees()),
        None => println!("no player start found (toy2.exe not next to data\\, or not a main level file)"),
    }
    let mut viewer = Viewer {
        backdrop,
        backdrop_image,
        level,
        level_decal,
        level_two_sided,
        level_far_two_sided,
        level_far,
        billboards: [near_bb, far_bb],
        world,
        player: None,
        player_creature,
        start,
        buzz,
        audio,
        // Buzz's model faces -Z in game space; confirmed in play testing (two R presses).
        player_yaw_offset: std::f32::consts::PI,
        anim_preview: None,
        jump_pressed: false,
        collision_batch,
        random_table,
        poles,
        ledges,
        show_collision: false,
        stomp_pressed: false,
        ngn,
        anims,
        gfx: None,
        cam: Camera { pos: Vec3::ZERO, yaw: 0.0, pitch: 0.0 },
        orig_cam: None,
        abilities_shown: u8::MAX,
        use_orig_cam: true,
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
        pickups,
        specials,
        boxes,
        script,
        creatures,
        jingles: Vec::new(),
        hud: String::new(),
        fire_pressed: false,
        fire_held: false,
        lasers,
        beam_sprite,
        laser_pose,
        hud_state: Default::default(),
        hud_sprites,
        hud_last: [0; 3],
        particles,
        rewards: ts2::rewards::Rewards::new(&level_lists),
        // Hook points (level.dat list 59) for the grappling hook's targets.
        look: {
            let mut fp = ts2::look::FirstPerson::default();
            fp.hooks = dat_lists.iter().find(|l| l.kind == 59).map(|l| l.points.clone()).unwrap_or_default();
            fp
        },
        look_pressed: false,
        r1_pressed: false,
        talk_jump: false,
        talk_square: false,
        jump_blocked: false,
        fp_mouse: [0.0; 2],
        token_seq: 0,
        token_choice: 0,
        potato_part: 0,
        token_timer: 0,
        menu_nav: 0,
        hud_frames: 0,
        front: None,
        level_lists,
        quit_level: false,
        out_of_lives: false,
        game_dir: level_path.ancestors().nth(3).map(std::path::Path::to_path_buf),
        level_track: level_no.and_then(|lv| ts2::campaign::LEVELS.iter().position(|l| l.internal_id == lv)),
        music_track: level_no.and_then(|lv| ts2::campaign::LEVELS.iter().position(|l| l.internal_id == lv)),
        level_ended: false,
    };
    // The first-person helmet and arms only show in first person.
    for id in ts2::look::MODEL {
        viewer.specials.set_scale(id, [0; 3]);
    }
    Ok(viewer)
}

/// The background, behind everything (screen space, 0..1, y down). The PC version (0x48FB70)
/// clears to the sky colour above, and the ground colour below, a line `horizon` + `height`
/// PS1 lines (of 240) under the view's horizon. Deviation: as on the PS1, the level's picture
/// (packet 36: houses, trees, sky) fills the `height` lines above that line, repeating round
/// the view and turning with it like far scenery.
fn sky_batch(b: &packets::Backdrop, view_proj: Mat4, forward: Vec3, picture: Option<usize>, aspect: f32) -> Vec<Batch> {
    let flat = Vec3::new(forward.x, 0.0, forward.z);
    // Screen x (0..1) and y of a horizontal direction at yaw `a` (radians), if in front.
    let project = |a: f32| {
        let d = Vec3::new(a.sin(), 0.0, a.cos());
        let c = view_proj * d.extend(0.0);
        (c.w > 1e-6).then(|| ((c.x / c.w + 1.0) / 2.0, (1.0 - c.y / c.w) / 2.0))
    };
    let yaw = flat.x.atan2(flat.z);
    let ahead = if flat.length() < 1e-3 { None } else { project(yaw) };
    let Some((x0, horizon)) = ahead else {
        // Straight up or down: all sky or all ground.
        let c = if forward.y > 0.0 { b.sky } else { b.ground };
        return vec![Batch { texture: None, blend: scene::Blend::Opaque, vertices: quad(0.0, 0.0, 1.0, 1.0, [0.0; 4], c) }];
    };
    let top = horizon - 0.5 + b.horizon as f32 / 240.0;
    let line = top + b.height as f32 / 240.0;
    let mut out = Vec::new();
    let mut flat_v = Vec::new();
    let l = line.clamp(0.0, 1.0);
    if l > 0.0 {
        flat_v.extend(quad(0.0, 0.0, 1.0, l, [0.0; 4], b.sky));
    }
    if l < 1.0 {
        flat_v.extend(quad(0.0, l, 1.0, 1.0, [0.0; 4], b.ground));
    }
    out.push(Batch { texture: None, blend: scene::Blend::Opaque, vertices: flat_v });
    if let Some(tex) = picture
        && let Some((x1, _)) = project(yaw + 0.01)
    {
        // Screen widths per radian of turn, and a tile width that repeats a whole number of
        // times round a full turn.
        let k = (x1 - x0) / 0.01;
        let turn = std::f32::consts::TAU * k.abs();
        let want = (line - top) * b.width as f32 / b.height as f32 / aspect;
        let n = (turn / want).round().max(1.0);
        let w = turn / n;
        // Where the tile containing yaw 0 starts, moving with the view.
        let origin = x0 - yaw * k;
        let mut s = origin - ((origin / w).ceil()) * w;
        let mut v = Vec::new();
        while s < 1.0 {
            let (u0, u1) = if k > 0.0 { (0.0, 1.0) } else { (1.0, 0.0) };
            v.extend(quad(s, top, s + w, line, [u0, 0.0, u1, 1.0], [0xFF; 3]));
            s += w;
        }
        out.push(Batch { texture: Some(tex), blend: scene::Blend::Opaque, vertices: v });
    }
    out
}

/// A screen-space rectangle (two triangles) with uv rectangle `uv` (u0, v0, u1, v1) and a
/// flat colour.
fn quad(x0: f32, y0: f32, x1: f32, y1: f32, uv: [f32; 4], c: [u8; 3]) -> Vec<scene::GpuVertex> {
    let colour = [0x80, c[0] / 2, c[1] / 2, c[2] / 2];
    let v = |x: f32, y: f32, u: f32, w: f32| scene::GpuVertex { pos: [x, y, 0.0], uv: [u, w], color: colour };
    vec![v(x0, y0, uv[0], uv[1]), v(x1, y0, uv[2], uv[1]), v(x1, y1, uv[2], uv[3]), v(x0, y0, uv[0], uv[1]), v(x1, y1, uv[2], uv[3]), v(x0, y1, uv[0], uv[3])]
}

/// Left and right gains shared with a playing loop, so its balance can follow its owner.
#[derive(Clone, Default)]
struct Gains(std::sync::Arc<[std::sync::atomic::AtomicU32; 2]>);

impl Gains {
    fn set(&self, l: f32, r: f32) {
        self.0[0].store(l.to_bits(), std::sync::atomic::Ordering::Relaxed);
        self.0[1].store(r.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }
    fn get(&self, k: usize) -> f32 {
        f32::from_bits(self.0[k].load(std::sync::atomic::Ordering::Relaxed))
    }
}

/// A mono sample looped forever as stereo at the current `gains`.
struct Looped {
    data: std::sync::Arc<Vec<i16>>,
    rate: u32,
    /// Output sample index (two per frame).
    at: usize,
    gains: Gains,
}

impl Iterator for Looped {
    type Item = i16;
    fn next(&mut self) -> Option<i16> {
        if self.data.is_empty() {
            return None;
        }
        let s = self.data[(self.at / 2) % self.data.len()] as f32 * self.gains.get(self.at & 1);
        self.at = (self.at + 1) % (self.data.len() * 2);
        Some(s as i16)
    }
}

impl Source for Looped {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}


#[cfg(test)]
mod scenery_tests {
    use super::*;
    #[test]
    fn zone_zero_scenery_is_drawn_and_camera_models_are_separate() {
        let Some(game) = std::env::var_os("TS2_GAME_DIR") else { return };
        for level in [1, 2] {
            let path = ts2::campaign::level(level).unwrap().path(std::path::Path::new(&game));
            let n = ngn::parse(&fs::read(path).unwrap()).unwrap();
            let specials = ts2::specials::Specials::new(&n);
            let dynamic: std::collections::HashSet<_> = specials.instances().collect();
            for id in ts2::look::MODEL {
                let s = specials.get(id).unwrap();
                assert!(dynamic.contains(&(s.layer,s.instance)));
            }
            if level == 1 {
                // Preserve the signed-off house: every old zone-0 mesh is a special.
                for (l,layer) in n.layers.iter().enumerate() {
                    for (i,inst) in layer.instances.iter().enumerate() {
                        assert!(inst.flags & 0xffff != 0 || dynamic.contains(&(l,i)), "unhandled L{l}#{i}");
                    }
                }
            } else {
                assert!(!dynamic.contains(&(0,3)), "house must be static scenery");
                let house = scene::level_batches_where(&n, |l,i,_| l == 0 && i == 3);
                assert!(house.iter().any(|b| !b.vertices.is_empty()));
                let sets = level_sets(&n,&specials);
                // Each triangle of the formerly hidden house survives the static draw pass.
                for b in house {
                    for tri in b.vertices.chunks_exact(3) {
                        assert!(sets.near.iter().chain(&sets.decal).any(|out|
                            out.vertices.chunks_exact(3).any(|t| t.iter().zip(tri).all(|(a,b)| a.pos == b.pos))));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod front_tests {
    use super::*;

    /// The front end's scenes load, and every sprite their screens draw has its texture.
    #[test]
    fn front_scenes_load_with_their_pictures_and_sprites() {
        let Some(game) = std::env::var_os("TS2_GAME_DIR").map(std::path::PathBuf::from) else { return };
        let exe = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
        for (scene, variant, sprites, textures) in [
            (ts2::campaign::FRONT, 0, &[0x32, 0x3E][..], &["bgr40", "bgr44"][..]),
            (ts2::campaign::HUB, 0, &[0x36, 0x37, 0x39, 0x3A][..], &["tex31"][..]),
            // The results screen.
            (ts2::campaign::FRONT, 1, &[0x4C, 0x4E, 0x4F, 0x50, 0x51, 0x52, 0x53][..], &["bgr40", "bgr41", "tex31"][..]),
            // The boss-defeated picture.
            (ts2::campaign::BOSS_DONE, 0, &[][..], &["tex37"][..]),
        ] {
            let v = load(&ts2::campaign::variant_path(&game, scene, variant).to_string_lossy(), None).unwrap();
            let table = ts2::hud::sprite_table(&exe, scene);
            for t in textures {
                assert!(v.ngn.textures.iter().any(|x| x.name.eq_ignore_ascii_case(t)), "scene {scene}: no {t}");
            }
            for &s in sprites {
                let d = table[s].as_ref().unwrap_or_else(|| panic!("scene {scene}: no sprite {s:#x}"));
                let name = format!("tex{:02}", d.texture);
                assert!(v.ngn.textures.iter().any(|x| x.name.eq_ignore_ascii_case(&name)), "scene {scene}: sprite {s:#x} needs {name}");
            }
            if scene == ts2::campaign::HUB {
                assert!(v.level_lists.iter().any(|l| l.kind == 2 && l.points.len() == 48));
            }
            // The menus' select, move and cancel sounds (0x4A37E0 ids 0–2).
            let bank = ts2::audio::SoundBank::load(&exe, &game.join("data/sfx"), scene).unwrap();
            for id in 0..3u16 {
                assert!(bank.sound(id).is_some(), "scene {scene}: no sound {id}");
            }
        }
    }
}

#[cfg(test)]
mod culling_tests {
    use super::*;

    /// Splitting the level by material sidedness keeps every billboard (level 1's carved
    /// headboards are billboards in paper-thin meshes, once dropped with them), and level 2's
    /// two-sided fences are drawn from both sides in the far set too.
    #[test]
    fn billboards_kept_and_two_sided_in_both_sets() {
        let Some(game) = std::env::var_os("TS2_GAME_DIR").map(std::path::PathBuf::from) else { return };
        for level in [1, 2] {
            let path = ts2::campaign::level(level).unwrap().path(&game);
            let n = ngn::parse(&fs::read(path).unwrap()).unwrap();
            let specials = ts2::specials::Specials::new(&n);
            let dynamic: std::collections::HashSet<_> = specials.instances().collect();
            let sets = level_sets(&n, &specials);
            let (_, all_near) = scene::level_parts_where(&n, |l, i, _| l == 0 && !dynamic.contains(&(l, i)));
            let (_, all_far) = scene::level_parts_where(&n, |l, i, _| l > 0 && !dynamic.contains(&(l, i)));
            assert_eq!(sets.near_bb.len(), all_near.len(), "level {level}: near billboards lost");
            assert_eq!(sets.far_bb.len(), all_far.len(), "level {level}: far billboards lost");
            assert!(!sets.two_sided.is_empty() && !sets.far_two_sided.is_empty(), "level {level}: no two-sided materials");
        }
    }
}
