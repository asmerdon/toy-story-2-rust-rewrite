//! CPU-side scene building: turns parsed `.ngn` data into flat triangle batches per texture.
//! Kept free of GPU types so it can be tested headless.

use crate::formats::anim::Animation;
pub use crate::formats::ngn::Blend;
use crate::formats::ngn::{Creature, Mesh, Ngn, Vertex};

/// One vertex as uploaded to the GPU. Positions stay in game units and game axes (+Y down);
/// the shader converts.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    /// Raw vertex colour bytes; the shader picks the interpretation.
    pub color: [u8; 4],
}

/// Triangles that share one texture (`None` = untextured) and blend mode.
#[derive(Debug, Clone, Default)]
pub struct Batch {
    pub texture: Option<usize>,
    pub blend: Blend,
    pub vertices: Vec<GpuVertex>,
}

fn texture_index(ngn: &Ngn, name: &str) -> Option<usize> {
    ngn.textures.iter().position(|t| t.name == name)
}

pub fn push(batches: &mut Vec<Batch>, texture: Option<usize>, blend: Blend, tri: [GpuVertex; 3]) {
    match batches.iter_mut().find(|b| b.texture == texture && b.blend == blend) {
        Some(b) => b.vertices.extend_from_slice(&tri),
        None => batches.push(Batch { texture, blend, vertices: tri.to_vec() }),
    }
}

fn gpu(v: &Vertex, pos: [f32; 3]) -> GpuVertex {
    GpuVertex { pos, uv: v.uv, color: v.color }
}

fn transform(m: Option<&[[f32; 4]; 4]>, p: [f32; 3]) -> [f32; 3] {
    match m {
        Some(m) => [0, 1, 2].map(|j| p[0] * m[0][j] + p[1] * m[1][j] + p[2] * m[2][j] + m[3][j]),
        None => p,
    }
}

/// Rotation about the vertical axis (game space), then translation.
#[derive(Debug, Clone, Copy)]
struct Place {
    offset: [f32; 3],
    /// Rows of the rotation, applied as a row vector times it.
    m: [[f32; 3]; 3],
}

impl Place {
    fn new(offset: [f32; 3], yaw: f32) -> Place {
        Place::turned(offset, [0.0, yaw, 0.0])
    }

    /// Turned about X, then the vertical, then Z (the root matrix of 0x4CD880), radians.
    fn turned(offset: [f32; 3], [ax, ay, az]: [f32; 3]) -> Place {
        let (sx, cx) = ax.sin_cos();
        let (sy, cy) = ay.sin_cos();
        let (sz, cz) = az.sin_cos();
        let rx = [[1.0, 0.0, 0.0], [0.0, cx, sx], [0.0, -sx, cx]];
        let ry = [[cy, 0.0, -sy], [0.0, 1.0, 0.0], [sy, 0.0, cy]];
        let rz = [[cz, sz, 0.0], [-sz, cz, 0.0], [0.0, 0.0, 1.0]];
        let mul = |a: [[f32; 3]; 3], b: [[f32; 3]; 3]| std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()));
        Place { offset, m: mul(mul(rx, ry), rz) }
    }

    fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let m = &self.m;
        std::array::from_fn(|j| p[0] * m[0][j] + p[1] * m[1][j] + p[2] * m[2][j] + self.offset[j])
    }
}

fn add_mesh(ngn: &Ngn, out: &mut Vec<Batch>, mesh: &Mesh, place: Place, m: Option<&[[f32; 4]; 4]>) {
    add_mesh_parts(ngn, out, None, mesh, place, m, None);
}

