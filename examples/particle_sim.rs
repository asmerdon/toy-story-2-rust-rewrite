//! Diagnostic: one particle of a type and spawn mode, tick by tick, with no floor:
//! `particle_sim <type hex> <mode hex> [ticks] [floor y]`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let typ = u8::from_str_radix(a[1].trim_start_matches("0x"), 16).unwrap();
    let mode = usize::from_str_radix(a[2].trim_start_matches("0x"), 16).unwrap();
    let ticks: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(40);
    let floor: Option<i32> = a.get(4).and_then(|s| s.parse().ok());
    let game = std::path::PathBuf::from(std::env::var("TS2_GAME_DIR").unwrap());
    let e = ts2::formats::exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let mut p = ts2::particles::Particles::new(Some(&e), vec![0x80; 64]);
    let w = ts2::game::World::from_triangles(vec![]);
    let k = p.spawn([0, 0, 0], typ, mode).unwrap();
    if floor.is_some() {
        p.slots[k].floor = floor;
    }
    for t in 0..ticks {
        let q = p.slots[k];
        println!("tick {t}: pos {:?} vel {:?} gravity {} life {} flags {:#x} typ {:#x}", q.pos, q.vel, q.gravity, q.life, q.flags, q.typ);
        p.tick(&w, ([0x100000, 0, 0], 0));
    }
}
