//! CPU-side scene building: turns parsed `.ngn` data into flat triangle batches per texture.
//! Kept free of GPU types so it can be tested headless.

use crate::formats::anim::Animation;
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

/// Triangles that share one texture (`None` = untextured).
#[derive(Debug, Clone, Default)]
pub struct Batch {
    pub texture: Option<usize>,
    pub vertices: Vec<GpuVertex>,
}

fn texture_index(ngn: &Ngn, name: &str) -> Option<usize> {
    ngn.textures.iter().position(|t| t.name == name)
}

fn push(batches: &mut Vec<Batch>, texture: Option<usize>, tri: [GpuVertex; 3]) {
    match batches.iter_mut().find(|b| b.texture == texture) {
        Some(b) => b.vertices.extend_from_slice(&tri),
        None => batches.push(Batch { texture, vertices: tri.to_vec() }),
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

fn add_mesh(ngn: &Ngn, out: &mut Vec<Batch>, mesh: &Mesh, offset: [f32; 3], m: Option<&[[f32; 4]; 4]>) {
    for g in &mesh.groups {
        let tex = mesh.group_texture(g).and_then(|n| texture_index(ngn, n));
        for t in g.triangles() {
            let tri = t.map(|i| {
                let v = &mesh.vertices[i as usize];
                let p = transform(m, v.pos);
                gpu(v, [p[0] + offset[0], p[1] + offset[1], p[2] + offset[2]])
            });
            push(out, tex, tri);
        }
    }
}

/// All level geometry, placed by its instances.
pub fn level_batches(ngn: &Ngn) -> Vec<Batch> {
    let mut out = Vec::new();
    for layer in &ngn.layers {
        for inst in &layer.instances {
            if let Some(mesh) = layer.meshes.get(inst.mesh as usize) {
                add_mesh(ngn, &mut out, mesh, inst.pos, None);
            }
        }
    }
    out
}

/// A creature at `origin` (game units), optionally posed by `anim` at fractional frame `t`.
pub fn creature_batches(ngn: &Ngn, c: &Creature, origin: [f32; 3], anim: Option<(&Animation, f32)>) -> Vec<Batch> {
    let mut out = Vec::new();
    let matrices: Vec<Option<[[f32; 4]; 4]>> =
        (0..c.nodes.len()).map(|n| anim.and_then(|(a, t)| a.matrix_at(n, t))).collect();
    for (n, node) in c.nodes.iter().enumerate() {
        if let Some(mesh) = &node.mesh {
            add_mesh(ngn, &mut out, mesh, origin, matrices[n].as_ref());
        }
    }
    for patch in &c.patches {
        let tex = c
            .shared_materials
            .get(patch.material as usize)
            .and_then(|m| m.texture)
            .and_then(|t| c.shared_textures.get(t as usize))
            .and_then(|name| texture_index(ngn, name));
        let verts: Vec<GpuVertex> = patch
            .vertices()
            .into_iter()
            .map(|(vi, ni)| {
                let v = &c.shared_vertices[vi];
                let p = transform(matrices.get(ni).and_then(|m| m.as_ref()), v.pos);
                gpu(v, [p[0] + origin[0], p[1] + origin[1], p[2] + origin[2]])
            })
            .collect();
        for t in patch.triangles() {
            push(&mut out, tex, t.map(|i| verts[i]));
        }
    }
    out
}

/// Appends `more` into `into`, merging batches that share a texture.
pub fn merge(into: &mut Vec<Batch>, more: Vec<Batch>) {
    for b in more {
        match into.iter_mut().find(|x| x.texture == b.texture) {
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