/// Like `add_mesh`; with `billboards`, billboard groups go there instead of into `out`. With
/// `sided`, only groups whose material is (`Some(true)`) or isn't two-sided go into `out`, and
/// billboards (never culled) only in the two-sided call.
fn add_mesh_parts(ngn: &Ngn, out: &mut Vec<Batch>, mut billboards: Option<&mut Vec<Billboard>>, mesh: &Mesh, place: Place, m: Option<&[[f32; 4]; 4]>, sided: Option<bool>) {
    for g in &mesh.groups {
        let is_billboard = billboards.is_some() && g.sprite_pivot().is_some();
        if sided.is_some_and(|s| if is_billboard { !s } else { s != mesh.group_two_sided(g) }) {
            continue;
        }
        if let (Some(bb), Some(pivot)) = (billboards.as_deref_mut(), g.sprite_pivot()) {
            let corner = |k: usize| &mesh.vertices[g.indices[k] as usize];
            bb.push(Billboard {
                pivot: place.apply(transform(m, mesh.vertices[pivot as usize].pos)),
                corners: [1, 2, 3, 4].map(|k| corner(k).pos),
                uv: [1, 2, 3, 4].map(|k| corner(k).uv),
                color: [1, 2, 3, 4].map(|k| corner(k).color),
                texture: mesh.group_texture(g).and_then(|n| texture_index(ngn, n)),
                blend: mesh.group_blend(g),
                upright: matches!(g.prim, crate::formats::ngn::Prim::Fan),
            });
            continue;
        }
        let tex = mesh.group_texture(g).and_then(|n| texture_index(ngn, n));
        let blend = mesh.group_blend(g);
        for t in g.triangles() {
            let tri = t.map(|i| {
                let v = &mesh.vertices[i as usize];
                gpu(v, place.apply(transform(m, mesh.position(g, i))))
            });
            push(out, tex, blend, tri);
        }
    }
}

/// All level geometry, placed by its instances.
pub fn level_batches(ngn: &Ngn) -> Vec<Batch> {
    level_batches_filtered(ngn, |_| true)
}

/// Level geometry for the instances `keep` accepts.
pub fn level_batches_filtered(ngn: &Ngn, keep: impl Fn(&crate::formats::ngn::Instance) -> bool) -> Vec<Batch> {
    level_batches_where(ngn, |_, _, i| keep(i))
}

/// Level geometry for the instances `keep(layer, index, instance)` accepts, billboards drawn
/// as placed (see `level_parts_where` for camera-facing ones).
pub fn level_batches_where(ngn: &Ngn, keep: impl Fn(usize, usize, &crate::formats::ngn::Instance) -> bool) -> Vec<Batch> {
    let mut out = Vec::new();
    for (li, layer) in ngn.layers.iter().enumerate() {
        for (_, inst) in layer.instances.iter().enumerate().filter(|(ii, i)| keep(li, *ii, i)) {
            if let Some(mesh) = layer.meshes.get(inst.mesh as usize) {
                let m = instance_matrix(inst);
                add_mesh(ngn, &mut out, mesh, Place::new(inst.pos, 0.0), Some(&m));
            }
        }
    }
    out
}

/// Level geometry split into static batches and billboards, which must be rebuilt each frame
/// with `billboard_batches`.
pub fn level_parts_where(ngn: &Ngn, keep: impl Fn(usize, usize, &crate::formats::ngn::Instance) -> bool) -> (Vec<Batch>, Vec<Billboard>) {
    level_parts_sided(ngn, keep, None)
}

/// As [`level_parts_where`], with only the two-sided (`Some(true)`) or one-sided materials.
pub fn level_parts_sided(ngn: &Ngn, keep: impl Fn(usize, usize, &crate::formats::ngn::Instance) -> bool, sided: Option<bool>) -> (Vec<Batch>, Vec<Billboard>) {
    let (mut out, mut bb) = (Vec::new(), Vec::new());
    let images: Vec<Option<crate::bmp::Image>> = ngn.textures.iter().map(|t| crate::bmp::decode(&t.bmp, Some(crate::bmp::PC_KEY))).collect();
    for (li, layer) in ngn.layers.iter().enumerate() {
        for (_, inst) in layer.instances.iter().enumerate().filter(|(ii, i)| keep(li, *ii, i)) {
            if let Some(mesh) = layer.meshes.get(inst.mesh as usize) {
                let m = instance_matrix(inst);
                let mut one = Vec::new();
                add_mesh_parts(ngn, &mut one, Some(&mut bb), mesh, Place::new(inst.pos, 0.0), Some(&m), sided);
                light_black_faces(&mut one);
                show_keyed_glass(&mut one, &images);
                merge(&mut out, one);
            }
        }
    }
    (out, bb)
}

