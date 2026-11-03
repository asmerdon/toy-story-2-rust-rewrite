//! Diagnostic: the follow camera while Buzz rides level 10's lift 1 up (`rise` a tick):
//! per tick, Buzz's height, the camera's height above him and its distance:
//! `lift_camera <level10 dir> rise`.
use glam::Vec3;
use ts2::game::{Input, Player, World};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let rise: f32 = a[2].parse().unwrap();
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let m = 1;
    let at = world.mover_pos(m).unwrap();
    // The car's floor under its middle.
    let mut y = at.y - 9000.0;
    let floor = loop {
        let Some((f, o)) = world.floor_below_obj(at.x, at.z, y) else { panic!("no floor") };
        if o == Some(m) && f > at.y - 6000.0 { break f; }
        y = f + 1.0;
    };
    let mut p = Player::new(Vec3::new(at.x, floor - 2.0, at.z));
    let dt = 1.0 / 60.0;
    for _ in 0..20 { p.update(&world, Input::default(), dt); }
    let mut cam = ts2::camera::Camera::new(&p);
    for _ in 0..60 { p.update(&world, Input::default(), dt); cam.tick(&p, &world); }
    for k in 0..120 {
        p.update_with(&world, Input::default(), dt, &mut |_, w| w.move_mover(m, Vec3::new(0.0, -rise, 0.0)));
        cam.tick(&p, &world);
        let c = cam.position();
        println!("{k:3} buzz y {:8.1} cam above {:7.1} dist {:6.1}", p.pos.y, p.pos.y - c.y, (c - p.pos).length());
    }
}
