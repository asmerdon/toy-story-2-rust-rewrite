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
    add_mesh_parts(ngn, out, None, mesh, place, m);
}

/// Like `add_mesh`; with `billboards`, billboard groups go there instead of into `out`.
fn add_mesh_parts(ngn: &Ngn, out: &mut Vec<Batch>, mut billboards: Option<&mut Vec<Billboard>>, mesh: &Mesh, place: Place, m: Option<&[[f32; 4]; 4]>) {
    for g in &mesh.groups {
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
    let (mut out, mut bb) = (Vec::new(), Vec::new());
    for (li, layer) in ngn.layers.iter().enumerate() {
        for (_, inst) in layer.instances.iter().enumerate().filter(|(ii, i)| keep(li, *ii, i)) {
            if let Some(mesh) = layer.meshes.get(inst.mesh as usize) {
                let m = instance_matrix(inst);
                add_mesh_parts(ngn, &mut out, Some(&mut bb), mesh, Place::new(inst.pos, 0.0), Some(&m));
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
    add_mesh_parts(ngn, &mut out, Some(billboards), mesh, Place::new(s.pos, 0.0), Some(&m));
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
    let corner = |sx: f32, sy: f32, u: f32, v: f32| GpuVertex {
        pos: [0, 1, 2].map(|k| pos[k] + (right[k] * sx + up[k] * sy) * half),
        uv: [u, v],
        color,
    };
    let [[u0, v0], [u1, v1]] = uv;
    let (a, b, c, d) = (corner(-1.0, 1.0, u0, v0), corner(1.0, 1.0, u1, v0), corner(1.0, -1.0, u1, v1), corner(-1.0, -1.0, u0, v1));
    push(out, texture, Blend::Alpha, [a, b, c]);
    push(out, texture, Blend::Alpha, [a, c, d]);
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
    let mut place = Place::turned(origin, angles);
    for row in place.m.iter_mut() {
        for v in row.iter_mut() {
            *v *= scale;
        }
    }
    creature_batches_placed(ngn, c, place, anim, None)
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