/// A camera-facing quad from a code 5/6 group (0x4B70E0 types 5/4). Only the pivot goes
/// through the instance's matrix; the corners are turned to the camera around it.
#[derive(Debug, Clone)]
pub struct Billboard {
    /// World position (game units).
    pub pivot: [f32; 3],
    /// Corner offsets in the billboard's plane (x right, y down), in strip order.
    pub corners: [[f32; 3]; 4],
    pub uv: [[f32; 2]; 4],
    pub color: [[u8; 4]; 4],
    pub texture: Option<usize>,
    pub blend: Blend,
    /// Code 6: turned about the vertical axis only (stays upright).
    pub upright: bool,
}

/// Triangles for `billboards` seen by a camera whose right and up axes (game space, +Y down)
/// are `right` and `up`.
pub fn billboard_batches(billboards: &[Billboard], right: [f32; 3], up: [f32; 3]) -> Vec<Batch> {
    let mut out = Vec::new();
    let flat = {
        let l = (right[0] * right[0] + right[2] * right[2]).sqrt().max(1e-6);
        [right[0] / l, 0.0, right[2] / l]
    };
    for b in billboards {
        let (rx, dy) = if b.upright { (flat, [0.0, 1.0, 0.0]) } else { (right, up.map(|v| -v)) };
        let v: [GpuVertex; 4] = std::array::from_fn(|k| {
            let c = b.corners[k];
            GpuVertex { pos: [0, 1, 2].map(|j| b.pivot[j] + rx[j] * c[0] + dy[j] * c[1]), uv: b.uv[k], color: b.color[k] }
        });
        push(&mut out, b.texture, b.blend, [v[0], v[1], v[2]]);
        push(&mut out, b.texture, b.blend, [v[1], v[3], v[2]]);
    }
    out
}

/// A special object at its current position, angles and scale (`specials::Special`); its
/// billboards go to `billboards`.
pub fn special_batches(ngn: &Ngn, s: &crate::specials::Special, billboards: &mut Vec<Billboard>) -> Vec<Batch> {
    let mut out = Vec::new();
    if s.hidden {
        return out;
    }
    let Some(inst) = ngn.layers.get(s.layer).and_then(|l| l.instances.get(s.instance)) else { return out };
    let Some(mesh) = ngn.layers[s.layer].meshes.get(inst.mesh as usize) else { return out };
    let placed = crate::formats::ngn::Instance { pos: s.pos, rot: s.angles, scale: s.scale, ..*inst };
    let m = instance_matrix(&placed);
    add_mesh_parts(ngn, &mut out, Some(billboards), mesh, Place::new(s.pos, 0.0), Some(&m), None);
    out
}

/// One level instance, turned further by `spin` (12-bit angles added to its own, as the
/// game's 0x4CCCE0 sets them) and scaled by `scale` on top of its own scale (0x4CCB20).
pub fn instance_batches(ngn: &Ngn, layer: usize, index: usize, spin: [i32; 3], scale: f32) -> Vec<Batch> {
    let mut out = Vec::new();
    let Some(inst) = ngn.layers.get(layer).and_then(|l| l.instances.get(index)) else { return out };
    let Some(mesh) = ngn.layers[layer].meshes.get(inst.mesh as usize) else { return out };
    let mut turned = *inst;
    turned.rot = std::array::from_fn(|k| inst.rot[k].wrapping_add((spin[k] as u32) << 4));
    turned.scale = inst.scale.map(|s| s * scale);
    let m = instance_matrix(&turned);
    add_mesh(ngn, &mut out, mesh, Place::new(inst.pos, 0.0), Some(&m));
    out
}

