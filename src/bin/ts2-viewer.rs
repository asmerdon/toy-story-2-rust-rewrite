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
/// Deviation (mod): the mods named by `OPENTS2_MODS` and `--mod`, loaded once at start-up.
static MODS: std::sync::OnceLock<Vec<ts2::modding::Mod>> = std::sync::OnceLock::new();

/// Deviation (mod): the player drawn as another creature (`[play_as]`, docs/play-as-mods.md).
/// Buzz's controller, collision and camera are unchanged; only the drawn model differs.
struct PlayAsState {
    /// The substitute, with its textures renamed into the level's texture table.
    creature: ngn::Creature,
    retarget: ts2::modding::Retarget,
    scale: f32,
    /// False: no firing, charging or first-person mode.
    laser: bool,
    mode: ts2::modding::AnimationMode,
    /// Deviation (mod), hop gait: the creature's own animations, and the numbers of its idle
    /// and hop ones.
    anims: Vec<anim::Animation>,
    idle: usize,
    hop: usize,
    /// `hop_height` as a launch-speed factor (height goes as speed², airtime as speed).
    hop_speed: f32,
    /// `touch_damage`: the touch reach in level units (0 = off), from the drawn height.
    touch_reach: i32,
    /// `camera_distance`, as the follow camera's target distance.
    camera_distance: i32,
    /// Seconds in the air (0 on the ground), and a free clock for the idle loop.
    air: f32,
    clock: f32,
}

/// Deviation (mod), hop gait: seconds a low hop and a full jump stay in the air, which the
/// hop animation is stretched over (26 and 47 ticks at 60 Hz, printed by the hop gait test in game.rs).
const HOP_AIR_LOW: f32 = 0.43;
const HOP_AIR_FULL: f32 = 0.78;

/// Deviation (mod): the creature's rest-pose height over Buzz's, from their parts' bounds.
fn height_ratio(buzz: &[Option<ts2::modding::Bounds>], other: &[Option<ts2::modding::Bounds>]) -> f32 {
    let h = |b: &[Option<ts2::modding::Bounds>]| {
        let (lo, hi) = b.iter().flatten().fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x.lo[1]), hi.max(x.hi[1])));
        (hi - lo).max(1.0)
    };
    h(other) / h(buzz)
}

/// Deviation (mod): finds `want` (a creature name) in the current level, else in `model`
/// (relative to the game folder), else in every level's `.ngn`. A creature from another file
/// brings its textures into the level's table, renamed so they cannot clash.
fn build_play_as(ngn: &mut ngn::Ngn, game: Option<&std::path::Path>) -> Option<PlayAsState> {
    let mods = MODS.get()?;
    let (m, pa) = ts2::modding::active_play_as(mods)?;
    println!("play_as: {} from mod {} ({})", pa.creature, m.manifest.id, m.dir.display());
    if !ngn.creatures.iter().any(|c| c.id == 0) {
        println!("play_as: this scene has no Buzz; playing as Buzz");
        return None;
    }
    let by_name = |g: &ngn::Ngn| g.creatures.iter().position(|c| c.name.eq_ignore_ascii_case(&pa.creature));
    let mut source: Option<(ngn::Creature, Option<Vec<ngn::Texture>>)> = None;
    if pa.model.is_none() && let Some(i) = by_name(ngn) {
        source = Some((ngn.creatures[i].clone(), None));
    }
    if source.is_none() {
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        match (&pa.model, game) {
            (Some(model), Some(g)) => files.push(g.join(model)),
            (None, Some(g)) => files.extend((0..=20).flat_map(|n| ["level.ngn", "level1.ngn"].map(|f| g.join(format!("data/level{n:02}/{f}"))))),
            _ => {}
        }
        for f in files {
            if let Some(g) = fs::read(&f).ok().and_then(|b| ngn::parse(&b).ok())
                && let Some(i) = by_name(&g)
            {
                println!("play_as: model from {}", f.display());
                source = Some((g.creatures[i].clone(), Some(g.textures)));
                break;
            }
        }
    }
    let Some((mut creature, textures)) = source else {
        println!("play_as: no creature named {} found; playing as Buzz", pa.creature);
        return None;
    };
    if let Some(textures) = textures {
        // Textures are looked up by name in the level's table: bring the creature's own,
        // under a prefix.
        let mut renamed = Vec::new();
        let mut rename = |name: &mut String| {
            if let Some(t) = textures.iter().find(|t| t.name == *name) {
                let new = format!("mod:{}", t.name);
                if !renamed.contains(&new) {
                    renamed.push(new.clone());
                    ngn.textures.push(ngn::Texture { name: new.clone(), bmp: t.bmp.clone() });
                }
                *name = new;
            }
        };
        creature.nodes.iter_mut().filter_map(|n| n.mesh.as_mut()).flat_map(|m| m.textures.iter_mut()).for_each(&mut rename);
        creature.shared_textures.iter_mut().for_each(&mut rename);
    }
    let bounds = |c: &ngn::Creature| -> Vec<Option<ts2::modding::Bounds>> {
        c.nodes.iter().map(|n| n.mesh.as_ref().and_then(|m| ts2::modding::Bounds::of(m.vertices.iter().map(|v| v.pos)))).collect()
    };
    let buzz = ngn.creatures.iter().find(|c| c.id == 0)?;
    let anims: Vec<anim::Animation> = creature.animations.iter().filter_map(|a| anim::parse(a).ok()).collect();
    let (idle, hop) = (pa.idle.unwrap_or(0), pa.hop.unwrap_or(1));
    if pa.animation == ts2::modding::AnimationMode::Hop && (idle >= anims.len() || hop >= anims.len()) {
        println!("play_as: {} has {} animations; idle {idle} / hop {hop} out of range; playing as Buzz", pa.creature, anims.len());
        return None;
    }
    // Deviation (mod): touch damage reaches as far as Buzz's spin (400), scaled up with the
    // drawn height over Buzz's (the Tin Man at full size is about 2.6 times as tall).
    let touch_reach = if pa.touch_damage { (400.0 * (height_ratio(&bounds(buzz), &bounds(&creature)) * pa.scale).max(1.0)).round() as i32 } else { 0 };
    let retarget = ts2::modding::Retarget::new(&bounds(buzz), &bounds(&creature), &pa.map, &pa.rest, &pa.joints);
    println!("play_as: node map (creature node -> Buzz node): {:?}", retarget.map);
    Some(PlayAsState { creature, retarget, scale: pa.scale, laser: pa.laser, mode: pa.animation, anims, idle, hop, hop_speed: pa.hop_height.max(0.0).sqrt(), touch_reach, camera_distance: (ts2::camera::DISTANCE as f32 * pa.camera_distance.max(0.1)).round() as i32, air: 0.0, clock: 0.0 })
}
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
    // w: the level's colours (0x54DD6C, 1.0 = normal); 0 leaves them alone (the HUD).
    if (u.params.w > 0.0) { shade = shade * u.params.w; }
    // The gamma option (0x4B3740): a straight gain on the colours, 1.0 at the default.
    shade = shade * u.range.w;
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
/// then, after clearing depth, the detailed first set within reach (`DETAIL`), so the detailed set
/// always wins and the low-detail one only fills its gaps (the attic entrance has a face only
/// in the low-detail set). The two sets overlap exactly (coplanar), so drawing both with one
/// depth buffer flickers. The exe's defaults (detail level 2 of the table at 0x508D24) also
/// give the far pass 10000 (`FAR_FROM`), read here earlier as a near clip; drawn that way the
/// gaps showed as holes, so the far pass now starts at 0. View depth in game units.
#[allow(dead_code)]
const FAR_FROM: f32 = 10000.0;
/// 0x508D28, by detail level: (detailed set's reach, special objects' reach, low-detail set's
/// nearest special; 0x4CDD10 → 0x4BC410). The high row is the constants this file started with.
const DETAIL: [(f32, f32, f32); 3] = [(3500.0, 10000.0, 0.0), (6500.0, 10000.0, 0.0), (12000.0, 12000.0, 1000.0)];

/// Game space (units, +Y down, +Z forward) to viewer world space (scaled, +Y up).
fn game_to_world(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], -p[1], -p[2]) * WORLD_SCALE
}

fn world_to_game(p: Vec3) -> [f32; 3] {
    [p.x / WORLD_SCALE, -p.y / WORLD_SCALE, -p.z / WORLD_SCALE]
}

/// `data\\PAD\\PATHnn.BIN` (0x4A3770): (pad, ticks held) pairs of i16, one step per frame
/// of the demo's two ticks (0x52F2D4 = 2).
struct Demo {
    words: Vec<u16>,
    at: i64,
    hold: i32,
    pad: u32,
    acc: f32,
    /// A button was pressed (0x52AD84).
    stop: bool,
}

impl Demo {
    /// 0x49DFE0's replay for one frame: the bits for this frame.
    fn step(&mut self) {
        self.hold -= 1;
        if self.hold < 0 {
            self.hold = self.words.get((self.at + 3) as usize).copied().unwrap_or(0) as i16 as i32;
            self.at += 2;
        }
        self.pad = self.words.get(self.at as usize).copied().unwrap_or(0) as u32;
    }
}

/// The fixed keys behind the rows of `controls::ACTIONS`. Fire and the camera buttons have the
/// original's own defaults (0x414F20); the others are the port's.
const CONTROL_DEFAULTS: [KeyCode; 13] = [KeyCode::KeyW, KeyCode::KeyS, KeyCode::KeyA, KeyCode::KeyD, KeyCode::Space, KeyCode::ControlLeft, KeyCode::KeyE, KeyCode::Delete, KeyCode::PageDown, KeyCode::KeyQ, KeyCode::KeyX, KeyCode::Escape, KeyCode::Backspace];

/// Keys a row can be put on, for saving them by name. 0x49C420 refuses the function keys, the
/// arrows and Escape, so they aren't here.
const BINDABLE: &[KeyCode] = &[
    KeyCode::KeyA, KeyCode::KeyB, KeyCode::KeyC, KeyCode::KeyD, KeyCode::KeyE, KeyCode::KeyF, KeyCode::KeyG, KeyCode::KeyH, KeyCode::KeyI, KeyCode::KeyJ, KeyCode::KeyK, KeyCode::KeyL, KeyCode::KeyM,
    KeyCode::KeyN, KeyCode::KeyO, KeyCode::KeyP, KeyCode::KeyQ, KeyCode::KeyR, KeyCode::KeyS, KeyCode::KeyT, KeyCode::KeyU, KeyCode::KeyV, KeyCode::KeyW, KeyCode::KeyX, KeyCode::KeyY, KeyCode::KeyZ,
    KeyCode::Digit0, KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9,
    KeyCode::Space, KeyCode::Tab, KeyCode::ShiftLeft, KeyCode::ShiftRight, KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::AltLeft, KeyCode::AltRight, KeyCode::Backspace, KeyCode::Delete,
    KeyCode::Insert, KeyCode::Home, KeyCode::End, KeyCode::PageUp, KeyCode::PageDown, KeyCode::Comma, KeyCode::Period, KeyCode::Slash, KeyCode::Semicolon, KeyCode::Quote, KeyCode::BracketLeft, KeyCode::BracketRight,
];

fn controls_path() -> std::path::PathBuf {
    ts2::save::dir().join("controls.txt")
}

/// The saved bindings (one key name a row; `-` for none), or the defaults.
fn load_controls() -> ts2::controls::Binds<KeyCode> {
    let mut b = ts2::controls::Binds::new(CONTROL_DEFAULTS.to_vec());
    let Ok(text) = fs::read_to_string(controls_path()) else { return b };
    for (k, name) in text.lines().enumerate().take(CONTROL_DEFAULTS.len()) {
        b.keys[k] = BINDABLE.iter().chain(CONTROL_DEFAULTS.iter()).copied().find(|c| format!("{c:?}") == name);
    }
    b
}

fn save_controls(b: &ts2::controls::Binds<KeyCode>) {
    let text: String = b.keys.iter().map(|k| k.map_or("-".to_string(), |c| format!("{c:?}")) + "\n").collect();
    if let Err(e) = fs::write(controls_path(), text) {
        eprintln!("controls not saved: {e}");
    }
}

/// A key's name for the controller page: lower case letters and digits only.
fn key_label(c: KeyCode) -> String {
    let s = format!("{c:?}");
    s.trim_start_matches("Key").trim_start_matches("Digit").chars().flat_map(|ch| if ch.is_uppercase() && !s.starts_with("Key") { vec![' ', ch.to_ascii_lowercase()] } else { vec![ch.to_ascii_lowercase()] }).collect::<String>().trim().to_string()
}

/// Sound output: the level's samples, mixed with the original's volume and pan.
struct AudioOut {
    _stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
    bank: ts2::audio::SoundBank,
    music: Option<rodio::Sink>,
    /// The sound level (0x5028C8): 1.0 at the default.
    sfx: f32,
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
/// The HUD's own sounds (owned sounds played with obj 0).
const OWNER_HUD: u32 = u32::MAX;

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
        let (gl, gr) = (gl * self.sfx, gr * self.sfx);
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

