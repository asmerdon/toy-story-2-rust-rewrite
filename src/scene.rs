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

fn push(batches: &mut Vec<Batch>, texture: Option<usize>, blend: Blend, tri: [GpuVertex; 3]) {
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
    sin: f32,
    cos: f32,
}

impl Place {
    fn new(offset: [f32; 3], yaw: f32) -> Place {
        let (sin, cos) = yaw.sin_cos();
        Place { offset, sin, cos }
    }
    fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let x = p[0] * self.cos + p[2] * self.sin;
        let z = -p[0] * self.sin + p[2] * self.cos;
        [x + self.offset[0], p[1] + self.offset[1], z + self.offset[2]]
    }
}

fn add_mesh(ngn: &Ngn, out: &mut Vec<Batch>, mesh: &Mesh, place: Place, m: Option<&[[f32; 4]; 4]>) {
    for g in &mesh.groups {
        let tex = mesh.group_texture(g).and_then(|n| texture_index(ngn, n));
        let blend = mesh.group_blend(g);
        for t in g.triangles() {
            let tri = t.map(|i| {
                let v = &mesh.vertices[i as usize];
                gpu(v, place.apply(transform(m, v.pos)))
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
    let mut out = Vec::new();
    for layer in &ngn.layers {
        for inst in layer.instances.iter().filter(|i| keep(i)) {
            if let Some(mesh) = layer.meshes.get(inst.mesh as usize) {
                add_mesh(ngn, &mut out, mesh, Place::new(inst.pos, 0.0), None);
            }
        }
    }
    out
}

/// A creature at `origin` (game units) turned by `yaw` radians about the vertical axis,
/// optionally posed by `anim` at fractional frame `t`.
pub fn creature_batches(ngn: &Ngn, c: &Creature, origin: [f32; 3], yaw: f32, anim: Option<(&Animation, f32)>) -> Vec<Batch> {
    creature_batches_layered(ngn, c, origin, yaw, anim, None)
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
    let mut out = Vec::new();
    let place = Place::new(origin, yaw);
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