/// A sprite centred on `pos`, `half` units from centre to edge along `right` and `up`
/// (the camera's axes for a billboard, X and Z for one lying flat). `uv` is (top-left,
/// bottom-right). Drawn alpha-blended.
#[allow(clippy::too_many_arguments)]
pub fn sprite(out: &mut Vec<Batch>, texture: Option<usize>, pos: [f32; 3], half: f32, right: [f32; 3], up: [f32; 3], uv: [[f32; 2]; 2], color: [u8; 4]) {
    sprite_blend(out, texture, Blend::Alpha, pos, half, right, up, uv, color);
}

/// As `sprite`, with blend mode `blend` (Opaque: alpha-tested, writing depth).
#[allow(clippy::too_many_arguments)]
pub fn sprite_blend(out: &mut Vec<Batch>, texture: Option<usize>, blend: Blend, pos: [f32; 3], half: f32, right: [f32; 3], up: [f32; 3], uv: [[f32; 2]; 2], color: [u8; 4]) {
    let corner = |sx: f32, sy: f32, u: f32, v: f32| GpuVertex {
        pos: [0, 1, 2].map(|k| pos[k] + (right[k] * sx + up[k] * sy) * half),
        uv: [u, v],
        color,
    };
    let [[u0, v0], [u1, v1]] = uv;
    let (a, b, c, d) = (corner(-1.0, 1.0, u0, v0), corner(1.0, 1.0, u1, v0), corner(1.0, -1.0, u1, v1), corner(-1.0, -1.0, u0, v1));
    push(out, texture, blend, [a, b, c]);
    push(out, texture, blend, [a, c, d]);
}

/// An instance's rotation and scale as a row-vector matrix (no translation): scale, then
/// rotate about Z, Y, X by its 16-bit angles (0x4000 = 90°). Checked against the collision
/// data, which is stored in world orientation (examples/rotfit.rs: rotating moves most rotated
/// instances onto their collision surfaces).
pub fn instance_matrix(inst: &crate::formats::ngn::Instance) -> [[f32; 4]; 4] {
    use glam::Mat3;
    let a = |r: u32| (r & 0xFFFF) as f32 / 65536.0 * std::f32::consts::TAU;
    let r = Mat3::from_rotation_x(a(inst.rot[0])) * Mat3::from_rotation_y(a(inst.rot[1])) * Mat3::from_rotation_z(a(inst.rot[2]));
    let r = r * Mat3::from_diagonal(glam::Vec3::from(inst.scale));
    // Row i of a row-vector matrix is column i of the column-vector one.
    let c = |i: usize| r.col(i).to_array();
    [[c(0)[0], c(0)[1], c(0)[2], 0.0], [c(1)[0], c(1)[1], c(1)[2], 0.0], [c(2)[0], c(2)[1], c(2)[2], 0.0], [0.0, 0.0, 0.0, 1.0]]
}

/// A creature at `origin` (game units) turned by `yaw` radians about the vertical axis,
/// optionally posed by `anim` at fractional frame `t`.
pub fn creature_batches(ngn: &Ngn, c: &Creature, origin: [f32; 3], yaw: f32, anim: Option<(&Animation, f32)>) -> Vec<Batch> {
    creature_batches_layered(ngn, c, origin, yaw, anim, None)
}

/// As `creature_batches`, turned by all three of its angles (tilt X, facing Y, lean Z;
/// radians, applied in that order).
pub fn creature_batches_turned(ngn: &Ngn, c: &Creature, origin: [f32; 3], angles: [f32; 3], anim: Option<(&Animation, f32)>) -> Vec<Batch> {
    creature_batches_placed(ngn, c, Place::turned(origin, angles), anim, None)
}

/// As `creature_batches_turned`, scaled by `scale` (1 = as built) in its own frame.
pub fn creature_batches_scaled(ngn: &Ngn, c: &Creature, origin: [f32; 3], angles: [f32; 3], scale: f32, anim: Option<(&Animation, f32)>) -> Vec<Batch> {
    creature_batches_scaled_xyz(ngn, c, origin, angles, [scale; 3], anim)
}

