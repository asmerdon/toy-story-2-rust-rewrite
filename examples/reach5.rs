//! Diagnostic: can Buzz, with a running jump (and a double jump) from the pier, touch each of
//! level 5's five challenge bones? Searches jump timings.
use glam::Vec3;
use ts2::formats::{exe, terrain};
use ts2::game::{Input, Player, World};
fn main() {
    let game = std::path::PathBuf::from(std::env::var_os("TS2_GAME_DIR").unwrap());
    let e = exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let dir = game.join("data/level05");
    let dat = std::fs::read(dir.join("level.dat")).unwrap();
    let t = terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let base = ts2::pickups::Pickups::new(5, &dat, Some(&e)).unwrap();
    let bones: Vec<(u8, [i32; 3])> = base.items.iter().filter(|p| (0x61..=0x65).contains(&p.id)).map(|p| (p.id, p.pos)).collect();
    let on_pier = |x: f32, z: f32| world.floor_below(x, z, -3000.0).is_some_and(|f| f < 1700.0);
    for (id, pos) in bones {
        let (bx, bz) = (pos[0] as f32, pos[2] as f32);
        let mut result = None;
        // Start points on the pier, in line with the bone, at various distances back from the edge.
        'search: for lateral in [0.0f32, -300.0, 300.0] {
            for back in [100.0f32, 400.0, 800.0] {
                let c = Vec3::new(3750.0, 0.0, 25500.0);
                let to = Vec3::new(bx - c.x, 0.0, bz - c.z).normalize();
                let side = Vec3::new(-to.z, 0.0, to.x);
                let mut edge = 0.0f32;
                while on_pier(c.x + to.x * edge, c.z + to.z * edge) {
                    edge += 20.0;
                }
                let s = c + to * (edge - back) + side * lateral;
                if !on_pier(s.x, s.z) {
                    continue;
                }
                for lead in [0.0f32, 40.0, 80.0, 160.0, 300.0] {
                    for second in [0, 10, 16, 22, 30, 40] {
                        for hold in [0, 20, 40] {
                            let mut p = Player::new(Vec3::new(s.x, 1557.0, s.z));
                            let mut pk = ts2::pickups::Pickups::new(5, &dat, Some(&e)).unwrap();
                            pk.show_collectables(true);
                            let mut jumped_at = None;
                            for tick in 0..300 {
                                let b = p.pos;
                                if jumped_at.is_none() && p.on_floor && !on_pier(b.x + to.x * lead, b.z + to.z * lead) {
                                    jumped_at = Some(tick);
                                }
                                let k = jumped_at.map_or(-1, |j| tick - j);
                                let jump = k == 0 || (second > 0 && k == second);
                                let held = k >= 0 && k < hold;
                                let input = Input { move_xz: [to.x, to.z], jump, jump_held: held || jump, ..Default::default() };
                                p.update_with(&world, input, 1.0 / 60.0, &mut |_, _| {});
                                let b = p.pos;
                                pk.tick([b.x, b.y, b.z], [b.x, b.y, b.z]);
                                if pk.collectables > 0 {
                                    result = Some(format!("from {s:?} lead {lead} second {second} hold {hold} at tick {tick} pos {b:?}"));
                                    break 'search;
                                }
                            }
                        }
                    }
                }
            }
        }
        println!("bone {id:#x} {pos:?}: {}", result.unwrap_or("NOT reached".into()));
    }
}
