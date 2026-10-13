//! Debug exporters for inspecting parsed assets in external tools (Blender etc.).

use crate::formats::all::Model;
use std::fmt::Write;

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