/// As [`creature_batches_scaled`], scaled per model axis (the slot's +0x2C / +0x2E / +0x30).
pub fn creature_batches_scaled_xyz(ngn: &Ngn, c: &Creature, origin: [f32; 3], angles: [f32; 3], scale: [f32; 3], anim: Option<(&Animation, f32)>) -> Vec<Batch> {
    let mut place = Place::turned(origin, angles);
    for (row, s) in place.m.iter_mut().zip(scale) {
        for v in row.iter_mut() {
            *v *= s;
        }
    }
    creature_batches_placed(ngn, c, place, anim, None)
}

/// Transform a point on an animated creature node using the same matrices as rendering.
/// The result is in level units; gameplay callers convert it to object units (x32).
pub fn creature_node_point(origin: [f32; 3], angles: [f32; 3], scale: f32,
    animation: &Animation, frame: f32, node: usize, point: [f32; 3]) -> Option<[f32; 3]> {
    // A node this animation doesn't move stays at rest, as the renderer draws it.
    if node >= animation.node_offsets.len() {
        return None;
    }
    let matrix = animation.matrix_at(node, frame);
    let mut place = Place::turned(origin, angles);
    for row in &mut place.m { for value in row { *value *= scale; } }
    Some(place.apply(transform(matrix.as_ref(), point)))
}

/// Point on the base layer's root node that the overlay follows (0x43BA80).
const ROOT_ANCHOR: [f32; 3] = [0.0, -200.0, 0.0];

/// How far the base layer's root node moves `ROOT_ANCHOR` from its rest position. Buzz's pose
/// builder (0x43BA80) adds this to every overlay node's translation, so the upper body follows
/// the hips. Only when the layers are different animations and the base has a root node.
fn root_offset(base: Option<(&Animation, f32)>, overlay: Option<(&Animation, f32)>) -> Option<[f32; 3]> {
    let ((b, t), (o, _)) = (base?, overlay?);
    if std::ptr::eq(b, o) || b.root_node < 0 {
        return None;
    }
    let m = b.matrix_at(b.root_node as usize, t)?;
    let p = transform(Some(&m), ROOT_ANCHOR);
    Some([0, 1, 2].map(|k| p[k] - ROOT_ANCHOR[k]))
}

/// As the game draws objects (0x4CD7B0): the `base` animation poses every node it covers,
/// then `overlay` replaces the nodes it animates, shifted by the base's root-node offset.
pub fn creature_batches_layered(
    ngn: &Ngn,
    c: &Creature,
    origin: [f32; 3],
    yaw: f32,
    base: Option<(&Animation, f32)>,
    overlay: Option<(&Animation, f32)>,
) -> Vec<Batch> {
    creature_batches_placed(ngn, c, Place::new(origin, yaw), base, overlay)
}

fn creature_batches_placed(ngn: &Ngn, c: &Creature, place: Place, base: Option<(&Animation, f32)>, overlay: Option<(&Animation, f32)>) -> Vec<Batch> {
    let mut out = Vec::new();
    let offset = root_offset(base, overlay).unwrap_or([0.0; 3]);
    let matrices: Vec<Option<[[f32; 4]; 4]>> = (0..c.nodes.len())
        .map(|n| {
            let over = overlay.and_then(|(a, t)| a.matrix_at(n, t)).map(|mut m| {
                (0..3).for_each(|k| m[3][k] += offset[k]);
                m
            });
            over.or_else(|| base.and_then(|(a, t)| a.matrix_at(n, t)))
        })
        .collect();
    for (n, node) in c.nodes.iter().enumerate() {
        if node.hidden {
            continue;
        }
        if let Some(mesh) = &node.mesh {
            add_mesh(ngn, &mut out, mesh, place, matrices[n].as_ref());
        }
    }
    for patch in &c.patches {
        let material = c.shared_materials.get(patch.material as usize);
        let blend = material.map(|m| m.blend()).unwrap_or_default();
        let tex = material.and_then(|m| m.texture).and_then(|t| c.shared_textures.get(t as usize)).and_then(|name| texture_index(ngn, name));
        let verts: Vec<GpuVertex> = patch
            .vertices()
            .into_iter()
            .map(|(vi, ni)| {
                let v = &c.shared_vertices[vi];
                gpu(v, place.apply(transform(matrices.get(ni).and_then(|m| m.as_ref()), v.pos)))
            })
            .collect();
        for t in patch.triangles() {
            push(&mut out, tex, blend, t.map(|i| verts[i]));
        }
    }
    out
}

