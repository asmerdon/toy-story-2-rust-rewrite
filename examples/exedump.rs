//! Diagnostic: read data from toy2.exe by virtual address. Needs TS2_GAME_DIR.
//! Run: cargo run --release --example exedump -- <kind> <hex va> <count>
//! Kinds: `i32`, `i16`, `u8` (hex words), `str` (one C string), `strs` (a table of `count`
//! pointers to C strings).
use ts2::formats::exe::Exe;

fn cstr(exe: &Exe, va: u32) -> String {
    let mut s = Vec::new();
    for k in 0..256 {
        match exe.at(va + k, 1) {
            Ok([0]) | Err(_) => break,
            Ok(b) => s.push(b[0]),
        }
    }
    String::from_utf8_lossy(&s).into_owned()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(kind), Some(va)) = (args.get(1), args.get(2).and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())) else {
        eprintln!("usage: exedump <i32|i16|u8|str|strs> <hex va> [count]");
        return;
    };
    let n: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(16);
    let dir = std::path::PathBuf::from(std::env::var_os("TS2_GAME_DIR").expect("TS2_GAME_DIR"));
    let exe = Exe::parse(std::fs::read(dir.join("toy2.exe")).unwrap()).unwrap();
    let word = |va: u32, size: u32| -> Option<i64> {
        let b = exe.at(va, size as usize).ok()?;
        Some(match size {
            4 => i32::from_le_bytes(b.try_into().ok()?) as i64,
            2 => i16::from_le_bytes(b.try_into().ok()?) as i64,
            _ => b[0] as i64,
        })
    };
    match kind.as_str() {
        "str" => println!("{va:#x}: {:?}", cstr(&exe, va)),
        "strs" => {
            for k in 0..n {
                let p = word(va + 4 * k, 4).unwrap_or(0) as u32;
                println!("{k:3} {p:#x}: {:?}", cstr(&exe, p));
            }
        }
        _ => {
            let size = match kind.as_str() {
                "i32" => 4,
                "i16" => 2,
                _ => 1,
            };
            for row in 0..n.div_ceil(8) {
                let at = va + row * 8 * size;
                let words: Vec<String> = (0..8.min(n - row * 8)).filter_map(|k| word(at + k * size, size)).map(|w| format!("{w:#x}")).collect();
                println!("{at:#x}: {}", words.join(" "));
            }
        }
    }
}
