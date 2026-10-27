//! Read-only installed-data audit across campaign levels.
use std::{env, fs, path::PathBuf};
use ts2::{campaign, formats::{packets, ngn, terrain, leveldat, exe::Exe}, game::{World, Player}, specials::Specials, boxes::Boxes, script};
use glam::Vec3;
fn main() {
 let game=PathBuf::from(env::var("TS2_GAME_DIR").expect("TS2_GAME_DIR required"));
 let e=Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
 for lv in campaign::LEVELS {
  let path=lv.path(&game);
  let pk=packets::parse(&fs::read(path.with_extension("raw")).unwrap()).unwrap();
  let bg=pk.iter().find_map(packets::backdrop);
  let rgba=pk.iter().find_map(packets::backdrop_rgba);
  println!("campaign {} {} backdrop {:?} decoded={}",lv.number,lv.name,bg,rgba.is_some());
  if ![1,2,5].contains(&lv.number) {continue}
  let n=ngn::parse(&fs::read(&path).unwrap()).unwrap();
  let t=terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
  let lists=leveldat::parse(&fs::read(path.with_extension("dat")).unwrap()).unwrap();
  for (m,s,yaw) in match lv.number {1=>vec![(15,15,0)],2=>vec![(0,0,0x961),(1,1,0xB4A)],_=>vec![(2,4,0),(1,6,0)]} {
   let w=World::from_terrain(&t); let mut sp=Specials::new(&n);
   let mut sc=script::for_level(lv.internal_id,&w,&mut sp,Some(&e),&lists,None).unwrap();
   let bx=Boxes::new(lv.internal_id,&lists,Some(&e),&w,&mut sp);
   let pivot=Vec3::from(sp.pos_obj(s).unwrap().map(|v|v as f32/32.));
   let angle=yaw as f32/4096.*std::f32::consts::TAU;
   let at=pivot+Vec3::new(angle.cos(),0.,-angle.sin())*-350.;
   let floor=w.floor_below_obj(at.x,at.z,at.y-2000.);
   let mut p=Player::new(Vec3::new(at.x,floor.map_or(at.y,|v|v.0)-100.,at.z));
   println!(" saw {m}/{s} pivot={pivot:?} collision={:?} initialfloor={floor:?}",w.mover_pos(m));
   let mut contact=0;
   for _ in 0..400 { w.clear_motion(); sc.step(&w,&mut sp,&mut p,&bx); p.tick(&w,[0.;2],false,false); if p.floor_mover.is_some_and(|v|v.0==m){contact+=1;} }
   println!(" final angle={:?} contact_ticks={contact} pos={:?} floor={:?}",sp.angles(s),p.pos,p.floor_mover);
  }
 }
}