/// Appends `more` into `into`, merging batches that share a texture and blend mode.
pub fn merge(into: &mut Vec<Batch>, more: Vec<Batch>) {
    for b in more {
        match into.iter_mut().find(|x| x.texture == b.texture && x.blend == b.blend) {
            Some(x) => x.vertices.extend(b.vertices),
            None => into.push(b),
        }
    }
}

/// Axis-aligned bounds of a set of batches (game units).
pub fn bounds(batches: &[Batch]) -> Option<([f32; 3], [f32; 3])> {
    let mut it = batches.iter().flat_map(|b| &b.vertices).map(|v| v.pos);
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| ([0, 1, 2].map(|k| lo[k].min(p[k])), [0, 1, 2].map(|k| hi[k].max(p[k])))))
}

/// Deviation: a triangle baked pure black (all three vertex colours 0, 0, 0) in an object that
/// also has lit faces takes the object's average lit colour. The data has such faces facing
/// into rooms (level 1's attic entrance: one inner wall black, the one across it wood); the
/// original shows them lit. Objects that are all black (the boxes' insides) stay black.
fn light_black_faces(batches: &mut [Batch]) {
    let black = |c: [u8; 4]| c[1] == 0 && c[2] == 0 && c[3] == 0;
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    for v in batches.iter().flat_map(|b| &b.vertices).filter(|v| !black(v.color)) {
        for k in 0..3 {
            sum[k] += v.color[k + 1] as u64;
        }
        n += 1;
    }
    if n == 0 {
        return;
    }
    let avg = sum.map(|s| (s / n) as u8);
    for b in batches.iter_mut() {
        for t in b.vertices.chunks_exact_mut(3) {
            if t.iter().all(|v| black(v.color)) {
                for v in t {
                    v.color = [v.color[0], avg[0], avg[1], avg[2]];
                }
            }
        }
    }
}

/// Deviation: a blended triangle whose texture area is (nearly) all the colour key would vanish
/// entirely. Level 1's kitchen jars are such: round glass mapped onto pure key green, shown in
/// the original as dark see-through green. When an object's keyed triangles aren't all in one
/// plane (windows are flat, and stay see-through), they're drawn untextured in a dark green lit
/// by their vertex colour, at half strength.
fn show_keyed_glass(batches: &mut Vec<Batch>, images: &[Option<crate::bmp::Image>]) {
    let keyed_tri = |t: &[GpuVertex], img: &crate::bmp::Image| {
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for v in t {
            for k in 0..2 {
                lo[k] = lo[k].min(v.uv[k]);
                hi[k] = hi[k].max(v.uv[k]);
            }
        }
        // 8 × 8 samples over the triangle's texture box.
        let mut keyed = 0;
        for sy in 0..8 {
            for sx in 0..8 {
                let u = lo[0] + (hi[0] - lo[0]) * (sx as f32 + 0.5) / 8.0;
                let v = lo[1] + (hi[1] - lo[1]) * (sy as f32 + 0.5) / 8.0;
                let x = (u.rem_euclid(1.0) * img.width as f32) as u32 % img.width;
                let y = (v.rem_euclid(1.0) * img.height as f32) as u32 % img.height;
                if img.rgba[((y * img.width + x) * 4 + 3) as usize] == 0 {
                    keyed += 1;
                }
            }
        }
        keyed * 10 >= 64 * 9
    };
    // Which triangles (batch, first vertex) are keyed, and are they all in one plane?
    let mut found: Vec<(usize, usize)> = Vec::new();
    let mut normals: Vec<glam::Vec3> = Vec::new();
    for (bi, b) in batches.iter().enumerate() {
        if b.blend == Blend::Opaque {
            continue;
        }
        let Some(img) = b.texture.and_then(|t| images.get(t)).and_then(|i| i.as_ref()) else { continue };
        for (ti, t) in b.vertices.chunks_exact(3).enumerate() {
            if keyed_tri(t, img) {
                found.push((bi, ti * 3));
                let p = t.iter().map(|v| glam::Vec3::from(v.pos)).collect::<Vec<_>>();
                normals.push((p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero());
            }
        }
    }
    let flat = normals.iter().all(|n| n.dot(normals[0]).abs() > 0.98);
    if found.is_empty() || flat {
        return;
    }
    let mut moved: Vec<Batch> = Vec::new();
    for &(bi, vi) in found.iter().rev() {
        let blend = batches[bi].blend;
        let tri: Vec<GpuVertex> = batches[bi].vertices.drain(vi..vi + 3).collect();
        let glass = tri.into_iter().map(|v| {
            let light = v.color[1].max(v.color[2]).max(v.color[3]);
            GpuVertex { color: [0x80, light / 6, light / 2, light / 6], ..v }
        });
        match moved.iter_mut().find(|m| m.blend == blend) {
            Some(m) => m.vertices.extend(glass),
            None => moved.push(Batch { texture: None, blend, vertices: glass.collect() }),
        }
    }
    merge(batches, moved);
}

/// One room-to-room link from ngn chunk 0x103 (read by 0x4C3DF0: count, then per link the
/// polygon index, the room it leads from and the room it leads to).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortalLink {
    pub poly: usize,
    pub from: u32,
    pub to: u32,
}