    /// An owned sound played with obj 0 (0x49E660 → 0x4A3980 at the table's two volumes):
    /// one loop per owner, kept going while played again each tick.
    fn play_owned_flat(&mut self, id: u16, owner: u32) {
        let Some((_, l, r)) = self.bank.sound(id) else { return };
        let Some(slot) = self.bank.sound_slot(id) else { return };
        let (gl, gr) = self.bank.gains(l, r);
        let (gl, gr) = (gl * self.sfx, gr * self.sfx);
        if !self.bank.is_owned(id) {
            self.buffered(slot, gl, gr);
            return;
        }
        let key = (slot, owner);
        if let Some((s, t, _)) = self.owned.get_mut(&key)
            && !s.empty()
        {
            *t = OWNED_TICKS;
            return;
        }
        let Some(sample) = self.bank.slot(slot) else { return };
        let Ok(sink) = rodio::Sink::try_new(&self.handle) else { return };
        let gains = Gains::default();
        gains.set(gl, gr);
        sink.append(Looped { data: std::sync::Arc::new(sample.data.clone()), rate: sample.rate, at: 0, gains: gains.clone() });
        self.owned.insert(key, (sink, OWNED_TICKS, gains));
    }

    /// A sound with no position (PlaySound with obj 0: 0x4A37E0), at its two table volumes.
    fn play_flat(&mut self, id: u16) {
        let Some((_, l, r)) = self.bank.sound(id) else { return };
        let Some(slot) = self.bank.sound_slot(id) else { return };
        let (gl, gr) = self.bank.gains(l, r);
        let (gl, gr) = (gl * self.sfx, gr * self.sfx);
        self.buffered(slot, gl, gr);
    }

    /// A sample slot played at a position (0x4A3810), as jingle steps do.
    fn play_slot(&mut self, slot: u16, vol: i32, rel: Vec3, right: Vec3, forward: Vec3) {
        let (l, r) = ts2::audio::positional(vol, rel, right, forward);
        let (gl, gr) = self.bank.gains(l, r);
        let (gl, gr) = (gl * self.sfx, gr * self.sfx);
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
    /// The room (instance flags & 0xFF) this batch's instances belong to, or `ANY_ROOM`.
    room: u32,
}

/// A batch drawn whatever rooms are in view.
const ANY_ROOM: u32 = u32::MAX;

/// Levels drawn by room: only the rooms seen from the camera's room through its portals
/// (0x4BC460). The owner tried it on level 1 and chose to keep every other level drawing every
/// room: modern hardware doesn't need it, and only level 4 has been played with it.
const ROOM_CULLED_LEVELS: [usize; 1] = [4];

/// Room-by-room drawing (0x4CDDD0 → 0x4BC460) for a level in `ROOM_CULLED_LEVELS`.
struct RoomCull {
    level: usize,
    portals: scene::Portals,
    /// The last room the camera was in (0x5088AC), used while it's over no room map.
    last: Option<u32>,
    /// The rooms in view this frame (bit per room).
    mask: u64,
    /// Rooms of the near and far billboards, parallel to `Viewer::billboards`.
    billboard_rooms: [Vec<u32>; 2],
    /// Rooms of the five level batch lists, parallel to them (near, decal, two-sided,
    /// far two-sided, far).
    batch_rooms: [Vec<u32>; 5],
}

impl RoomCull {
    fn sees(&self, room: u32) -> bool {
        room == ANY_ROOM || (room < 64 && self.mask & (1 << room) != 0)
    }
}

#[derive(Clone, Copy)]
struct Camera {
    pos: Vec3,
    yaw: f32,
    pitch: f32,
}

impl Camera {
    /// `self` (the last tick's camera) moved `t` of the way to `to`; yaw the short way round.
    fn lerp(&self, to: &Camera, t: f32) -> Camera {
        let tau = std::f32::consts::TAU;
        let dyaw =
            (to.yaw - self.yaw + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
        Camera {
            pos: self.pos.lerp(to.pos, t),
            yaw: self.yaw + dyaw * t,
            pitch: self.pitch + (to.pitch - self.pitch) * t,
        }
    }

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
    /// The textures themselves and their pixels, for the animated ones (0x49B260).
    textures: Vec<wgpu::Texture>,
    images: Vec<bmp::Image>,
    depth: wgpu::TextureView,
    /// Detailed level geometry (first mesh set) and the low-detail set drawn far away.
    level: Vec<GpuBatch>,
    level_decal: Vec<GpuBatch>,
    level_two_sided: Vec<GpuBatch>,
    level_far_two_sided: Vec<GpuBatch>,
    level_far: Vec<GpuBatch>,
    /// Billboards of the near and far sets, this frame.
    billboards: [Vec<GpuBatch>; 2],
    /// The rooms drawn this frame (bit per room): level batches of other rooms are skipped.
    rooms: u64,
    creature: Vec<GpuBatch>,
    /// Special objects of layers 0 and 1, drawn with their layer's set (clipped as it is).
    specials: [Vec<GpuBatch>; 2],
    /// The HUD, in screen space (0..1, y down), drawn last over everything.
    hud: Vec<GpuBatch>,
    /// The background (0x48FB70): cleared to the sky colour, the ground band over it.
    sky: Vec<GpuBatch>,
    clear: [f64; 3],
    /// For textures made after loading (movie frames): the layout and sampler, and the movie's
    /// texture, its bind group and size.
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    movie: Option<(wgpu::Texture, wgpu::BindGroup, [u32; 2])>,
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
    /// The rocket boots were running last frame (their flames go when they stop).
    rocket_on: bool,
    /// The level's rain, if it has any (`Script::rain`).
    rain: Option<ts2::rain::Rain>,
    /// Drawing between ticks (the game runs at 60 ticks a second; a faster display would
    /// otherwise show each tick for an uneven 2 or 3 frames, so steady motion judders).
    interp: Interp,
    /// The original follow camera (play mode) and whether it drives the view (V toggles).
    orig_cam: Option<ts2::camera::Camera>,
    /// The abilities the level's gates were last set up for (0x4A2080).
    abilities_shown: u8,
    use_orig_cam: bool,
    keys: std::collections::HashSet<KeyCode>,
    looking: bool,
    /// The cursor is hidden and held in the window (while playing as Buzz).
    cursor_held: bool,
    focused: bool,
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
    /// Room-by-room drawing, for the levels in `ROOM_CULLED_LEVELS`.
    cull: Option<RoomCull>,
    world: game::World,
    player: Option<game::Player>,
    /// Index of BUZZ (id 0) in `ngn.creatures`, if present.
    player_creature: Option<usize>,
    /// Original player start for this level, from the user's toy2.exe.
    start: Option<exe::PlayerStart>,
    /// Animation indices for idle, run and jump. Placeholders until the real ones are identified.
    /// Buzz's move table from toy2.exe; drives his animations in play mode.
    buzz: Option<buzz::BuzzAnimator>,
    /// Deviation (mod): draw the player as another creature.
    play_as: Option<PlayAsState>,
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
    /// Lens flares: 0x4F72D8, the level's lamps, and the graphics option (0x508D70 bit 0).
    flare_table: Vec<[i16; 6]>,
    lamps: ts2::flares::Lamps,
    /// The light on Buzz (0x830D60).
    lights: ts2::lights::Lights,
    lens_flare: bool,
    /// Escape in a level, and the level's end, for the front end to act on.
    quit_level: bool,
    /// The pause menu (0x52B818); Start seen this frame; close the window when set.
    pause: Option<ts2::pause::Pause>,
    pause_pressed: bool,
    pause_acc: f32,
    quit_app: bool,
    /// 0x52F0CD / 0x52F0CE and 0x52F0CC bit 0x40.
    volumes: (i32, i32),
    active_camera: bool,
    /// The detail and gamma options (0x508D74, 0x508D78 in steps).
    detail: i32,
    gamma: i32,
    /// The animated textures option (0x508D70 bit 2).
    animated_textures: bool,
    controls: ts2::controls::Binds<KeyCode>,
    /// The attract demo's recorded pad (0x52B820): words, read position, ticks left, last bits.
    demo: Option<Demo>,
    /// Loaded by the front end for the scene about to start.
    demo_file: Option<((), Demo)>,
    level_ended: bool,
    out_of_lives: bool,
    /// The game folder, the level's own music track (its campaign index) and the track playing.
    game_dir: Option<std::path::PathBuf>,
    level_track: Option<usize>,
    music_track: Option<usize>,
    /// The level's script calls 0x49EAC0 (boss levels don't, keeping their own track).
    theme_events: bool,
    /// The movie playing, those waiting, and a skip pressed.
    movie: Option<Movie>,
    movies: std::collections::VecDeque<usize>,
    movie_skip: bool,
}

/// A movie playing (`ts2::fmv`): frames from the decoder thread, the frame rate, when its
/// first frame showed, frames shown so far, and its sound.
struct Movie {
    frames: std::sync::mpsc::Receiver<Option<ts2::fmv::Frame>>,
    rate: f32,
    start: Option<Instant>,
    shown: u64,
    sound: Option<rodio::Sink>,
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
    level_sets_in(ngn, specials, None)
}

/// The level's batch lists room by room (instance flags & 0xFF, 0x4C3740's +0x84), each list
/// with the room of every batch in it: (near, decal, two-sided, far two-sided, far) and the
/// near and far billboards.
fn level_sets_by_room(ngn: &ngn::Ngn, specials: &ts2::specials::Specials) -> (LevelSets, [Vec<u32>; 5], [Vec<u32>; 2]) {
    let rooms: std::collections::BTreeSet<u32> = ngn.layers.iter().flat_map(|l| l.instances.iter().map(|i| i.flags & 0xFF)).collect();
    let mut all = LevelSets { near: Vec::new(), two_sided: Vec::new(), decal: Vec::new(), far: Vec::new(), far_two_sided: Vec::new(), near_bb: Vec::new(), far_bb: Vec::new() };
    let (mut tags, mut bb_tags): ([Vec<u32>; 5], [Vec<u32>; 2]) = Default::default();
    for r in rooms {
        let mut s = level_sets_in(ngn, specials, Some(r));
        for (list, tag) in [(&mut all.near, &mut s.near), (&mut all.decal, &mut s.decal), (&mut all.two_sided, &mut s.two_sided), (&mut all.far_two_sided, &mut s.far_two_sided), (&mut all.far, &mut s.far)].into_iter().zip(tags.iter_mut()) {
            tag.extend(std::iter::repeat_n(r, list.1.len()));
            list.0.append(list.1);
        }
        bb_tags[0].extend(std::iter::repeat_n(r, s.near_bb.len()));
        bb_tags[1].extend(std::iter::repeat_n(r, s.far_bb.len()));
        all.near_bb.append(&mut s.near_bb);
        all.far_bb.append(&mut s.far_bb);
    }
    (all, tags, bb_tags)
}

/// `level_sets` for the instances of one room only, or all of them.
fn level_sets_in(ngn: &ngn::Ngn, specials: &ts2::specials::Specials, room: Option<u32>) -> LevelSets {
    let dynamic: std::collections::HashSet<(usize, usize)> = specials.instances().collect();
    let keep = |l: usize, n: usize, i: &ngn::Instance| !dynamic.contains(&(l, n)) && room.is_none_or(|r| i.flags & 0xFF == r);
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
    let [near, two_sided, decal, far, far_two_sided] = [near, two_sided, decal, far, far_two_sided].map(solid_blends);
    near_bb.append(&mut decal_bb);
    LevelSets { near, two_sided, decal, far, far_two_sided, near_bb, far_bb }
}

/// Blended (mode 0x02) level triangles that are wholly opaque, vertex alpha 0xFF over a
/// colour-keyed texture (the level-2 tree's leaves and bushes), drawn as alpha-tested
/// opaque ones that write depth. Unsorted and without depth writes, whichever face of a
/// two-sided bush was drawn last showed, its back over its front and a far bush over a near
/// one, where the original shows the near faces. Triangles with any see-through vertex stay
/// blended.
fn solid_blends(batches: Vec<Batch>) -> Vec<Batch> {
    let mut out = Vec::new();
    for b in batches {
        if b.blend != scene::Blend::Alpha {
            scene::merge(&mut out, vec![b]);
            continue;
        }
        for t in b.vertices.chunks_exact(3) {
            let solid = t.iter().all(|v| v.color[0] == 0xFF);
            scene::push(&mut out, b.texture, if solid { scene::Blend::Opaque } else { b.blend }, [t[0], t[1], t[2]]);
        }
    }
    out
}

/// Instance flag 0x80000 (meaning unknown; 288 ordinary instances in level 1). They're kept in
/// their own list with a depth nudge, tried against the attic boxes' flicker; that turned out
/// to be black back faces fighting the lit fronts, now culled, so the nudge is 0.
const DECAL: u32 = 0x80000;
/// The nudge: clip depth × (1 + this), with reversed depth (larger is nearer).
const DECAL_BIAS: f32 = 0.0;

/// The rooms in view this frame (0x440F70 → 0x4CDDD0 → 0x4BC460), as a bit mask; every
/// room for levels not drawn by room. The camera's room is the room map's under it
/// (0x4885C0), or the last one while it's over none.
fn update_rooms(cull: &mut Option<RoomCull>, world: &game::World, cam: &Camera, aspect: f32) -> u64 {
    let Some(c) = cull.as_mut() else { return u64::MAX };
    let eye = Vec3::from(world_to_game(cam.pos));
    let mut room = world.room_at(eye).map(u32::from).filter(|&r| r < 64);
    // 0x440F70, level 4: in the paint room's map but higher than −7900 (over its roof),
    // the camera counts as outside (room 1).
    if c.level == 4 && room == Some(2) && eye.y < -7900.0 {
        room = Some(1);
    }
    if room.is_some() {
        c.last = room;
    }
    c.mask = match c.last {
        // No room yet, or no portals: everything (0x4BC720's path).
        Some(r) if !c.portals.links.is_empty() => {
            let vp = Mat4::perspective_infinite_reverse_rh(60f32.to_radians(), aspect, 0.05) * cam.view();
            c.portals.visible_rooms(r, |p| vp * game_to_world(p).extend(1.0), c.level == 10)
        }
        _ => u64::MAX,
    };
    c.mask
}

/// Whether a special object is drawn this frame, by 0x4BC460 / 0x4BC720's per-instance tests:
/// its room is in view (`rooms`, all bits set on levels not drawn by room), and its centre is
/// within `NEAR_CULL` of the eye for the detailed set (layer 0, against 0x5088B0) or beyond
/// `FAR_SKIP` for the low-detail set (layer 1, against 0x5088B4). The exe treats special
/// objects as ordinary instances, so these hold on every level.
fn special_in_view(ngn: &ngn::Ngn, s: &ts2::specials::Special, rooms: u64, eye: Vec3, detail: usize) -> bool {
    let Some(inst) = ngn.layers.get(s.layer).and_then(|l| l.instances.get(s.instance)) else { return true };
    let room = inst.flags & 0xFF;
    if room < 64 && rooms & (1 << room) == 0 {
        return false;
    }
    let Some(mesh) = ngn.layers[s.layer].meshes.get(inst.mesh as usize) else { return true };
    let placed = ngn::Instance { pos: s.pos, rot: s.angles, scale: s.scale, ..*inst };
    let d2 = Vec3::from(scene::instance_centre(mesh, &placed)).distance_squared(eye);
    let (_, near, far) = DETAIL[detail];
    if s.layer == 0 { d2 < near * near } else { d2 > far * far }
}

/// The camera a script's look-at scene (0x4020F0) or conversation holds, if one is on.
fn scene_camera(script: Option<&dyn ts2::script::Script>) -> Option<Camera> {
    let l = script?;
    let (from, at) = match (l.look, l.cutscene.as_ref()) {
        (Some(lk), _) => (lk.from, lk.at),
        (None, Some(c)) => c.camera(),
        (None, None) => return None,
    };
    let p = Vec3::from(from.map(|v| v as f32 / 32.0));
    let q = Vec3::from(at.map(|v| v as f32 / 32.0));
    let g = (q - p).normalize_or_zero();
    let w = Vec3::new(g.x, -g.y, -g.z).normalize_or_zero();
    Some(Camera {
        pos: game_to_world(p.to_array()),
        yaw: w.x.atan2(-w.z),
        pitch: w.y.clamp(-1.0, 1.0).asin(),
    })
}

/// Drawing between game ticks: where things were at the previous tick, and how far (0..1)
/// the next tick has come. Moves longer than a jump (a teleport, a respawn) aren't blended.
/// The view from the follow camera (game space to the renderer's).
fn follow_view(c: &ts2::camera::Camera) -> Camera {
    let g = c.forward();
    let w = Vec3::new(g.x, -g.y, -g.z).normalize_or_zero();
    Camera { pos: game_to_world(c.position().to_array()), yaw: w.x.atan2(-w.z), pitch: w.y.clamp(-1.0, 1.0).asin() }
}

#[derive(Default)]
struct Interp {
    alpha: f32,
    buzz_prev: Option<Vec3>,
    creatures_prev: Vec<[i32; 3]>,
    /// Special objects' positions (level units) at the previous tick: a lift drawn where the
    /// last tick left it judders against the camera, which is drawn between ticks.
    specials_prev: std::collections::HashMap<u16, [f32; 3]>,
    /// The camera at the previous and the latest tick.
    cam: Option<(Camera, Camera)>,
}

impl Interp {
    /// Level units.
    fn buzz(&self, now: Vec3) -> Vec3 {
        match self.buzz_prev {
            Some(p) if p.distance(now) < 256.0 => p.lerp(now, self.alpha),
            _ => now,
        }
    }

