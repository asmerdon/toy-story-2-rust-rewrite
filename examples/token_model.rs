//! Exports the Pizza Planet token model (Level 1's first token, from the exe's token list
//! 0x4F0EDC) as OBJ + MTL + its BMP textures, for inspection in Blender.
//!
//!     cargo run --release --example token_model -- <game_dir> <out_dir>
//!
//! Output is game data: write it under out/ (git-ignored), never into the repository.
use std::fmt::Write;
use std::path::PathBuf;

use ts2::formats::{exe, ngn};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let game = PathBuf::from(args.get(1).cloned().or_else(|| std::env::var("TS2_GAME_DIR").ok()).expect("game dir"));
    let out = PathBuf::from(args.get(2).map_or("out/token", String::as_str));
    let exe = exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let ids = exe.at(0x4F0EDC, 10).unwrap();
    let token = i16::from_le_bytes([ids[0], ids[1]]) as u16;
    let path = ts2::campaign::scene_path(&game, 1);
    let level = ngn::parse(&std::fs::read(&path).unwrap()).unwrap();
    let s = level.specials.iter().find(|s| s.id == token).expect("token special");
    let inst = &level.layers[s.layer].instances[s.instance];
    let mesh = &level.layers[s.layer].meshes[inst.mesh as usize];
    println!("{}: special {token:#x} -> layer {} instance {} mesh {} '{}'", path.display(), s.layer, s.instance, inst.mesh, mesh.name);
    println!("  {} vertices, textures {:?}", mesh.vertices.len(), mesh.textures);

    std::fs::create_dir_all(&out).unwrap();
    let mut obj = "mtllib token.mtl\no token\n".to_string();
    let mut mtl = String::new();
    for v in &mesh.vertices {
        let [x, y, z] = v.pos.map(|c| c / 256.0);
        let [r, g, b] = [v.color[1], v.color[2], v.color[3]].map(|c| (c as f32 / 128.0).min(1.0));
        writeln!(obj, "v {x} {} {} {r:.3} {g:.3} {b:.3}", -y, -z).unwrap();
    }
    for v in &mesh.vertices {
        writeln!(obj, "vt {} {}", v.uv[0], 1.0 - v.uv[1]).unwrap();
    }
    for g in &mesh.groups {
        let mat = mesh.group_texture(g).unwrap_or("untextured");
        writeln!(obj, "usemtl {mat}").unwrap();
        for t in g.triangles() {
            let [a, b, c] = t.map(|i| 1 + i as usize);
            writeln!(obj, "f {a}/{a} {b}/{b} {c}/{c}").unwrap();
        }
    }
    for name in &mesh.textures {
        writeln!(mtl, "newmtl {name}\nKd 1 1 1\nmap_Kd {name}.bmp\n").unwrap();
        if let Some(t) = level.textures.iter().find(|t| &t.name == name) {
            std::fs::write(out.join(format!("{name}.bmp")), &t.bmp).unwrap();
        }
    }
    mtl.push_str("newmtl untextured\nKd 0.8 0.8 0.8\n");
    std::fs::write(out.join("token.obj"), obj).unwrap();
    std::fs::write(out.join("token.mtl"), mtl).unwrap();
    println!("wrote {}", out.display());
}
