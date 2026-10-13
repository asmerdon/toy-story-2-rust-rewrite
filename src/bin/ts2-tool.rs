//! Asset inspection CLI.
//!
//!   ts2-tool scan-models <game_dir>
//!   ts2-tool export-obj  <file.all> <out.obj>
//!   ts2-tool export-all  <game_dir> <out_dir>

use std::path::{Path, PathBuf};
use std::{env, fs, process};
use ts2::export::{creature_to_obj, level_to_obj, model_to_obj};
use ts2::formats::all::{self, Stop};
use ts2::formats::{anim, ngn};

const OBJ_SCALE: f32 = 1.0 / 256.0;

fn main() {
    let args: Vec<String> = env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("scan-models") if args.len() == 3 => scan_models(Path::new(&args[2])),
        Some("export-obj") if args.len() == 4 => export_obj(Path::new(&args[2]), Path::new(&args[3])),
        Some("export-all") if args.len() == 4 => export_all(Path::new(&args[2]), Path::new(&args[3])),
        Some("scan-ngn") if args.len() == 3 => scan_ngn(Path::new(&args[2])),
        Some("export-level") if args.len() == 4 => export_level(Path::new(&args[2]), Path::new(&args[3]), true),
        Some("export-level-raw") if args.len() == 4 => export_level(Path::new(&args[2]), Path::new(&args[3]), false),
        Some("anim-info") if args.len() == 4 => anim_info(Path::new(&args[2]), &args[3]),
        Some("export-pose") if args.len() == 7 => export_pose(Path::new(&args[2]), &args[3], &args[4], &args[5], Path::new(&args[6])),
        _ => {
            eprintln!(
                "usage:\n  ts2-tool scan-models <game_dir>\n  ts2-tool export-obj <file.all> <out.obj>\n  ts2-tool export-all <game_dir> <out_dir>\n  \
                 ts2-tool scan-ngn <game_dir>\n  ts2-tool export-level <file.ngn> <out_dir>\n  ts2-tool export-level-raw <file.ngn> <out_dir>   (meshes untransformed)\n  \
                 ts2-tool export-pose <file.ngn> <creature> <anim|all> <frame|all> <out_dir>
  ts2-tool anim-info <file.ngn> <creature>"
            );
            process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

type Res = Result<(), Box<dyn std::error::Error>>;

fn model_files(game_dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let data = game_dir.join("data");
    let mut files = Vec::new();
    for dir in fs::read_dir(&data)? {
        let dir = dir?.path();
        let is_chars = dir.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_ascii_lowercase().starts_with("chars"));
        if !dir.is_dir() || !is_chars {
            continue;
        }
        for f in fs::read_dir(&dir)? {
            let f = f?.path();
            if f.extension().is_some_and(|e| e.eq_ignore_ascii_case("all")) {
                files.push(f);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn label(path: &Path) -> String {
    let dir = path.parent().and_then(Path::file_name).unwrap_or_default().to_string_lossy();
    let name = path.file_stem().unwrap_or_default().to_string_lossy();
    format!("{dir}/{name}")
}

fn scan_models(game_dir: &Path) -> Res {
    let files = model_files(game_dir)?;
    let (mut full, mut total_polys) = (0, 0);
    println!("{:<20} {:>6} {:>6} {:>6} {:>8}  stop", "model", "parts", "polys", "norms", "geom%");
    for f in &files {
        let m = all::parse(&fs::read(f)?)?;
        let norms: usize = m.parts.iter().map(|p| p.normals.len()).sum();
        let stop = match m.stop {
            Stop::GeometryEnd => {
                full += 1;
                "end marker".to_string()
            }
            Stop::Unknown(w) => format!("unknown block {w:08x}"),
            Stop::Eof => "eof".to_string(),
        };
        total_polys += m.poly_count();
        println!(
            "{:<20} {:>6} {:>6} {:>6} {:>7.1}%  {stop} @ {:#x}",
            label(f),
            m.parts.len(),
            m.poly_count(),
            norms,
            100.0 * m.stop_offset as f64 / m.file_len as f64,
            m.stop_offset
        );
    }
    println!("\n{} models, {} polygons, {} reached the geometry end marker", files.len(), total_polys, full);
    Ok(())
}

fn export_obj(input: &Path, output: &Path) -> Res {
    let m = all::parse(&fs::read(input)?)?;
    fs::write(output, model_to_obj(&m, OBJ_SCALE))?;
    println!("{} -> {} ({} parts, {} polys)", input.display(), output.display(), m.parts.len(), m.poly_count());
    Ok(())
}

fn ngn_files(game_dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    for dir in fs::read_dir(game_dir.join("data"))? {
        let dir = dir?.path();
        if !dir.is_dir() {
            continue;
        }
        for f in fs::read_dir(&dir)? {
            let f = f?.path();
            if f.extension().is_some_and(|e| e.eq_ignore_ascii_case("ngn")) {
                files.push(f);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn scan_ngn(game_dir: &Path) -> Res {
    println!("{:<22} {:>5} {:>7} {:>8} {:>8} {:>9}  chunks", "file", "tex", "layers", "meshes", "insts", "tris");
    for f in ngn_files(game_dir)? {
        let b = fs::read(&f)?;
        match ngn::parse(&b) {
            Ok(n) => {
                let meshes: usize = n.layers.iter().map(|l| l.meshes.len()).sum();
                let insts: usize = n.layers.iter().map(|l| l.instances.len()).sum();
                let tris: usize = n.layers.iter().flat_map(|l| &l.meshes).flat_map(|m| &m.groups).map(|g| g.triangles().len()).sum();
                let ids: Vec<String> = n.chunks.iter().map(|c| format!("{:x}", c.id)).collect();
                println!("{:<22} {:>5} {:>7} {:>8} {:>8} {:>9}  {}", label(&f), n.textures.len(), n.layers.len(), meshes, insts, tris, ids.join(" "));
            }
            Err(e) => println!("{:<22} ERROR {e}", label(&f)),
        }
    }
    Ok(())
}

fn export_level(input: &Path, out_dir: &Path, apply_instances: bool) -> Res {
    let n = ngn::parse(&fs::read(input)?)?;
    fs::create_dir_all(out_dir)?;
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    let mtl_name = format!("{stem}.mtl");
    let (obj, mtl) = level_to_obj(&n, &mtl_name, apply_instances);
    fs::write(out_dir.join(format!("{stem}.obj")), obj)?;
    fs::write(out_dir.join(&mtl_name), mtl)?;
    for t in &n.textures {
        fs::write(out_dir.join(format!("{}.bmp", t.name)), &t.bmp)?;
    }
    for c in &n.creatures {
        let name = format!("creature_{:02}_{}.obj", c.id, c.name.to_ascii_lowercase());
        fs::write(out_dir.join(name), creature_to_obj(c, &mtl_name, None))?;
    }
    println!(
        "{} -> {} ({} textures, {} layers, {} creatures)",
        input.display(),
        out_dir.display(),
        n.textures.len(),
        n.layers.len(),
        n.creatures.len()
    );
    Ok(())
}

/// Writes `<creature>_a<anim>_f<frame>.obj` for the requested animation(s) and frame(s).
fn export_pose(input: &Path, creature: &str, anim_sel: &str, frame_sel: &str, out_dir: &Path) -> Res {
    let n = ngn::parse(&fs::read(input)?)?;
    let c = n
        .creatures
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(creature))
        .ok_or_else(|| format!("no creature {creature} in {}; have {:?}", input.display(), n.creatures.iter().map(|c| &c.name).collect::<Vec<_>>()))?;
    fs::create_dir_all(out_dir)?;
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    let mtl_name = format!("{stem}.mtl");
    let anims: Vec<usize> = if anim_sel == "all" { (0..c.animations.len()).collect() } else { vec![anim_sel.parse()?] };
    let mut written = 0;
    for ai in anims {
        let raw = c.animations.get(ai).ok_or_else(|| format!("{} has {} animations", c.name, c.animations.len()))?;
        let a = anim::parse(raw)?;
        let frames: Vec<usize> = if frame_sel == "all" { (0..a.frame_count as usize).collect() } else { vec![frame_sel.parse()?] };
        for f in frames {
            let name = format!("{}_a{ai:02}_f{f:03}.obj", c.name.to_ascii_lowercase());
            fs::write(out_dir.join(name), creature_to_obj(c, &mtl_name, Some((&a, f))))?;
            written += 1;
        }
    }
    println!("wrote {written} posed OBJ(s) to {} (textures/MTL come from export-level of the same file)", out_dir.display());
    Ok(())
}

/// Prints each animation's header and node table (record offset, `-1/-2/-3` markers, `s` = scale).
fn anim_info(input: &Path, creature: &str) -> Res {
    let n = ngn::parse(&fs::read(input)?)?;
    let c = n.creatures.iter().find(|c| c.name.eq_ignore_ascii_case(creature)).ok_or_else(|| format!("no creature {creature}"))?;
    println!("{}: {} nodes, {} animations", c.name, c.nodes.len(), c.animations.len());
    for (i, raw) in c.animations.iter().enumerate() {
        let a = anim::parse(raw)?;
        let table: Vec<String> = a
            .raw_table
            .iter()
            .zip(&a.has_scale)
            .map(|(&v, &s)| if v < 0 { v.to_string() } else { format!("{v}{}", if s { "s" } else { "" }) })
            .collect();
        println!("a{i:02} frames {:3} stride {:3} root {:3} [{}]", a.frame_count, a.stride, a.root_node, table.join(" "));
    }
    Ok(())
}

fn export_all(game_dir: &Path, out_dir: &Path) -> Res {
    fs::create_dir_all(out_dir)?;
    for f in model_files(game_dir)? {
        let out = out_dir.join(format!("{}.obj", label(&f).replace('/', "_")));
        export_obj(&f, &out)?;
    }
    Ok(())
}
