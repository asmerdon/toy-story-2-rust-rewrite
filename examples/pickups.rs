//! Diagnostic: dump a level's pickup list (level.dat list 63) next to its .ngn special objects
//! and texture names. Run: cargo run --release --example pickups -- <levelNN dir>
use ts2::formats::ngn;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let stem = std::env::args().nth(2).unwrap_or_else(|| "level".into());
    let dat = std::fs::read(dir.join(format!("{stem}.dat"))).unwrap();
    let rd = |o: usize| i32::from_le_bytes(dat[o..o + 4].try_into().unwrap());
    let n = rd(0);
    let mut o = 4;
    for _ in 0..n {
        let head = rd(o);
        let count = (head & 0xFFFF) as i16 as i32;
        let kind = (head >> 16) as i16;
        let words = if kind < 0 { (count * 3 + 1) / 2 + 4 } else if kind == 0x3F { count * 4 + 1 } else { count * 3 + 1 };
        if kind == 0x3F {
            println!("list 63: {count} entries");
            for k in 0..count as usize {
                let e = o + 4 + 16 * k;
                println!("  {k:3}: {:7} {:7} {:7}  id {:3} r {:#04x} w {:#06x}", rd(e), rd(e + 4), rd(e + 8), dat[e + 12], dat[e + 13], u16::from_le_bytes([dat[e + 14], dat[e + 15]]));
            }
        }
        o += words.max(1) as usize * 4;
    }
    if let Ok(game) = std::env::var("TS2_GAME_DIR") {
        let exe = ts2::formats::exe::Exe::parse(std::fs::read(std::path::Path::new(&game).join("toy2.exe")).unwrap()).unwrap();
        for va in [0x4DC020u32, 0x4DC028, 0x4DC02C, 0x4DC030, 0x4DC034, 0x4DC038] {
            println!("float {va:#x} = {}", f32::from_le_bytes(exe.at(va, 4).unwrap().try_into().unwrap()));
        }
        let names: std::collections::HashMap<u16, String> = ts2::audio::slot_names(&exe, 1).unwrap().into_iter().collect();
        for id in [0x29u32, 0x33, 0x4F, 0xA9, 0x2D, 0x31, 0x3E, 0x3F, 0x5E, 0x4C, 0x50] {
            let d = exe.at(0x502950 + 16 * id, 16).unwrap();
            let slot = u16::from_le_bytes([d[0], d[1]]);
            println!("sound {id:#x}: slot {slot:#x} {:?} vol {}", names.get(&(slot & 0x3FFF)), i16::from_le_bytes([d[4], d[5]]));
        }
        println!("slot 0x29 = {:?}", names.get(&0x29));
        for i in 0..50u32 {
            let p = u32::from_le_bytes(exe.at(0x4EC0A4 + 4 * i, 4).unwrap().try_into().unwrap());
            if p == 0 {
                continue;
            }
            let d = exe.at(p, 0x48).unwrap();
            println!("sprite {i:2} @ {p:#x}: tex {} w {} h {} | {:02x?}", i16::from_le_bytes([d[0], d[1]]), d[2], d[3], &d[4..0x30]);
        }
    }
    let n = ngn::parse(&std::fs::read(dir.join(format!("{stem}.ngn"))).unwrap()).unwrap();
    println!("textures: {}", n.textures.len());
    for (i, t) in n.textures.iter().enumerate() {
        println!("  {i:3} {} {:?}", t.name, t.dimensions());
    }
    let objs = ts2::formats::leveldat::special_objects(&dat).unwrap();
    println!("specials: {} (.ngn), {} (level.dat table)", n.specials.len(), objs.len());
    for s in &n.specials {
        let inst = &n.layers[s.layer].instances[s.instance];
        let o = objs.get(s.id as usize).copied().flatten();
        println!("  id {:3} layer {} inst {:4} pos {:?} | {:?}", s.id, s.layer, s.instance, inst.pos, o);
    }
}