/// A level's room portals: the polygons of chunk 0x102 (0x4C3BE0: count, then per polygon a
/// point count and that many float triples, in instance-position space) and the links of
/// chunk 0x103. The original walks them each frame from the camera's room (0x4BC460).
#[derive(Debug, Clone, Default)]
pub struct Portals {
    pub polys: Vec<Vec<[f32; 3]>>,
    pub links: Vec<PortalLink>,
}

impl Portals {
    /// Reads chunks 0x102 / 0x103 from a level's `.ngn` bytes; empty when absent or malformed.
    pub fn parse(bytes: &[u8]) -> Portals {
        let mut out = Portals::default();
        let Ok(chunks) = crate::formats::ngn::chunks(bytes) else { return out };
        let u32_at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
        let f32_at = |o: usize| u32_at(o).map(f32::from_bits);
        for ch in &chunks {
            let mut o = ch.data.start;
            match ch.id {
                0x102 => {
                    let Some(n) = u32_at(o) else { continue };
                    o += 4;
                    for _ in 0..n {
                        let Some(k) = u32_at(o).filter(|&k| o + 4 + k as usize * 12 <= ch.data.end) else { break };
                        o += 4;
                        let pts = (0..k as usize).map(|j| [0, 1, 2].map(|a| f32_at(o + j * 12 + a * 4).unwrap())).collect();
                        o += k as usize * 12;
                        out.polys.push(pts);
                    }
                }
                0x103 => {
                    let Some(n) = u32_at(o) else { continue };
                    o += 4;
                    for _ in 0..n {
                        let (Some(p), Some(from), Some(to)) = (u32_at(o), u32_at(o + 4), u32_at(o + 8)) else { break };
                        o += 12;
                        out.links.push(PortalLink { poly: p as usize, from, to });
                    }
                }
                _ => {}
            }
        }
        out.links.retain(|l| l.poly < out.polys.len());
        out
    }
}

/// A screen rectangle in normalised device coordinates: x0, y0, x1, y1 (y up).
pub type Rect = [f32; 4];
/// The whole screen.
pub const FULL_SCREEN: Rect = [-1.0, -1.0, 1.0, 1.0];