    /// Special `id` at `now` (level units).
    fn special(&self, id: u16, now: [f32; 3]) -> [f32; 3] {
        let now_v = Vec3::from(now);
        match self.specials_prev.get(&id).map(|p| Vec3::from(*p)) {
            Some(p) if p.distance(now_v) < 256.0 => p.lerp(now_v, self.alpha).to_array(),
            _ => now,
        }
    }

    /// Slot `i` at `now` (object units), in level units.
    fn creature(&self, i: usize, now: [i32; 3]) -> [f32; 3] {
        let now_f = Vec3::from(now.map(|v| v as f32 / 32.0));
        match self
            .creatures_prev
            .get(i)
            .map(|p| Vec3::from(p.map(|v| v as f32 / 32.0)))
        {
            Some(p) if p.distance(now_f) < 256.0 => p.lerp(now_f, self.alpha).to_array(),
            _ => now_f.to_array(),
        }
    }
}

fn upload(device: &wgpu::Device, batches: &[Batch], white: usize) -> Vec<GpuBatch> {
    upload_rooms(device, batches, &[], white)
}

/// As `upload`, each batch tagged with its room from `rooms` (parallel; `ANY_ROOM` past its end).
fn upload_rooms(device: &wgpu::Device, batches: &[Batch], rooms: &[u32], white: usize) -> Vec<GpuBatch> {
    batches
        .iter()
        .enumerate()
        .filter(|(_, b)| !b.vertices.is_empty())
        .map(|(i, b)| GpuBatch {
            texture: b.texture.unwrap_or(white),
            blend: b.blend,
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&b.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            count: b.vertices.len() as u32,
            room: rooms.get(i).copied().unwrap_or(ANY_ROOM),
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
        let mut textures = Vec::new();
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
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    &img.rgba,
                );
                textures.push(tex.clone());
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
        Gfx { textures, images, window, surface, target, device, queue, config, pipelines, uniform_bufs, uniform_bgs, texture_bgs, depth, level, level_decal: Vec::new(), level_two_sided: Vec::new(), level_far_two_sided: Vec::new(), level_far, billboards: [Vec::new(), Vec::new()], creature: Vec::new(), specials: [Vec::new(), Vec::new()], hud: Vec::new(), sky: Vec::new(), clear: [0.05, 0.06, 0.1], texture_layout, sampler, movie: None, rooms: u64::MAX }
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

    /// Puts a movie frame in the movie texture (made, or remade, to the frame's size).
    fn movie_frame(&mut self, f: &ts2::fmv::Frame) {
        let size = [f.width, f.height];
        if self.movie.as_ref().is_none_or(|m| m.2 != size) {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("movie"),
                size: wgpu::Extent3d { width: f.width, height: f.height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.texture_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            });
            self.movie = Some((tex, bg, size));
        }
        if let Some((tex, _, _)) = &self.movie {
            self.queue.write_texture(
                tex.as_image_copy(),
                &f.rgba,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(f.width * 4), rows_per_image: Some(f.height) },
                wgpu::Extent3d { width: f.width, height: f.height, depth_or_array_layers: 1 },
            );
        }
    }

    /// 0x4AFD30: a tile copied within texture `index`.
    fn blit_texture(&mut self, index: usize, b: &ts2::texanim::Blit) {
        let (Some(tex), Some(img)) = (self.textures.get(index), self.images.get_mut(index)) else { return };
        let Some(px) = b.pixels(&img.rgba, img.width, img.height) else { return };
        let [w, h] = b.size.map(|v| v as u32);
        let [x, y] = b.dst.map(|v| v as u32);
        if x + w > img.width || y + h > img.height {
            return;
        }
        for row in 0..h {
            let from = (row * w * 4) as usize;
            let to = (((y + row) * img.width + x) * 4) as usize;
            img.rgba[to..to + (w * 4) as usize].copy_from_slice(&px[from..from + (w * 4) as usize]);
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
            &px,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: None },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    }

