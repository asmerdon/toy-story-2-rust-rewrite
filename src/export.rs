//! Debug exporters for inspecting parsed assets in external tools (Blender etc.).

use crate::formats::all::Model;
use crate::formats::ngn::{Instance, Layer, Ngn};
use std::fmt::Write;

const LEVEL_SCALE: f32 = 1.0 / 256.0;

/// World position of a mesh vertex: translation only.
/// Rotation and scale are already baked into the vertices. Applying them reduces the number of
/// vertices shared between neighbouring pieces (level01: 507 → 388 seams for rotated instances).
fn instance_transform(inst: &Instance, p: [f32; 3]) -> [f32; 3] {
    [p[0] + inst.pos[0], p[1] + inst.pos[1], p[2] + inst.pos[2]]
}

/// Level geometry as OBJ + MTL. Textures are referenced as `<name>.bmp` next to the OBJ.
/// Returns (obj, mtl).
pub fn level_to_obj(ngn: &Ngn, mtl_name: &str, apply_instances: bool) -> (String, String) {
    let mut obj = format!("mtllib {mtl_name}\n");
    let mut mtl = String::new();
    for t in &ngn.textures {
        writeln!(mtl, "newmtl {0}\nKd 1 1 1\nmap_Kd {0}.bmp\n", t.name).unwrap();
    }
    writeln!(mtl, "newmtl untextured\nKd 0.8 0.8 0.8\n").unwrap();

    let mut base = 1usize;
    for (li, layer) in ngn.layers.iter().enumerate() {
        for (ii, (inst, mesh)) in layer_items(layer, apply_instances).enumerate() {
            writeln!(obj, "o L{li}_{ii:04}_{}", mesh.name).unwrap();
            for v in &mesh.vertices {
                let [x, y, z] = inst.map_or(v.pos, |i| instance_transform(i, v.pos)).map(|c| c * LEVEL_SCALE);
                // Bytes 1..=3 treated as RGB with 0x80 = 1.0 (PS1 convention); unconfirmed.
                let [r, g, b] = [v.color[1], v.color[2], v.color[3]].map(|c| (c as f32 / 128.0).min(1.0));
                writeln!(obj, "v {x} {} {z} {r:.3} {g:.3} {b:.3}", -y).unwrap();
            }
            for v in &mesh.vertices {
                writeln!(obj, "vt {} {}", v.uv[0], 1.0 - v.uv[1]).unwrap();
            }
            for g in &mesh.groups {
                let mat = mesh.group_texture(g).unwrap_or("untextured");
                writeln!(obj, "usemtl {mat}").unwrap();
                for [a, b, c] in g.triangles() {
                    // Y is flipped, so reverse winding.
                    let [a, b, c] = [c, b, a].map(|i| base + i as usize);
                    writeln!(obj, "f {a}/{a} {b}/{b} {c}/{c}").unwrap();
                }
            }
            base += mesh.vertices.len();
        }
    }
    (obj, mtl)
}

/// One creature's node meshes as OBJ (one object per node), using the level's MTL.
/// With `pose = Some((anim, frame))`, each node's mesh is transformed by its decoded pose
/// (row-vector convention: v' = v · M, model space, no hierarchy).
/// Smooth-joint geometry from the shared vertex buffer (tag 0x207) is not included yet.
pub fn creature_to_obj(c: &crate::formats::ngn::Creature, mtl_name: &str, pose: Option<(&crate::formats::anim::Animation, usize)>) -> String {
    let mut obj = format!("mtllib {mtl_name}\n");
    let mut base = 1usize;
    for (ni, node) in c.nodes.iter().enumerate() {
        let Some(mesh) = &node.mesh else { continue };
        let m = pose.and_then(|(a, f)| a.pose(ni, f)).map(|p| p.matrix());
        let xf = |p: [f32; 3], w: f32| match &m {
            Some(m) => [0, 1, 2].map(|j| p[0] * m[0][j] + p[1] * m[1][j] + p[2] * m[2][j] + w * m[3][j]),
            None => p,
        };
        writeln!(obj, "o {:02}_{}", ni, node.name).unwrap();
        for v in &mesh.vertices {
            let [x, y, z] = xf(v.pos, 1.0).map(|c| c * LEVEL_SCALE);
            writeln!(obj, "v {x} {} {z}", -y).unwrap();
        }
        for v in &mesh.vertices {
            writeln!(obj, "vt {} {}", v.uv[0], 1.0 - v.uv[1]).unwrap();
        }
        for v in &mesh.vertices {
            let [x, y, z] = xf(v.normal, 0.0);
            writeln!(obj, "vn {x} {} {z}", -y).unwrap();
        }
        for g in &mesh.groups {
            writeln!(obj, "usemtl {}", mesh.group_texture(g).unwrap_or("untextured")).unwrap();
            for [a, b, c] in g.triangles() {
                let [a, b, c] = [c, b, a].map(|i| base + i as usize);
                writeln!(obj, "f {a}/{a}/{a} {b}/{b}/{b} {c}/{c}/{c}").unwrap();
            }
        }
        base += mesh.vertices.len();
    }
    obj
}

fn layer_items(layer: &Layer, apply_instances: bool) -> Box<dyn Iterator<Item = (Option<&Instance>, &crate::formats::ngn::Mesh)> + '_> {
    if apply_instances && !layer.instances.is_empty() {
        Box::new(layer.instances.iter().filter_map(|i| layer.meshes.get(i.mesh as usize).map(|m| (Some(i), m))))
    } else {
        Box::new(layer.meshes.iter().map(|m| (None, m)))
    }
}

/// Wavefront OBJ with per-vertex colours (`v x y z r g b`, which Blender imports).
/// One `o` object per part. Y is flipped so the model stands upright (PS1 +Y is down).
/// Parts are written in their stored coordinates; if they are bone-local they will overlap
/// until the skeleton from the trailer / `.anm` is applied.
pub fn model_to_obj(model: &Model, scale: f32) -> String {
    let mut s = String::new();
    let mut base = 1usize;
    for (pi, part) in model.parts.iter().enumerate() {
        if part.polys.is_empty() {
            continue;
        }
        writeln!(s, "o part_{pi:02}").unwrap();
        for poly in &part.polys {
            for (v, c) in poly.verts.iter().zip(poly.colors.iter()) {
                let [x, y, z] = v.pos.map(|c| c as f32 * scale);
                let [r, g, b] = c.map(|c| c as f32 / 255.0);
                writeln!(s, "v {x} {} {z} {r:.3} {g:.3} {b:.3}", -y).unwrap();
            }
        }
        for poly in &part.polys {
            // Quads are stored in perimeter order (0-1-2-3), not PS1 strip order.
            // Flipping Y mirrors the model, so reverse winding to keep faces outward.
            if poly.verts.len() == 4 {
                writeln!(s, "f {} {} {} {}", base + 3, base + 2, base + 1, base).unwrap();
            } else {
                writeln!(s, "f {} {} {}", base + 2, base + 1, base).unwrap();
            }
            base += poly.verts.len();
        }
    }
    s
}