impl Portals {
    /// The rooms seen from `start` this frame, as a bit mask (bit `r` = room `r`; room 0, the
    /// always-drawn set, included). `clip` takes a point in instance-position space to clip
    /// space (w = view depth). Links into room 15 are dead unless `room15` (0x4BC2C0 drops them
    /// outside level 10).
    ///
    /// As 0x4BC460: the start room is drawn, then each link out of it, used at most once a
    /// frame (+0x10, cleared by 0x4BC170), not leading straight back to the room it came from,
    /// whose polygon isn't wholly outside the current view (0x4BA2F0) and survives 0x4BACF0 —
    /// the polygon's screen box, cut down to the current rectangle, must be non-empty and the
    /// polygon must face the camera; with a corner at or behind the eye it passes uncut — leads
    /// on with the view narrowed to that box. Room 0 is drawn after, with the whole view
    /// (0x4CDDD0's second 0x4BC460 call).
    pub fn visible_rooms(&self, start: u32, clip: impl Fn([f32; 3]) -> glam::Vec4, room15: bool) -> u64 {
        let polys: Vec<Vec<glam::Vec4>> = self.polys.iter().map(|p| p.iter().map(|&v| clip(v)).collect()).collect();
        let mut used = vec![false; self.links.len()];
        let mut mask = 1u64;
        self.walk(start, FULL_SCREEN, None, &polys, &mut used, &mut mask, room15);
        mask
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(&self, room: u32, rect: Rect, came_from: Option<u32>, polys: &[Vec<glam::Vec4>], used: &mut [bool], mask: &mut u64, room15: bool) {
        if room < 64 {
            *mask |= 1 << room;
        }
        for (i, l) in self.links.iter().enumerate() {
            if l.from != room || used[i] || Some(l.to) == came_from || (l.to == 15 && !room15) || l.to >= 64 {
                continue;
            }
            let p = &polys[l.poly];
            if outside(p, rect) {
                continue;
            }
            let Some(narrowed) = narrow(p, rect) else { continue };
            used[i] = true;
            self.walk(l.to, narrowed, Some(room), polys, used, mask, room15);
        }
    }
}

/// 0x4BA2F0: the polygon (clip space) lies wholly outside one side of the view through `r`,
/// or wholly behind the eye.
fn outside(p: &[glam::Vec4], r: Rect) -> bool {
    let all = |f: &dyn Fn(&glam::Vec4) -> bool| !p.is_empty() && p.iter().all(f);
    all(&|v| v.w <= 0.0) || all(&|v| v.x < r[0] * v.w) || all(&|v| v.x > r[2] * v.w) || all(&|v| v.y < r[1] * v.w) || all(&|v| v.y > r[3] * v.w)
}

/// 0x4BACF0: the view through a portal polygon (clip space) from inside rectangle `r`.
fn narrow(p: &[glam::Vec4], r: Rect) -> Option<Rect> {
    // A corner at or behind the eye: passed whole, the view not narrowed.
    if p.len() < 3 || p.iter().any(|v| v.w <= 1e-3) {
        return Some(r);
    }
    let s: Vec<glam::Vec2> = p.iter().map(|v| glam::Vec2::new(v.x / v.w, v.y / v.w)).collect();
    // Facing: its first three corners wind clockwise on screen (y up), as the data's portals
    // do seen from the room they lead out of (tests/rooms.rs checks both directions).
    let (a, b) = (s[1] - s[0], s[1] - s[2]);
    if a.perp_dot(b) < 0.0 {
        return None;
    }
    let lo = s.iter().fold(glam::Vec2::splat(f32::MAX), |m, v| m.min(*v));
    let hi = s.iter().fold(glam::Vec2::splat(f32::MIN), |m, v| m.max(*v));
    let n = [lo.x.max(r[0]), lo.y.max(r[1]), hi.x.min(r[2]), hi.y.min(r[3])];
    (n[0] < n[2] && n[1] < n[3]).then_some(n)
}

/// An instance's centre (0x4C3740's +0x74: the mesh's centre through the instance's matrix, plus
/// its position), taken here as the middle of the mesh's vertex box.
pub fn instance_centre(mesh: &Mesh, inst: &crate::formats::ngn::Instance) -> [f32; 3] {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &mesh.vertices {
        for k in 0..3 {
            lo[k] = lo[k].min(v.pos[k]);
            hi[k] = hi[k].max(v.pos[k]);
        }
    }
    if lo[0] > hi[0] {
        return inst.pos;
    }
    let c = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);
    let m = instance_matrix(inst);
    let t = transform(Some(&m), c);
    [0, 1, 2].map(|k| t[k] + inst.pos[k])
}
