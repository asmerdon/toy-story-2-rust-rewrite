//! Asset inspection CLI.
//!
//!   ts2-tool scan-models <game_dir>
//!   ts2-tool export-obj  <file.all> <out.obj>
//!   ts2-tool export-all  <game_dir> <out_dir>

use std::path::{Path, PathBuf};
use std::{env, fs, process};
use ts2::export::model_to_obj;
use ts2::formats::all::{self, Stop};

const OBJ_SCALE: f32 = 1.0 / 256.0;

fn main() {
    let args: Vec<String> = env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("scan-models") if args.len() == 3 => scan_models(Path::new(&args[2])),
        Some("export-obj") if args.len() == 4 => export_obj(Path::new(&args[2]), Path::new(&args[3])),
        Some("export-all") if args.len() == 4 => export_all(Path::new(&args[2]), Path::new(&args[3])),
        _ => {
            eprintln!("usage:\n  ts2-tool scan-models <game_dir>\n  ts2-tool export-obj <file.all> <out.obj>\n  ts2-tool export-all <game_dir> <out_dir>");
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

fn export_all(game_dir: &Path, out_dir: &Path) -> Res {
    fs::create_dir_all(out_dir)?;
    for f in model_files(game_dir)? {
        let out = out_dir.join(format!("{}.obj", label(&f).replace('/', "_")));
        export_obj(&f, &out)?;
    }
    Ok(())
}