    /// Draws the movie frame alone, as large as fits, on black.
    fn render_movie(&mut self) {
        let Some((_, bg, [w, h])) = &self.movie else { return };
        let frame = match &self.surface {
            Some(surface) => match surface.get_current_texture() {
                Ok(f) => Some(f),
                Err(_) => {
                    surface.configure(&self.device, &self.config);
                    return;
                }
            },
            None => None,
        };
        let view = match (&frame, &self.target) {
            (Some(f), _) => f.texture.create_view(&Default::default()),
            (None, Some(t)) => t.create_view(&Default::default()),
            (None, None) => return,
        };
        // Fit the picture's aspect inside the window's (screen space 0..1, y down).
        let (sw, sh) = (self.config.width as f32, self.config.height as f32);
        let scale = (sw / *w as f32).min(sh / *h as f32);
        let (qw, qh) = (*w as f32 * scale / sw, *h as f32 * scale / sh);
        let (x0, y0) = ((1.0 - qw) / 2.0, (1.0 - qh) / 2.0);
        let c = |x: f32, y: f32, u: f32, v: f32| GpuVertex { pos: [x, y, 0.0], uv: [u, v], color: [0xFF, 0x80, 0x80, 0x80] };
        let verts = [c(x0, y0, 0.0, 0.0), c(x0 + qw, y0, 1.0, 0.0), c(x0 + qw, y0 + qh, 1.0, 1.0), c(x0, y0, 0.0, 0.0), c(x0 + qw, y0 + qh, 1.0, 1.0), c(x0, y0 + qh, 0.0, 1.0)];
        let buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&verts), usage: wgpu::BufferUsages::VERTEX });
        let hud = Uniforms {
            view_proj: Mat4::from_cols(glam::Vec4::new(2.0, 0.0, 0.0, 0.0), glam::Vec4::new(0.0, 2.0, 0.0, 0.0), glam::Vec4::ZERO, glam::Vec4::new(-1.0, 1.0, 0.5, 1.0)).to_cols_array_2d(),
            params: [1.0, 0.0, 1.0, 0.0],
            range: [0.0, f32::MAX, 0.0, 1.0],
        };
        self.queue.write_buffer(&self.uniform_bufs[3], 0, bytemuck::bytes_of(&hud));
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipelines[0]);
            pass.set_bind_group(0, &self.uniform_bgs[3], &[]);
            pass.set_bind_group(1, bg, &[]);
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..6, 0..1);
        }
        self.queue.submit([enc.finish()]);
        if let Some(f) = frame {
            f.present();
        }
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
        let ranges = [[0.0, f32::MAX], [0.0, uniforms.range[1] * s], [0.0, f32::MAX]];
        for (buf, r) in self.uniform_bufs.iter().zip(ranges) {
            let u = Uniforms { range: [r[0], r[1], 0.0, uniforms.range[3]], ..*uniforms };
            self.queue.write_buffer(buf, 0, bytemuck::bytes_of(&u));
        }
        let decal = Uniforms { range: [0.0, uniforms.range[1] * s, DECAL_BIAS, uniforms.range[3]], ..*uniforms };
        self.queue.write_buffer(&self.uniform_bufs[4], 0, bytemuck::bytes_of(&decal));
        // The HUD: screen (x, y) in 0..1, y down, straight to clip space (the shader negates y).
        let hud = Uniforms {
            view_proj: Mat4::from_cols(glam::Vec4::new(2.0, 0.0, 0.0, 0.0), glam::Vec4::new(0.0, 2.0, 0.0, 0.0), glam::Vec4::ZERO, glam::Vec4::new(-1.0, 1.0, 0.5, 1.0)).to_cols_array_2d(),
            params: [1.0, 0.0, 1.0, 0.0],
            range: [0.0, f32::MAX, 0.0, uniforms.range[3]],
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
            (&[(0, &self.level_far, true), (0, &self.level_far_two_sided, false), (0, &self.billboards[1], false), (0, &self.specials[1], false)], false),
            (&[(1, &self.level, true), (1, &self.level_two_sided, false), (4, &self.level_decal, true), (1, &self.billboards[0], false), (1, &self.specials[0], false), (2, &self.creature, false)], false),
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
                    for b in batches.iter().filter(|b| b.blend == *mode && (b.room == ANY_ROOM || (b.room < 64 && self.rooms & (1 << b.room) != 0))) {
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

    /// The controller page waiting for a key (0x49C420): takes `raw`, true if it did.
    fn capture_key(&mut self, raw: KeyCode) -> bool {
        let Some(row) = self.front.as_ref().and_then(|f| f.state.capturing()) else { return false };
        if raw == KeyCode::Escape {
            if let Some(f) = self.front.as_mut() {
                f.state.capture_cancelled();
            }
            return true;
        }
        if !BINDABLE.contains(&raw) {
            return true;
        }
        self.controls.bind(row, raw);
        let names = self.controls.keys.iter().map(|k| k.map_or(String::new(), key_label)).collect();
        if let Some(f) = self.front.as_mut() {
            f.state.capture_done(names);
        }
        true
    }

    /// 0x49F4B0: the pause menu's tick; `true` keeps it open.
    fn pause_update(&mut self, dt: f32) {
        use ts2::frontend::Pad;
        let Some(p) = self.pause.as_mut() else { return };
        let bits = [
            (&[KeyCode::ArrowUp, KeyCode::KeyW][..], Pad::UP),
            (&[KeyCode::ArrowDown, KeyCode::KeyS], Pad::DOWN),
            (&[KeyCode::ArrowLeft, KeyCode::KeyA], Pad::LEFT),
            (&[KeyCode::ArrowRight, KeyCode::KeyD], Pad::RIGHT),
            (&[KeyCode::Space, KeyCode::Enter, KeyCode::NumpadEnter], Pad::JUMP),
            (&[KeyCode::Backspace], Pad::CANCEL),
            (&[KeyCode::Escape, KeyCode::F2], Pad::START),
        ];
        let pad = Pad(bits.iter().filter(|(keys, _)| keys.iter().any(|k| self.keys.contains(k))).fold(0, |a, (_, b)| a | b));
        self.pause_acc += dt;
        let mut out = Vec::new();
        while self.pause_acc >= 1.0 / 60.0 {
            self.pause_acc -= 1.0 / 60.0;
            out.extend(p.tick(pad));
        }
        for o in out {
            use ts2::pause::Out;
            match o {
                Out::Sound(id) => {
                    if let Some(a) = self.audio.as_mut() {
                        a.play_flat(id);
                    }
                }
                Out::Resume => {
                    self.pause = None;
                    if let Some(m) = self.audio.as_ref().and_then(|a| a.music.as_ref()) {
                        m.play();
                    }
                    self.last = Instant::now();
                    return;
                }
                Out::ExitLevel => {
                    self.pause = None;
                    if let Some(m) = self.audio.as_ref().and_then(|a| a.music.as_ref()) {
                        m.play();
                    }
                    if self.front.is_some() {
                        self.level_ended = true;
                    } else {
                        self.quit_app = true;
                    }
                    return;
                }
                Out::Camera(active) => self.active_camera = active,
                Out::Volumes { music, sfx } => {
                    self.volumes = (music, sfx);
                    self.apply_volumes();
                }
            }
        }
    }

    /// 0x4A3ED0: the music at table 0x5028A8 × 2 / 3, the sounds at 0x5028C8 × 3 / 2; level 8
    /// (the defaults) is the volume the port has always played at.
    fn apply_volumes(&mut self) {
        const MUSIC: [i32; 11] = [0, 9, 18, 27, 36, 45, 54, 63, 72, 81, 90];
        const SFX: [i32; 11] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 72, 80];
        let (m, s) = self.volumes;
        if let Some(a) = self.audio.as_mut() {
            a.sfx = (SFX[s.clamp(0, 10) as usize] * 3 / 2) as f32 / 96.0;
            if let Some(music) = a.music.as_ref()
                && music.volume() > 0.0
            {
                music.set_volume((MUSIC[m.clamp(0, 10) as usize] * 2 / 3) as f32 / 96.0);
            }
        }
    }

    fn update(&mut self, dt: f32) {
        if self.movie.is_some() || !self.movies.is_empty() {
            self.movie_update();
            return;
        }
        // Start (fresh): 0x49DFE0 pauses unless a level is ending.
        if std::mem::take(&mut self.pause_pressed) && self.pause.is_none() && self.player.is_some() && !self.level_ended && !self.out_of_lives && self.demo.is_none() {
            self.pause = Some(ts2::pause::Pause::new(self.volumes.0, self.volumes.1));
            if let Some(a) = self.audio.as_mut() {
                a.play_flat(0x3D);
                if let Some(m) = a.music.as_ref() {
                    m.pause();
                }
            }
        }
        if self.pause.is_some() {
            self.pause_update(dt);
        }
        // The attract demo: its pad bits drive the keys, one step per two ticks.
        if let Some(mut d) = self.demo.take() {
            d.acc += dt;
            while d.acc >= 1.0 / 30.0 {
                d.acc -= 1.0 / 30.0;
                d.step();
            }
            for (bit, key) in [(0x10, KeyCode::KeyW), (0x40, KeyCode::KeyS), (0x80, KeyCode::KeyA), (0x20, KeyCode::KeyD), (0x4000, KeyCode::Space), (0x2000, KeyCode::KeyE)] {
                let on = d.pad & bit != 0;
                if on && !self.keys.contains(&key) {
                    match key {
                        KeyCode::Space => self.jump_pressed = true,
                        KeyCode::KeyE => self.stomp_pressed = true,
                        _ => {}
                    }
                }
                if on {
                    self.keys.insert(key);
                } else {
                    self.keys.remove(&key);
                }
            }
            let held = d.pad & 0x8000 != 0;
            if held && !self.fire_held {
                self.fire_pressed = true;
            }
            self.fire_held = held;
            let stop = d.stop || std::mem::take(&mut self.level_ended) || std::mem::take(&mut self.out_of_lives);
            if stop {
                self.keys.clear();
                self.fire_held = false;
                self.player = None;
                if let Some(f) = self.front.as_mut() {
                    f.state.demo_done();
                }
            } else {
                self.demo = Some(d);
            }
        }
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
        if let Some(player) = self.player.as_mut().filter(|_| self.pause.is_none()) {
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
            let no_laser = self.play_as.as_ref().is_some_and(|p| !p.laser);
            // Deviation (mod): the bounce gait (`animation = "hop"`) is a Player flag; the
            // viewer only times the hop animation over the airtime.
            if let Some(pa) = self.play_as.as_mut() {
                player.hop_gait = pa.mode == ts2::modding::AnimationMode::Hop;
                player.hop_vy = (game::HOP_VY as f32 * pa.hop_speed).round() as i32;
                player.touch_reach = pa.touch_reach;
                pa.clock += dt;
                pa.air = if player.on_floor { 0.0 } else { pa.air + dt };
            }
            let input = game::Input {
                move_xz,
                jump: free && std::mem::take(&mut self.jump_pressed),
                jump_held: free && self.keys.contains(&KeyCode::Space),
                stomp: !talking && std::mem::take(&mut self.stomp_pressed),
                stomp_held: !talking && self.keys.contains(&KeyCode::KeyE),
                // Deviation (mod): a play-as mod with `laser = false` has no firing, charging
                // or first-person mode.
                fire: !talking && std::mem::take(&mut self.fire_pressed) && !no_laser,
                fire_held: !talking && self.fire_held && !no_laser,
                look: !talking && player.first_person == 0 && self.look_pressed && !no_laser,
                dpad: [(KeyCode::KeyW, game::pad::UP), (KeyCode::KeyS, game::pad::DOWN), (KeyCode::KeyA, game::pad::LEFT), (KeyCode::KeyD, game::pad::RIGHT)]
                    .iter()
                    .filter(|(k, _)| self.keys.contains(k))
                    .fold(0, |acc, (_, b)| acc | b),
            };
            // Boxes and the level script run inside each tick, just before Buzz moves (0x433700
            // is called from 0x436220 there).
            let (boxes, specials, script) = (&mut self.boxes, &mut self.specials, &mut self.script);
            // What the last tick started from, however many ticks this frame runs: blending
            // from further back makes a slow frame jump backwards.
            let mut buzz_before = player.pos;
            let mut specials_before: std::collections::HashMap<u16, [f32; 3]> = std::collections::HashMap::new();
            // Ticks before the last one run the camera here, on Buzz as that tick left him; the
            // last runs below, with its focus.
            let mut ran = 0;
            let (orig_cam, follow) = (&mut self.orig_cam, self.use_orig_cam);
            if let Some(c) = orig_cam.as_mut() {
                c.passive = !self.active_camera;
                c.pad = [(KeyCode::KeyW, 0x10), (KeyCode::KeyS, 0x40), (KeyCode::KeyA, 0x80), (KeyCode::KeyD, 0x20), (KeyCode::Delete, 0x100), (KeyCode::PageDown, 0x200), (KeyCode::Backspace, 0x1000)]
                    .iter()
                    .filter(|(k, _)| self.keys.contains(k))
                    .fold(0, |a, (_, b)| a | b);
            }
            if let (Some(l), Some(c)) = (script.as_mut(), orig_cam.as_ref()) {
                l.view = (c.pos, c.look_yaw);
            }
            let ticks = player.update_with(&self.world, input, dt, &mut |p, w| {
                buzz_before = p.pos;
                specials_before = specials.objects.iter().map(|(&id, s)| (id, s.pos)).collect();
                if ran > 0 {
                    if let Some(c) = orig_cam.as_mut().filter(|_| follow && p.first_person == 0) {
                        c.tick(p, w);
                        let t = std::mem::take(&mut c.turn_buzz);
                        if t != 0 {
                            p.set_facing((p.facing + t) & 0xFFF);
                        }
                    }
                }
                ran += 1;
                if let Some(b) = boxes.as_mut() {
                    b.tick(p, w, specials);
                    if let Some(l) = script.as_mut() {
                        l.step(w, specials, p, b);
                    }
                }
            });
            if ticks > 0 {
                self.interp.buzz_prev = Some(buzz_before);
                self.interp.specials_prev = specials_before;
            }
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
            // A script's look-at scene (0x4020F0) takes the camera and ends first person.
            if player.first_person != 0 && self.script.as_ref().is_some_and(|l| l.look.is_some()) {
                self.look.cancel(player, &mut self.particles, &mut self.specials);
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
            // The camera as the tick before the last left it, when this frame ran several.
            let mut cam_before = None;
            if player.first_person == 0
                && let Some(c) = self.orig_cam.as_mut()
            {
                c.focus = self.creatures.as_mut().and_then(|cr| cr.boss.focus.take());
                if ticks > 0 {
                    if ticks > 1 {
                        cam_before = Some(c.clone());
                    }
                    // Deviation (mod): a play-as mod's `camera_distance`.
                    c.target_distance = self.play_as.as_ref().map_or(ts2::camera::DISTANCE, |p| p.camera_distance);
                    c.tick(player, &self.world);
                    let t = std::mem::take(&mut c.turn_buzz);
                    if t != 0 {
                        player.set_facing((player.facing + t) & 0xFFF);
                    }
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
            match self
                .orig_cam
                .as_ref()
                .filter(|_| self.use_orig_cam && player.first_person == 0)
            {
                _ if let Some(c) = scene_camera(self.script.as_deref()) => self.cam = c,
                _ if player.first_person != 0 => {
                    let e = ts2::look::FirstPerson::eye(player);
                    self.cam.pos = game_to_world(e.map(|v| v as f32 / 32.0));
                    let g = self.look.forward();
                    let w = Vec3::new(g.x, -g.y, -g.z).normalize_or_zero();
                    self.cam.yaw = w.x.atan2(-w.z);
                    self.cam.pitch = w.y.clamp(-1.0, 1.0).asin();
                }
                Some(c) => self.cam = follow_view(c),
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
                // Discs (0x4A4960): at most 6 in flight (0x4A4910). Homing (0x47) at the
                // nearest creature it can hit within 4096 units, or in first person the one
                // targeted; else straight (0x48) at a third of full speed along the aim.
                let disc_shots: Vec<game::Shot> = player.disc_shots.drain(..).collect();
                for shot in disc_shots {
                    if self.particles.discs_in_flight() >= 6 {
                        continue;
                    }
                    let a = shot.yaw as f32 / 4096.0 * std::f32::consts::TAU;
                    let muzzle = [
                        shot.origin[0] + (a.sin() * 90.0 * 32.0) as i32,
                        shot.origin[1],
                        shot.origin[2] + (a.cos() * 90.0 * 32.0) as i32,
                    ];
                    let target = if shot.first_person {
                        self.look.target
                    } else {
                        cr.disc_target(player.pos_obj())
                    };
                    let sin = |x: i32| ts2::script::sin_table(x);
                    match target {
                        Some(t) => {
                            if let Some(k) =
                                self.particles
                                    .create(muzzle, [0, -2, 0], shot.yaw << 2, 0, 0, 0x47)
                            {
                                self.particles.slots[k].target = Some(t);
                            }
                        }
                        None => {
                            let flat = sin(shot.pitch + 0x400);
                            let vel = [
                                (sin(shot.yaw) * flat >> 14) / 3,
                                -sin(shot.pitch) / 3,
                                (sin(shot.yaw + 0x400) * flat >> 14) / 3,
                            ];
                            self.particles.create(muzzle, vel, 0, 0, 0, 0x48);
                        }
                    }
                    sounds.push((0x54, player.pos_obj()));
                    player.disc_ammo -= 1;
                }
                cr.sounds.extend(sounds);
            }
            for (at, t, m, angle) in player.effects.drain(..) {
                if let Some(k) = self.particles.spawn(at, t, m) {
                    self.particles.slots[k].angle = angle & 0xFFF;
                }
            }
            if std::mem::take(&mut player.rocket_lit) {
                self.particles
                    .rocket_flames(player.pos_obj(), player.rocket);
            }
            if player.rocket == 0 && std::mem::replace(&mut self.rocket_on, false) {
                self.particles.end_rocket_flames();
            }
            self.rocket_on |= player.rocket > 0;
            self.particles.buzz_vel = player.vel;
            self.particles.camera = cam_game.map(|v| (v * 32.0) as i32);
            for _ in 0..ticks {
                if let Some(cr) = self.creatures.as_ref() {
                    self.particles.aims = cr.aims();
                }
                self.particles
                    .tick(&self.world, (player.pos_obj(), player.facing));
                if let Some(sprite) = self.script.as_ref().and_then(|l| l.rain) {
                    let rain = self
                        .rain
                        .get_or_insert_with(|| ts2::rain::Rain::new(sprite));
                    let f = self.cam.forward();
                    let yaw = ts2::boxes::angle12(f.x, -f.z);
                    let water = (player.water.0 != 0).then_some(player.water.1);
                    let cam = self.particles.camera;
                    rain.tick(
                        &self.world,
                        &mut self.particles,
                        cam,
                        yaw,
                        player.vel[1],
                        water,
                    );
                }
                self.rewards
                    .tick(self.particles.camera, &mut self.particles);
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
                let hud_status = ts2::hud::Status {
                    lives: player.lives,
                    health: player.health,
                    coins,
                    spin_charge: player.charge,
                    laser: player.laser,
                    laser_charge: player.laser_charge,
                    power: player.power,
                    discs: player.disc_ammo,
                    hooks: player.hook_ammo,
                    sheep: self.creatures.as_ref().map_or(0, |c| c.sheep),
                    race: self.script.as_ref().map_or(0, |l| l.race),
                    laps: self.script.as_ref().map_or(0, |l| l.laps),
                    boss: self.script.as_ref().and_then(|l| l.boss_bar).unwrap_or_else(|| self.creatures.as_ref().map_or(0, |c| c.boss.bar)),
                    boss_on: self.script.as_ref().is_some_and(|l| l.boss_hud),
                    collected: challenge_count(self.script.as_deref(), self.pickups.as_ref()),
                };
                self.hud_state.tick(&hud_status);
                // 0x49FD40: the countdown's tick (owned sound 0x5E, fixed volumes).
                if self.hud_state.countdown_ticking(&hud_status)
                    && let Some(a) = self.audio.as_mut()
                {
                    a.play_owned_flat(0x5E, OWNER_HUD);
                }
                // 0x49EAC0: the miniboss theme while the boss bar shows, the race theme while
                // the laps show with a race on (0x52F2F8 ≥ 2), else the level's own.
                if let Some(level_track) = self.level_track.filter(|_| self.theme_events) {
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
                cr.discs_out = player.disc_ammo != 0 && self.particles.discs_in_flight() > 0;
                cr.buzz_firing = player.laser != 0;
                cr.flares.clear();
                for k in 0..ticks {
                    if k + 1 == ticks {
                        self.interp.creatures_prev = cr.slots.iter().map(|c| c.pos).collect();
                    }
                    cr.tick(&self.world, cam_obj, player.pos_obj());
                    cr.contact(player);
                }
                for r in cr.particles.drain(..) {
                    self.particles.create(r.pos, r.vel, r.gravity, r.angle, r.spin, r.typ);
                }
                for (r, life) in cr.lasting.drain(..) {
                    if let Some(k) = self.particles.create(r.pos, r.vel, r.gravity, r.angle, r.spin, r.typ) {
                        self.particles.slots[k].life = life;
                    }
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
                    for (r, life) in l.lasting.drain(..) {
                        if let Some(k) = self
                            .particles
                            .create(r.pos, r.vel, r.gravity, r.angle, r.spin, r.typ)
                        {
                            self.particles.slots[k].life = life;
                        }
                    }
                    for (at, t, m, vel, spin) in l.kicked.drain(..) {
                        if let Some(k) = self.particles.spawn(at, t, m) {
                            self.particles.slots[k].vel = vel;
                            self.particles.slots[k].spin = spin;
                        }
                    }
                    for (at, t, m, spin, floor) in l.drops.drain(..) {
                        if let Some(k) = self.particles.spawn(at, t, m) {
                            self.particles.slots[k].spin = spin;
                            self.particles.slots[k].floor = Some(floor);
                        }
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
                    let ts2::creatures::Event::Died { pos, coin, centre, burst, slot } = ev;
                    // 0x405D20: fireballs come with a flash.
                    if burst > 0 {
                        self.lights.add((centre, 0xF08000, 0x20, slot as i64));
                    }
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
                        (self.world.start_floor(g), s.angle as i32)
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
            // The light on Buzz (0x49EE50, 0x49EEE0) and what feeds it.
            {
                if let Some(cr) = self.creatures.as_mut() {
                    for f in cr.lights.drain(..) {
                        self.lights.add(f);
                    }
                }
                if let Some(l) = self.script.as_mut() {
                    for f in l.lights.drain(..) {
                        self.lights.add(f);
                    }
                    self.lamps.tint = l.lamp_tint;
                }
                let buzz = player.pos_obj();
                let camera = world_to_game(self.cam.pos).map(|v| (v * 32.0) as i32);
                if self.lamps.level() == 3 {
                    // 0x41AA10: lit from one spot, or green by a '?' blob while there is one.
                    let d = [0x348D - buzz[0], -0x336DB - buzz[1], -0x30D5 - buzz[2]].map(|v| (v >> 7) as f32);
                    let n = Vec3::from(d).normalize_or_zero() * 4096.0;
                    self.lights.set_base([(n.x as i32) >> 2, (n.y as i32) >> 2, (n.z as i32) >> 2], 0xC0C000);
                    let blob = self.particles.slots.iter().find(|p| p.typ == 0x3F && p.life > 0).map(|p| (p.pos, [0, 0xFF, 0], -3));
                    self.lights.set_lamp(blob);
                } else {
                    self.lights.set_lamp(self.lamps.nearest(camera, buzz));
                }
                for _ in 0..ticks {
                    self.lights.tick(buzz);
                }
            }
            for ev in self.particles.events.drain(..) {
                match ev {
                    ts2::particles::Event::Sound(id, at) => creature_sounds.push((id, at)),
                    ts2::particles::Event::Light(f) => self.lights.add(f),
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
                    ts2::particles::Event::HitCreature { slot, angle } => {
                        if let Some(cr) = self.creatures.as_mut() {
                            cr.hit(slot, angle, 4);
                        }
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
                    pk.ammo_b = player.disc_ammo;
                    let evs = pk.tick(player.pos.to_array(), cam_game);
                    let speed = (player.vel[0] as f32).hypot(player.vel[2] as f32) as i32;
                    pk.follow_shield(player.pos_obj(), cam_game.map(|v| (v * 32.0) as i32), player.facing, speed);
                    if player.dying == 0 {
                        player.health = pk.health;
                    }
                    player.lives = pk.lives;
                    player.shield = pk.shield;
                    player.hook_ammo = pk.ammo_a;
                    player.disc_ammo = pk.ammo_b;
                    for ev in evs {
                        match ev {
                            ts2::pickups::Event::TokenBurst(at) => {
                                let at = at.map(|v| v * 32);
                                bursts.push(at);
                            }
                            // 0x410410 (0x32): five sparkles bursting from it.
                            ts2::pickups::Event::Returned(at) => {
                                for _ in 0..5 {
                                    let o = ((self.particles.rand() as i32 & 0x3F) - 0x20) * 0x32 * 2;
                                    let spin = self.particles.rand() as i32 - 0x80;
                                    if let Some(k) = self.particles.spawn(at.map(|v| v * 32 + o), 0x29, 0xF) {
                                        self.particles.slots[k].spin = spin;
                                    }
                                }
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
                            // Deviation (owner's request): wings out and away, not the exe's jump.
                            player.fly_off();
                            if let Some(a) = self.audio.as_mut() {
                                a.play_flat(0x33);
                            }
                            self.token_seq = 4;
                            self.token_timer = 0x44;
                        }
                    }
                    4 => {
                        player.fly_off();
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
            // The scripts have run: a scene's camera as they left it this tick.
            if let Some(c) = scene_camera(self.script.as_deref()) {
                self.cam = c;
            }
            // Draw between the last two ticks: the follow and scene cameras only (the free
            // camera, first person and the mouse-orbited camera move per frame already).
            let blend = player.first_person == 0
                && (self.use_orig_cam || scene_camera(self.script.as_deref()).is_some());
            self.interp.alpha = player.tick_fraction();
            if !blend {
                self.interp.cam = None;
            } else {
                let now = self.cam;
                let prev = match (cam_before.as_ref(), self.interp.cam) {
                    (Some(c), _) => follow_view(c),
                    (None, Some((p, n))) => {
                        if ticks > 0 {
                            n
                        } else {
                            p
                        }
                    }
                    (None, None) => now,
                };
                self.interp.cam = Some((prev, now));
                // A cut (a new scene, a respawn) doesn't blend.
                if prev.pos.distance(now.pos) < 256.0 * WORLD_SCALE {
                    self.cam = prev.lerp(&now, self.interp.alpha);
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
        // The hardware lights on flagged parts (0x4CE6C0) in play; models elsewhere stay baked.
        scene::set_model_light(self.player.is_some().then(|| scene::ModelLight::new(self.lights.direction, self.lights.colour)));
        // Animated textures (0x49B260; option 0x508D70 bit 2). A shield in use flickers too (0x4A5170).
        {
            let mut blits = self.script.as_mut().map_or_else(Vec::new, |l| std::mem::take(&mut l.tex_blits));
            if let (Some(p), Some(l)) = (self.player.as_ref(), self.script.as_ref())
                && p.shield > 0
            {
                let c = l.clock as i32;
                blits.push(ts2::texanim::Blit::new(0x10, 0x80, 0xC0, 0x40, 0x40, (c & 0xF) << 2, (c & 0x1F) << 1, 0, -0x40));
            }
            if self.animated_textures {
                for b in blits {
                    let name = format!("tex{:02}", b.texture);
                    if let Some(i) = self.ngn.textures.iter().position(|t| t.name.eq_ignore_ascii_case(&name)) {
                        gfx.blit_texture(i, &b);
                    }
                }
            }
        }
        if self.show_spawns {
            match (&self.creatures, self.player.is_some()) {
                // Play mode: the creatures as their scripts move and animate them.
                (Some(cr), true) => {
                    let cam = Vec3::from(world_to_game(self.cam.pos));
                    for (i, c) in cr
                        .slots
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| c.id != 0 && c.health > 0)
                    {
                        let Some(ci) = self
                            .ngn
                            .creatures
                            .iter()
                            .position(|n| n.id == c.id as usize)
                        else {
                            continue;
                        };
                        let pos = self.interp.creature(i, c.pos);
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
                    match &self.play_as {
                        // Deviation (mod): the bounce gait, drawn from the creature's own
                        // animations: the hop one stretched over the airtime, else its idle.
                        Some(pa) if pa.mode == ts2::modding::AnimationMode::Hop => {
                            let airborne = !p.on_floor && pa.air > 0.0;
                            let (ai, f) = if airborne {
                                let a = &pa.anims[pa.hop];
                                let total = if p.hop_low { HOP_AIR_LOW * pa.hop_speed } else { HOP_AIR_FULL };
                                (pa.hop, (pa.air / total).min(1.0) * (a.frame_count.max(2) - 1) as f32)
                            } else {
                                let a = &pa.anims[pa.idle];
                                (pa.idle, (pa.clock * ANIM_FPS) % (a.frame_count.max(2) - 1) as f32)
                            };
                            scene::merge(
                                &mut batches,
                                scene::creature_batches_scaled(&self.ngn, &pa.creature, self.interp.buzz(p.pos).to_array(), [0.0, yaw, 0.0], pa.scale, Some((&pa.anims[ai], f))),
                            );
                        }
                        // Deviation (mod): Buzz's pose retargeted onto the substitute creature.
                        Some(pa) => {
                            let buzz_pose = scene::layered_matrices(c.nodes.len(), layer(pose.base), layer(overlay));
                            let matrices = pa.retarget.matrices(&buzz_pose);
                            scene::merge(
                                &mut batches,
                                scene::creature_batches_matrices(&self.ngn, &pa.creature, self.interp.buzz(p.pos).to_array(), yaw, pa.scale, &matrices),
                            );
                        }
                        None => scene::merge(
                            &mut batches,
                            scene::creature_batches_layered(
                                &self.ngn,
                                c,
                                self.interp.buzz(p.pos).to_array(),
                                yaw,
                                layer(pose.base),
                                layer(overlay),
                            ),
                        ),
                    }
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
            scene::merge(&mut batches, pk.hook_batches(&self.ngn, &self.look.hooks, world_to_game(self.cam.pos), to_game(r), to_game(u)));
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
            // Floor shadows (0x445720): Buzz (0x436220, 0x4857E0), then the creatures and
            // particles; only in play.
            if let (Some(p), Some((tex, size))) = (self.player.as_ref(), texture(0x1F)) {
                let mut list = Vec::new();
                if let Some((y, _, _, n)) = self.world.floor_below_full(p.pos.x, p.pos.z, p.pos.y - 16.0) {
                    let n = n.to_array().map(|v| (v * 16384.0) as i32);
                    let at = p.pos_obj();
                    ts2::shadows::on_floor(&mut list, [at[0], (y * 32.0) as i32, at[2]], 100, at[1], n);
                }
                if let Some(cr) = self.creatures.as_ref() {
                    cr.shadows(&self.world, &mut list);
                }
                self.particles.shadows(&mut list);
                if let Some(l) = self.script.as_ref() {
                    for s in &l.shadows {
                        if list.len() < ts2::shadows::LIMIT {
                            list.push(*s);
                        }
                    }
                }
                scene::merge(&mut batches, ts2::shadows::batches(&list, tex, size));
            }
            let f = self.cam.forward();
            let r = f.cross(Vec3::Y).normalize_or_zero();
            let u = r.cross(f).normalize_or_zero();
            let g = |v: Vec3| Vec3::new(v.x, -v.y, -v.z);
            scene::merge(
                &mut batches,
                self.particles
                    .batches(&self.hud_sprites, &texture, g(r), g(u)),
            );
            // Only on a level that rains (a level change keeps the old drops otherwise).
            if let Some(rain) = self
                .rain
                .as_ref()
                .filter(|r| self.script.as_ref().and_then(|l| l.rain) == Some(r.sprite))
            {
                scene::merge(
                    &mut batches,
                    rain.batches(&self.hud_sprites, &texture, g(r), g(u)),
                );
            }
        }
        // Laser beams.
        scene::merge(&mut batches, self.lasers.beams_batches(Vec3::from(world_to_game(self.cam.pos)), self.beam_sprite));
        // Special objects at their current transforms, drawn with their own layer as 0x4CDDD0
        // draws every instance: layer 1 (low detail) before the depth clear, layer 0 after.
        // Their billboards join that layer's.
        let mut special_bb = [Vec::new(), Vec::new()];
        let (mut special_near, mut special_far) = (Vec::new(), Vec::new());
        let rooms = update_rooms(&mut self.cull, &self.world, &self.cam, gfx.config.width as f32 / gfx.config.height as f32);
        gfx.rooms = rooms;
        let eye = Vec3::from(world_to_game(self.cam.pos));
        for (&id, s) in &self.specials.objects {
            if !special_in_view(&self.ngn, s, if self.cull.is_some() { rooms } else { u64::MAX }, eye, self.detail.clamp(0, 2) as usize) {
                continue;
            }
            let far = s.layer != 0;
            let out = if far { &mut special_far } else { &mut special_near };
            let mut s = s.clone();
            s.pos = self.interp.special(id, s.pos);
            scene::merge(out, scene::special_batches(&self.ngn, &s, &mut special_bb[far as usize]));
        }
        gfx.specials = [upload(&gfx.device, &special_near, gfx.texture_bgs.len() - 1), upload(&gfx.device, &special_far, gfx.texture_bgs.len() - 1)];
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
                    discs: p.disc_ammo,
                    hooks: p.hook_ammo,
                    sheep: self.creatures.as_ref().map_or(0, |c| c.sheep),
                    race: self.script.as_ref().map_or(0, |l| l.race),
                    laps: self.script.as_ref().map_or(0, |l| l.laps),
                    boss: self.script.as_ref().and_then(|l| l.boss_bar).unwrap_or_else(|| self.creatures.as_ref().map_or(0, |c| c.boss.bar)),
                    boss_on: self.script.as_ref().is_some_and(|l| l.boss_hud),
                    collected: challenge_count(self.script.as_deref(), self.pickups.as_ref()),
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
                let mut flare_batches = Vec::new();
                if self.lens_flare && self.player.is_some() {
                    // 0x44F200: the lamps, then the particles; at most 8 pass, tested from the
                    // camera nine tenths of the way there (objects flagged 0x200 don't block it).
                    let cam_game = Vec3::from(world_to_game(self.cam.pos));
                    let mut flares = Vec::new();
                    self.lamps.lit(cam_game.to_array().map(|v| (v * 32.0) as i32), &mut flares);
                    self.particles.flares(&mut flares);
                    if let Some(cr) = self.creatures.as_ref() {
                        flares.extend_from_slice(&cr.flares);
                    }
                    let aspect = gfx.config.width as f32 / gfx.config.height as f32;
                    let m = Mat4::perspective_infinite_reverse_rh(60f32.to_radians(), aspect, 0.05) * self.cam.view();
                    let mut shown = 0;
                    let mut quads = Vec::new();
                    for f in flares {
                        if shown >= ts2::flares::LIMIT {
                            break;
                        }
                        let at = Vec3::from(f.pos.map(|v| v as f32 / 32.0));
                        if self.world.camera_sweep(cam_game, (at - cam_game) * 0.9, 0.0).is_some() {
                            continue;
                        }
                        let clip = m * game_to_world(at.to_array()).extend(1.0);
                        if clip.w <= 0.0 {
                            continue;
                        }
                        let s = ts2::flares::Screen { x: ((clip.x / clip.w + 1.0) * 256.0) as i32, y: ((1.0 - clip.y / clip.w) * 128.0) as i32, depth: (clip.w / WORLD_SCALE) as i32 };
                        if !(0..=0x200).contains(&s.x) || !(0..=0x100).contains(&s.y) || s.depth <= 0x80 {
                            continue;
                        }
                        shown += 1;
                        quads.extend(ts2::flares::quads(&f, s, &self.flare_table, &self.hud_sprites));
                    }
                    flare_batches = ts2::hud::batches(&quads, &self.hud_sprites, &texture);
                    for b in &mut flare_batches {
                        b.blend = scene::Blend::Additive;
                    }
                }
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
                if let Some(p) = &self.pause {
                    q.extend(p.quads(&self.hud_sprites));
                }
                // 0x49FD40: "demo mode", blinking.
                if self.demo.is_some() && self.hud_frames & 0x20 == 0 {
                    q.extend(ts2::pause::demo_text(&self.hud_sprites));
                }
                let mut hud = ts2::hud::batches(&q, &self.hud_sprites, &texture);
                hud.extend(flare_batches);
                hud
            }
            None => self.front.as_ref().map_or_else(Vec::new, |f| front_batches(f, &self.ngn)),
        };
        // 0x44DD80: colours over 0x80 (the lightning) also lay a white wash over the screen,
        // v = c × 2 − 0x100 at alpha 0xFF − v, added to the picture.
        let mut hud = hud;
        if let Some(c) = self.script.as_ref().and_then(|l| l.ambient).filter(|&c| c > 0x80) {
            hud.insert(0, ts2::flares::white_wash(c));
        }
        gfx.hud = upload(&gfx.device, &hud, gfx.texture_bgs.len() - 1);
        // Billboards (rails, spindles, plants) turn to the camera every frame.
        {
            let to_game = |v: Vec3| [v.x, -v.y, -v.z];
            let f = self.cam.forward();
            let r = f.cross(Vec3::Y).normalize_or_zero();
            let u = r.cross(f).normalize_or_zero();
            let white = gfx.texture_bgs.len() - 1;
            let seen = |k: usize| -> std::borrow::Cow<'_, [scene::Billboard]> {
                match &self.cull {
                    Some(c) => self.billboards[k].iter().zip(&c.billboard_rooms[k]).filter(|(_, r)| c.sees(**r)).map(|(b, _)| b.clone()).collect::<Vec<_>>().into(),
                    None => self.billboards[k].as_slice().into(),
                }
            };
            let mut near = scene::billboard_batches(&seen(0), to_game(r), to_game(u));
            scene::merge(&mut near, scene::billboard_batches(&special_bb[0], to_game(r), to_game(u)));
            let mut far = scene::billboard_batches(&seen(1), to_game(r), to_game(u));
            scene::merge(&mut far, scene::billboard_batches(&special_bb[1], to_game(r), to_game(u)));
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
        let feet = match self.start {
            Some(_) if !self.held(KeyCode::ShiftLeft) => self.world.start_floor(g),
            _ => self.world.floor_below(g.x, g.z, g.y - 600.0).map_or(g, |y| Vec3::new(g.x, y, g.z)),
        };
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
        // A movie playing: jump, Enter or Escape skips it.
        if self.movie.is_some() {
            self.movie_skip |= matches!(code, KeyCode::Space | KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Escape);
            return false;
        }
        // The attract demo ends at the first button (0x52AD84).
        if let Some(d) = self.demo.as_mut() {
            d.stop = true;
            return false;
        }
        // While paused the keys are the menu's (read each tick).
        if self.pause.is_some() {
            return false;
        }
        // Start: Escape, or F2 as in the original's default keys (0x49C420).
        if self.player.is_some() && matches!(code, KeyCode::Escape | KeyCode::F2) {
            self.pause_pressed = true;
            return false;
        }
        let n_anims = self.anims.get(self.creature_idx).map_or(0, Vec::len).max(1);
        // In the front end's menus the keys are the pad's (read each tick); Escape on the
        // title closes the window.
        if let Some(f) = &self.front {
            match f.state.screen {
                ts2::frontend::Screen::Title if code == KeyCode::Escape => return true,
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
            // The fire row's key (the mouse button does the same).
            KeyCode::ControlLeft if self.player.is_some() => {
                self.fire_held = true;
                self.fire_pressed = true;
            }
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
        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes().with_title("OpenTS2").with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
        // The Pizza Planet token embedded by build.rs (resource 1).
        #[cfg(windows)]
        {
            use winit::platform::windows::IconExtWindows;
            attrs = attrs.with_window_icon(winit::window::Icon::from_resource(1, None).ok());
        }
        let window = Arc::new(el.create_window(attrs).expect("create window"));
        self.attach(Some(window));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        self.window_event_inner(el, event);
        if self.quit_app || self.front.as_ref().is_some_and(|f| f.state.screen == ts2::frontend::Screen::Quit) {
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
            let white = g.texture_bgs.len() - 1;
            let none: [Vec<u32>; 5] = Default::default();
            let tags = self.cull.as_ref().map_or(&none, |c| &c.batch_rooms);
            g.level_decal = upload_rooms(&g.device, &self.level_decal, &tags[1], white);
            g.level_two_sided = upload_rooms(&g.device, &self.level_two_sided, &tags[2], white);
            g.level_far_two_sided = upload_rooms(&g.device, &self.level_far_two_sided, &tags[3], white);
            if self.cull.is_some() {
                // Gfx::new uploaded these untagged.
                g.level = upload_rooms(&g.device, &level, &tags[0], white);
                g.level_far = upload_rooms(&g.device, &level_far, &tags[4], white);
            }
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
                if let PhysicalKey::Code(raw) = event.physical_key {
                    // The options' controller page takes the next key.
                    if event.state == ElementState::Pressed && !event.repeat && self.capture_key(raw) {
                        return;
                    }
                    // In a level the keys go through the player's bindings (0x49C420).
                    let code = if self.player.is_some() && self.pause.is_none() { self.controls.translate(raw) } else { Some(raw) };
                    let Some(code) = code else { return };
                    if event.state == ElementState::Pressed {
                        if !event.repeat {
                            if self.key(code) {
                                el.exit();
                            }
                        }
                        self.keys.insert(code);
                    } else {
                        self.keys.remove(&code);
                        // The fire key held; the mouse has its own event.
                        if code == KeyCode::ControlLeft {
                            self.fire_held = false;
                        }
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
            // Focus back: the system dropped the grab, so take it again.
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                self.cursor_held = false;
                if !focused {
                    self.release_cursor();
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = ts2::game::snap_frame_time((now - self.last).as_secs_f32().min(0.1));
                self.last = now;
                self.update(dt);
                self.sync_cursor();
                self.draw();
                if let Some(w) = self.gfx.as_ref().and_then(|g| g.window.as_ref()) {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }

    /// While playing as Buzz the cursor is hidden and kept inside the window, so looking
    /// round with the mouse can't click outside it (or on the close button). Mouse looking
    /// uses raw device motion, which still arrives while the cursor is held.
    fn sync_cursor(&mut self) {
        let want = self.focused && self.player.is_some() && self.movie.is_none();
        if want == self.cursor_held {
            return;
        }
        let Some(w) = self.gfx.as_ref().and_then(|g| g.window.as_ref()) else { return };
        if want {
            use winit::window::CursorGrabMode;
            let held = w.set_cursor_grab(CursorGrabMode::Locked).or_else(|_| w.set_cursor_grab(CursorGrabMode::Confined)).is_ok();
            w.set_cursor_visible(false);
            self.cursor_held = held;
        } else if self.focused {
            self.release_cursor();
            self.cursor_held = false;
        }
    }

    fn release_cursor(&self) {
        if let Some(w) = self.gfx.as_ref().and_then(|g| g.window.as_ref()) {
            let _ = w.set_cursor_grab(winit::window::CursorGrabMode::None);
            w.set_cursor_visible(true);
        }
    }

    /// Renders the frame `update` prepared (or the movie playing).
    fn draw(&mut self) {
        let Some(g) = self.gfx.as_mut() else { return };
        if self.movie.is_some() && g.movie.is_some() {
            g.render_movie();
            return;
        }
        let aspect = g.config.width as f32 / g.config.height as f32;
        // Reversed depth (near 1, far 0) with an infinite far plane: a float depth
        // buffer then keeps its precision far out, so near-coplanar faces (a box lid
        // left ajar, a lamp's layered panels) don't fight.
        let proj = Mat4::perspective_infinite_reverse_rh(60f32.to_radians(), aspect, 0.05);
        g.sky = self.backdrop.map_or_else(Vec::new, |b| upload(&g.device, &sky_batch(&b, proj * self.cam.view(), self.cam.forward(), self.backdrop_image.as_ref().map(|_| self.ngn.textures.len()), aspect), g.texture_bgs.len() - 1));
        let u = Uniforms {
            view_proj: (proj * self.cam.view()).to_cols_array_2d(),
            params: [self.color_mode as f32, if self.alpha_test { 1.0 } else { 0.0 }, WORLD_SCALE, self.script.as_ref().and_then(|l| l.ambient).map_or(1.0, |a| a as f32 / 128.0)],
            range: [0.0, DETAIL[self.detail.clamp(0, 2) as usize].0, 0.0, 1.0 + self.gamma as f32 * 0.25],
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
                ShotStep::Teleport(at, facing) => {
                    if let Some(p) = self.player.as_mut() {
                        p.pos = Vec3::from(*at);
                        p.vel = [0; 3];
                        if let Some(f) = facing {
                            p.facing = *f;
                            p.target = *f;
                        }
                    }
                }
                ShotStep::Movie(n) => self.movies.push_back(*n),
                // Feet as they are stepping out of oil (0x53C60C = 180 ticks of kind 3).
                ShotStep::Oily => {
                    if let Some(p) = self.player.as_mut() {
                        p.footprints = 0xB4 << 16 | 3;
                    }
                }
                ShotStep::Report => {
                    if let Some(p) = self.player.as_ref() {
                        println!("buzz at {:.1?} facing {} health {} on floor {} surface {:?} prints {:x} flat particles {}", p.pos, p.facing, p.health, p.on_floor, p.surface, p.footprints, self.particles.slots.iter().filter(|q| q.life > 0 && q.flags & 0x10 != 0).count());
                    }
                }
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
        if let Some(p) = self.player.as_ref() {
            println!("buzz at {:.1?} facing {} health {} on floor {}", p.pos, p.facing, p.health, p.on_floor);
        }
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

    /// The movie playing, or the next one waiting: frames on the movie's clock (from its first
    /// frame), audio started with it, music paused meanwhile.
    fn movie_update(&mut self) {
        if self.movie.is_none() {
            let Some(n) = self.movies.pop_front() else { return };
            self.movie = self.start_movie(n);
            self.movie_skip = false;
            if self.movie.is_none() {
                return;
            }
            if let Some(m) = self.audio.as_ref().and_then(|a| a.music.as_ref()) {
                m.pause();
            }
        }
        let Some(m) = self.movie.as_mut() else { return };
        let mut latest = None;
        let mut ended = std::mem::take(&mut self.movie_skip);
        let due = m.start.map_or(1, |s| (s.elapsed().as_secs_f32() * m.rate) as u64 + 1);
        while m.shown < due {
            match m.frames.try_recv() {
                Ok(Some(f)) => {
                    latest = Some(f);
                    m.shown += 1;
                    if m.start.is_none() {
                        m.start = Some(Instant::now());
                        if let Some(s) = &m.sound {
                            s.play();
                        }
                    }
                }
                Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    ended = true;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
            }
        }
        if let (Some(f), Some(g)) = (latest, self.gfx.as_mut()) {
            g.movie_frame(&f);
        }
        if ended {
            self.movie = None;
            if self.movies.is_empty() {
                if let Some(g) = self.gfx.as_mut() {
                    g.movie = None;
                }
                if let Some(m) = self.audio.as_ref().and_then(|a| a.music.as_ref()) {
                    m.play();
                }
            }
            self.last = Instant::now();
        }
    }

    /// Opens movie `n` from the disc (as the music, `TS2_CD_DIR` or `ts2-media.txt`).
    fn start_movie(&mut self, n: usize) -> Option<Movie> {
        let game = self.front.as_ref().map(|f| f.game.clone()).or_else(|| self.game_dir.clone())?;
        let path = ts2::music::media_root(&game).and_then(|root| ts2::fmv::path(&root, n))?;
        let bytes = fs::read(&path).map_err(|e| println!("no movie {}: {e}", path.display())).ok()?;
        let (video, audio) = ts2::fmv::demux(&bytes);
        let rate = ts2::fmv::frame_rate(&video);
        let frames = ts2::fmv::decode_video(video, 60);
        let sound = self.audio.as_ref().and_then(|a| {
            let (samples, channels, sample_rate) = ts2::fmv::decode_audio(audio)?;
            let sink = rodio::Sink::try_new(&a.handle).ok()?;
            sink.pause();
            sink.append(rodio::buffer::SamplesBuffer::new(channels, sample_rate, samples));
            Some(sink)
        });
        println!("movie: {} (jump, Enter or Escape skips)", ts2::fmv::name(n).unwrap_or("?"));
        Some(Movie { frames, rate, start: None, shown: 0, sound })
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
            ts2::frontend::Screen::Demo(_) => {
                self.toggle_play();
                if let Some((_, n)) = self.demo_file.take() {
                    self.demo = Some(n);
                }
            }
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
                // The settings and the demo about to start outlive the scene.
                let (volumes, lens_flare, active_camera, demo_file) = (self.volumes, self.lens_flare, self.active_camera, self.demo_file.take());
                let (detail, gamma, animated_textures, controls) = (self.detail, self.gamma, self.animated_textures, self.controls.clone());
                *self = next;
                (self.volumes, self.lens_flare, self.active_camera, self.demo_file) = (volumes, lens_flare, active_camera, demo_file);
                (self.detail, self.gamma, self.animated_textures, self.controls) = (detail, gamma, animated_textures, controls);
                if attached {
                    self.attach(window);
                }
                self.apply_volumes();
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
        if matches!(f.state.screen, Screen::Play(_) | Screen::Demo(_)) {
            return;
        }
        let bits = [
            (&[KeyCode::ArrowUp, KeyCode::KeyW][..], Pad::UP),
            (&[KeyCode::ArrowDown, KeyCode::KeyS], Pad::DOWN),
            (&[KeyCode::ArrowLeft, KeyCode::KeyA], Pad::LEFT),
            (&[KeyCode::ArrowRight, KeyCode::KeyD], Pad::RIGHT),
            (&[KeyCode::Space, KeyCode::Enter, KeyCode::NumpadEnter], Pad::JUMP),
            (&[KeyCode::Enter, KeyCode::NumpadEnter], Pad::ENTER),
            (&[KeyCode::Escape, KeyCode::Backspace], Pad::CANCEL),
        ];
        let pad = Pad(bits.iter().filter(|(keys, _)| keys.iter().any(|k| self.keys.contains(k))).fold(0, |a, (_, b)| a | b));
        f.acc += dt;
        while f.acc >= 1.0 / 60.0 {
            f.acc -= 1.0 / 60.0;
            f.state.tick(pad);
        }
        let (mut pending_volumes, mut pending_demo) = (None, None);
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
                // Not in screenshots: they'd sit through the logos.
                Event::Movie(n) if !SILENT.load(std::sync::atomic::Ordering::Relaxed) => self.movies.push_back(n),
                Event::Movie(_) => {}
                Event::Volumes { music, sfx } => pending_volumes = Some((music, sfx)),
                Event::LensFlare(on) => self.lens_flare = on,
                Event::Graphics { detail, gamma, animated_textures } => (self.detail, self.gamma, self.animated_textures) = (detail, gamma, animated_textures),
                Event::ControlsReset => self.controls = load_controls(),
                Event::ControlsSave => save_controls(&self.controls),
                Event::ControlsDefault => self.controls.reset(),
                Event::Demo(level) => pending_demo = Some(level),
            }
        }
        if let Some(level) = pending_demo {
            let path = f.game.join("data").join("PAD").join(format!("PATH{level:02}.BIN"));
            let words = fs::read(&path).map(|b| b.chunks_exact(2).map(|w| u16::from_le_bytes([w[0], w[1]])).collect::<Vec<_>>()).unwrap_or_default();
            self.demo_file = Some(((), Demo { words, at: -2, hold: 0, pad: 0, acc: 0.0, stop: false }));
        }
        if let Some(v) = pending_volumes {
            self.volumes = v;
            self.apply_volumes();
        }
        let Some(f) = self.front.as_mut() else { return };
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
    // 0x44FF50's search for a loading picture: 0x24, 0x25, 0x28–0x2F, 0x58–0x5F.
    let searched = frame.search_picture.then(|| [0x24u32, 0x25].into_iter().chain(0x28..0x30).chain(0x58..0x60).find(|p| named(&format!("bgr{p:02}")).or_else(|| named(&format!("tex{p:02}"))).is_some())).flatten();
    // The picture is drawn blended (fully opaque), not as an opaque batch: the HUD pass draws
    // opaque batches first, writing depth, and the sprites over it would fail the test.
    if let Some(i) = frame.picture.or(searched).and_then(|p| named(&format!("bgr{p:02}")).or_else(|| named(&format!("tex{p:02}")))) {
        let c = |x: f32, y: f32| GpuVertex { pos: [x, y, 0.0], uv: [x, y], color: [0xFF, 0x80, 0x80, 0x80] };
        out.push(Batch { texture: Some(i), blend: scene::Blend::Alpha, vertices: vec![c(0.0, 0.0), c(1.0, 0.0), c(1.0, 1.0), c(0.0, 0.0), c(1.0, 1.0), c(0.0, 1.0)] });
    }
    // Screen-space pictures (0x4B8CC0), colour-keyed like the sprites.
    for (p, r, uv) in &frame.panels {
        if let Some(i) = named(&format!("tex{p:02}")).or_else(|| named(&format!("bgr{p:02}"))) {
            let c = |x: f32, y: f32, u: f32, w: f32| GpuVertex { pos: [x, y, 0.0], uv: [u, w], color: [0xFF, 0x80, 0x80, 0x80] };
            let v = vec![c(r[0], r[1], uv[0], uv[1]), c(r[2], r[1], uv[2], uv[1]), c(r[2], r[3], uv[2], uv[3]), c(r[0], r[1], uv[0], uv[1]), c(r[2], r[3], uv[2], uv[3]), c(r[0], r[3], uv[0], uv[3])];
            out.push(Batch { texture: Some(i), blend: scene::Blend::Alpha, vertices: v });
        }
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
    if frame.veil != 0 {
        let c = |x: f32, y: f32| GpuVertex { pos: [x, y, 0.0], uv: [0.0; 2], color: [frame.veil, 0, 0, 0] };
        out.push(Batch { texture: None, blend: scene::Blend::Alpha, vertices: vec![c(0.0, 0.0), c(1.0, 0.0), c(1.0, 1.0), c(0.0, 0.0), c(1.0, 1.0), c(0.0, 1.0)] });
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
    /// `movie:n`: plays movie n.
    Movie(usize),
    /// `boss`: the boss-defeated screen.
    Boss,
    /// `tp:x,y,z[,facing]`: puts Buzz there (level units, 12-bit facing), at rest.
    Teleport([f32; 3], Option<i32>),
    /// `?`: prints Buzz's position, health and floor.
    Report,
    Oily,
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
            } else if let Some(n) = w.strip_prefix("movie:").and_then(|n| n.parse().ok()) {
                shot.script.push(ShotStep::Movie(n));
            } else if w == "boss" {
                shot.script.push(ShotStep::Boss);
            } else if w == "play" {
                shot.script.push(ShotStep::Play);
            } else if w == "?" {
                shot.script.push(ShotStep::Report);
            } else if w == "oily" {
                shot.script.push(ShotStep::Oily);
            } else if let Some(t) = w.strip_prefix("tp:") {
                let v: Vec<f32> = t.split(',').filter_map(|x| x.parse().ok()).collect();
                if v.len() >= 3 {
                    shot.script.push(ShotStep::Teleport([v[0], v[1], v[2]], v.get(3).map(|&f| f as i32)));
                }
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
    // Deviation (mod): `--mod <dir>` (repeatable) and OPENTS2_MODS.
    let mut mod_args = Vec::new();
    while let Some(i) = args.iter().position(|a| a == "--mod") {
        let words: Vec<String> = args.drain(i..(i + 2).min(args.len())).collect();
        match words.get(1) {
            Some(d) => mod_args.push(std::path::PathBuf::from(d)),
            None => {
                eprintln!("--mod requires a folder");
                process::exit(2);
            }
        }
    }
    let loaded: Result<Vec<_>, String> = ts2::modding::mod_dirs(env::var_os("OPENTS2_MODS").as_deref(), &mod_args).iter().filter(|d| {
        // A mod folder that is gone (the launcher's list can lag) is skipped with a note; one
        // that is there but broken is an error.
        let there = d.join(ts2::modding::MANIFEST).is_file();
        if !there {
            eprintln!("mod {}: no {}; skipped", d.display(), ts2::modding::MANIFEST);
        }
        there
    }).map(|d| ts2::modding::load_mod(d)).collect();
    match loaded {
        Ok(mods) => {
            let _ = MODS.set(mods);
        }
        Err(e) => {
            eprintln!("{e}");
            process::exit(2);
        }
    }
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
    let mut viewer = viewer.unwrap_or_else(|e| {
        eprintln!("{e}");
        process::exit(1);
    });
    // A level opened on its own (the level tester): every ability Mr Potato Head gives is
    // already Buzz's (0x52F2D8), so the gadgets' cages are open from the start.
    if let Some(l) = viewer.script.as_mut() {
        l.abilities = 0xFF;
        // (Unset is marked 0xFF too; this makes the first tick open the gates.)
        viewer.abilities_shown = 0;
    }
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
    // Deviation (mod): a play-as creature's textures join the level's table before it is uploaded.
    let play_as = build_play_as(&mut ngn, std::path::Path::new(path).ancestors().nth(3));
    let anims: Vec<Vec<anim::Animation>> =
        ngn.creatures.iter().map(|c| c.animations.iter().filter_map(|a| anim::parse(a).ok()).collect()).collect();
    let creature_idx = creature
        .and_then(|name| ngn.creatures.iter().position(|c| c.name.eq_ignore_ascii_case(name)))
        .unwrap_or(0);
    println!("{} creatures: {:?}", ngn.creatures.len(), ngn.creatures.iter().map(|c| &c.name).collect::<Vec<_>>());

    // Spawns come from the packet file with the same stem (level.ngn -> level.raw).
    let raw_path = scene_file(std::path::Path::new(path), "raw");
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
        let dat = fs::read(scene_file(level_path, "dat")).ok()?;
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
    let room_level = exe::level_number(std::path::Path::new(path)).filter(|lv| ROOM_CULLED_LEVELS.contains(lv));
    let (sets, cull) = match room_level {
        Some(lv) => {
            let (sets, batch_rooms, billboard_rooms) = level_sets_by_room(&ngn, &specials);
            let portals = fs::read(path).map(|b| scene::Portals::parse(&b)).unwrap_or_default();
            (sets, Some(RoomCull { level: lv, portals, last: None, mask: u64::MAX, billboard_rooms, batch_rooms }))
        }
        None => (level_sets(&ngn, &specials), None),
    };
    let LevelSets { near: level, two_sided: level_two_sided, decal: level_decal, far: level_far, far_two_sided: level_far_two_sided, near_bb, far_bb } = sets;
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
    let dat_lists = std::fs::read(scene_file(std::path::Path::new(path), "dat")).ok().and_then(|b| ts2::formats::leveldat::parse(&b).ok()).unwrap_or_default();
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
    let level_lists = fs::read(scene_file(level_path, "dat")).ok().and_then(|b| ts2::formats::leveldat::parse(&b).ok()).unwrap_or_default();
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
                Some(AudioOut { _stream: stream, handle, bank, music, sfx: 1.0, voice: None, buffers: Default::default(), owned: Default::default() })
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
    let mut particles = ts2::particles::Particles::new(game_exe.as_ref(), random_table.clone().unwrap_or_default());
    particles.level = level_no.unwrap_or(0);
    let hud_sprites = game_exe.as_ref().zip(exe::level_number(level_path)).map(|(e, lv)| ts2::hud::sprite_table(e, lv)).unwrap_or_default();
    match start {
        Some(s) => println!("player start {:?}, facing {:.0} degrees (F to play, Shift+F to drop at camera)", s.pos, s.yaw_radians().to_degrees()),
        None => println!("no player start found (toy2.exe not next to data\\, or not a main level file)"),
    }
    // Hook points (level.dat list 59) for the grappling hook's targets, on their floors.
    let look = {
        let mut fp = ts2::look::FirstPerson::default();
        fp.hooks = dat_lists.iter().find(|l| l.kind == 59).map(|l| l.points.clone()).unwrap_or_default();
        fp.settle_hooks(&world);
        fp
    };
    let mut viewer = Viewer {
        backdrop,
        backdrop_image,
        level,
        level_decal,
        level_two_sided,
        level_far_two_sided,
        level_far,
        billboards: [near_bb, far_bb],
        cull,
        world,
        player: None,
        player_creature,
        play_as,
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
        cam: Camera {
            pos: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
        },
        interp: Interp::default(),
        rocket_on: false,
        rain: None,
        orig_cam: None,
        abilities_shown: u8::MAX,
        use_orig_cam: true,
        keys: Default::default(),
        looking: false,
        cursor_held: false,
        focused: true,
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
        look,
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
        flare_table: game_exe.as_ref().map(ts2::flares::table).unwrap_or_default(),
        lamps: level_no.map_or_else(Default::default, |lv| ts2::flares::Lamps::new(lv, &level_lists)),
        lens_flare: true,
        lights: level_no.map_or_else(Default::default, |lv| ts2::lights::Lights::new(game_exe.as_ref(), lv)),
        level_lists,
        quit_level: false,
        pause: None,
        pause_pressed: false,
        pause_acc: 0.0,
        quit_app: false,
        volumes: (8, 8),
        active_camera: true,
        detail: 2,
        gamma: 0,
        animated_textures: true,
        controls: load_controls(),
        demo: None,
        demo_file: None,
        out_of_lives: false,
        game_dir: level_path.ancestors().nth(3).map(std::path::Path::to_path_buf),
        level_track: level_no.and_then(|lv| ts2::campaign::LEVELS.iter().position(|l| l.internal_id == lv)),
        music_track: level_no.and_then(|lv| ts2::campaign::LEVELS.iter().position(|l| l.internal_id == lv)),
        theme_events: level_no.is_some_and(ts2::music::level_switches_theme),
        movie: None,
        movies: std::collections::VecDeque::new(),
        movie_skip: false,
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
    // The sky and ground colours are the picture's first and last pixels. Textures are decoded
    // from sRGB but vertex colours are not, so with a picture the fills sample its corner texels
    // (an exact match at the seam); without one the colours are converted to linear here.
    let (w, h) = (b.width.max(1) as f32, b.height.max(1) as f32);
    let (sky_uv, ground_uv) = ([0.5 / w, 0.5 / h], [1.0 - 0.5 / w, 1.0 - 0.5 / h]);
    let fill = |y0: f32, y1: f32, uv: [f32; 2], c: [u8; 3]| match picture {
        Some(_) => quad(0.0, y0, 1.0, y1, [uv[0], uv[1], uv[0], uv[1]], [0xFF; 3]),
        None => quad(0.0, y0, 1.0, y1, [0.0; 4], c.map(srgb_to_linear)),
    };
    if l > 0.0 {
        flat_v.extend(fill(0.0, l, sky_uv, b.sky));
    }
    if l < 1.0 {
        flat_v.extend(fill(l, 1.0, ground_uv, b.ground));
    }
    out.push(Batch { texture: picture, blend: scene::Blend::Opaque, vertices: flat_v });
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
        // The opaque picture must win equal-depth tests against the flat fills.
        out.insert(0, Batch { texture: Some(tex), blend: scene::Blend::Opaque, vertices: v });
    }
    out
}

/// 0x830D4C for the HUD: during a countdown (laps ≥ 100) in a level with challenge
/// collectables, how many are collected. Approximation: the original sets it when the
/// challenge starts; a plain countdown (level 10's runaway token) has none.
fn challenge_count(script: Option<&dyn ts2::script::Script>, pickups: Option<&ts2::pickups::Pickups>) -> Option<i32> {
    let (l, pk) = (script?, pickups?);
    (l.race == 2 && l.laps >= 100 && pk.items.iter().any(|i| i.kind == ts2::pickups::Kind::Collectable)).then_some(pk.collectables)
}

/// A scene's `.dat` / `.raw` beside its `.ngn`. The texture variants (`levelt1.ngn`,
/// `level1t3.ngn`, …) are built from their base scene's files (data\level16\levelt1.cfg:
/// `level1.dat`, `level1.raw`), so they fall back to the stem without `tN`.
fn scene_file(ngn: &std::path::Path, ext: &str) -> std::path::PathBuf {
    let own = ngn.with_extension(ext);
    let stem = ngn.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    match stem.char_indices().rev().nth(1) {
        Some((i, 't')) if !own.exists() && stem[i + 1..].bytes().all(|b| b.is_ascii_digit()) => ngn.with_file_name(format!("{}.{ext}", &stem[..i])),
        _ => own,
    }
}

/// An sRGB byte as a linear byte, for flat colours drawn into the sRGB target.
fn srgb_to_linear(c: u8) -> u8 {
    let s = c as f32 / 255.0;
    let l = if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) };
    (l * 255.0).round() as u8
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


#[cfg(test)]
mod room_tests {
    use super::*;

    /// Level 4 split room by room keeps every triangle and billboard of the whole-level lists,
    /// each tagged with its instances' room; the paint room (2) has detailed geometry only.
    #[test]
    fn level_sets_by_room_keep_everything_tagged() {
        let Some(game) = std::env::var_os("TS2_GAME_DIR") else { return };
        let path = ts2::campaign::level(4).unwrap().path(std::path::Path::new(&game));
        let n = ngn::parse(&fs::read(path).unwrap()).unwrap();
        let specials = ts2::specials::Specials::new(&n);
        let whole = level_sets(&n, &specials);
        let (split, tags, bb) = level_sets_by_room(&n, &specials);
        let count = |b: &[Batch]| b.iter().map(|b| b.vertices.len()).sum::<usize>();
        for (a, b, t) in [(&whole.near, &split.near, &tags[0]), (&whole.decal, &split.decal, &tags[1]), (&whole.two_sided, &split.two_sided, &tags[2]), (&whole.far_two_sided, &split.far_two_sided, &tags[3]), (&whole.far, &split.far, &tags[4])] {
            assert_eq!(count(a), count(b));
            assert_eq!(b.len(), t.len());
        }
        assert_eq!((whole.near_bb.len(), whole.far_bb.len()), (split.near_bb.len(), split.far_bb.len()));
        assert_eq!((bb[0].len(), bb[1].len()), (split.near_bb.len(), split.far_bb.len()));
        assert!(tags[0].contains(&2) && !tags[4].contains(&2), "room 2 is in the detailed set only");
    }

    /// Special objects of rooms not in view, or (detailed set) over 12000 from the eye, are
    /// left out (0x4BC460); others are kept.
    #[test]
    fn specials_culled_by_room_and_distance() {
        let Some(game) = std::env::var_os("TS2_GAME_DIR") else { return };
        let path = ts2::campaign::level(4).unwrap().path(std::path::Path::new(&game));
        let n = ngn::parse(&fs::read(path).unwrap()).unwrap();
        let specials = ts2::specials::Specials::new(&n);
        // 0x3F: the hatch's inside face (room 2); 0x40: its outside face (room 1).
        let (inside, outside) = (specials.get(0x3F).unwrap(), specials.get(0x40).unwrap());
        let near = Vec3::new(10500.0, -2400.0, 9000.0);
        assert!(special_in_view(&n, outside, 0b011, near, 2));
        assert!(!special_in_view(&n, inside, 0b011, near, 2), "room 2 not in view");
        assert!(special_in_view(&n, inside, 0b111, near, 2));
        assert!(!special_in_view(&n, outside, 0b111, near - Vec3::Z * 12000.0, 2), "too far");
    }

    /// On every level, a special object of the detailed set is drawn only within 12000 of the
    /// eye and one of the low-detail set only beyond 1000 (0x4BC460 / 0x4BC720 against
    /// 0x5088B0 / 0x5088B4): level 14's helicopter (0x08 in layer 0, 0x17 its layer-1 copy),
    /// seen from the start, is the low-detail copy, drawn with the low-detail set.
    #[test]
    fn specials_of_each_layer_have_their_own_distance() {
        let Some(game) = std::env::var_os("TS2_GAME_DIR") else { return };
        let path = ts2::campaign::level(14).unwrap().path(std::path::Path::new(&game));
        let n = ngn::parse(&fs::read(path).unwrap()).unwrap();
        let specials = ts2::specials::Specials::new(&n);
        let (detailed, low) = (specials.get(0x08).unwrap(), specials.get(0x17).unwrap());
        assert_eq!((detailed.layer, low.layer), (0, 1));
        let start = Vec3::new(-10010.0, 0.0, 30728.0);
        assert!(!special_in_view(&n, detailed, u64::MAX, start, 2), "detailed copy beyond 12000");
        assert!(special_in_view(&n, low, u64::MAX, start, 2));
        let under = Vec3::new(-10414.0, -3000.0, 1800.0);
        assert!(special_in_view(&n, detailed, u64::MAX, under, 2));
        assert!(!special_in_view(&n, low, u64::MAX, under, 2), "low-detail copy within 1000");
    }
}
