//! The pause menu: 0x49E330 runs instead of the frame (0x49DFE0) while 0x52B818 is set,
//! 0x49F4B0 takes the buttons and 0x49FD40 draws it.
//!
//! - Start (fresh, not in a demo, and not while the level is fading to black or being left)
//!   stops the sounds, plays 0x3D and opens "pause menu": continue, camera mode, volume control,
//!   exit level. The world stands still behind it.
//! - Up / down move the row (0x3F), jump picks (0x3D into a sub menu, 0x3E out), cancel backs out.
//! - Camera mode sets 0x52F0CC bit 0x40 (active) or clears it (passive). Volume control edits the
//!   sfx and bgm levels (0–10) with left / right, heard as they change; jump keeps them and cancel
//!   puts them back.
//! - Exit level asks "are you sure?"; yes leaves as 0x52B7DC = 5.
//! - Not ported: holding L2 + R2 (the coins-7 and coins-5 cheats) and the orbiting camera they
//!   start, and menu 4 ("exit game" from pad bit 1), neither reachable on the keyboard.

use crate::frontend::Pad;
use crate::hud::Quad;
use crate::pickups::SpriteDef;

/// 0x5039BC: rows in each menu.
const ROWS: [i32; 4] = [4, 2, 2, 2];

/// What the viewer must do about the buttons this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Out {
    /// A sound through 0x49E660.
    Sound(u16),
    /// 0x52B818 = 0, music back on.
    Resume,
    /// 0x52B7DC = 5.
    ExitLevel,
    /// 0x52F0CC bit 0x40.
    Camera(bool),
    /// 0x4A3ED0: the levels (0–10) to hear now.
    Volumes { music: i32, sfx: i32 },
}

/// 0x52ADB0 (menu), 0x52B7E4 (row), 0x529E48 (clock), 0x830E30 / 0x830E20 (the levels being edited).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pause {
    pub menu: usize,
    pub row: i32,
    clock: i32,
    music: i32,
    sfx: i32,
    /// 0x52F0CD / 0x52F0CE: the levels kept.
    pub kept: (i32, i32),
    prev: u32,
}

impl Pause {
    pub fn new(music: i32, sfx: i32) -> Pause {
        // The Start that opened it is still held.
        Pause { menu: 0, row: 0, clock: 0, music, sfx, kept: (music, sfx), prev: Pad::START }
    }

    /// 0x49F4B0 for one tick with the buttons held.
    pub fn tick(&mut self, pad: Pad) -> Vec<Out> {
        let mut out = Vec::new();
        let fresh = pad.0 & !self.prev;
        self.prev = pad.0;
        self.clock = (self.clock + 1) & 0x3F;
        let hit = |b: u32| fresh & b != 0;
        if hit(Pad::DOWN) && self.row < ROWS[self.menu] - 1 {
            out.push(Out::Sound(0x3F));
            self.row += 1;
        }
        if hit(Pad::UP) && self.row > 0 {
            out.push(Out::Sound(0x3F));
            self.row -= 1;
        }
        let cancel = hit(Pad::CANCEL);
        let mut resume = false;
        if hit(Pad::JUMP) || cancel {
            match self.menu {
                0 if !cancel => match self.row {
                    0 => {
                        out.push(Out::Sound(0x3E));
                        resume = true;
                    }
                    1 | 3 => {
                        out.push(Out::Sound(0x3D));
                        self.menu = self.row as usize;
                        self.row = 0;
                    }
                    _ => {
                        (self.music, self.sfx) = self.kept;
                        out.push(Out::Sound(0x3D));
                        self.menu = 2;
                        self.row = 0;
                    }
                },
                1 => {
                    if !cancel {
                        out.push(Out::Camera(self.row == 1));
                    }
                    self.menu = 0;
                    self.row = 0;
                    out.push(Out::Sound(0x3E));
                }
                2 => {
                    self.menu = 0;
                    self.row = 0;
                    if !cancel {
                        self.kept = (self.music, self.sfx);
                    }
                    out.push(Out::Volumes { music: self.kept.0, sfx: self.kept.1 });
                    out.push(Out::Sound(0x3E));
                }
                3 if cancel || self.row == 0 => {
                    self.row = 0;
                    self.menu = 0;
                    out.push(Out::Sound(0x3E));
                }
                3 => {
                    out.push(Out::Sound(0x3D));
                    out.push(Out::ExitLevel);
                }
                _ => {}
            }
        } else if self.menu == 2 {
            let level = if self.row == 0 { self.sfx } else { self.music };
            let mut v = level;
            if hit(Pad::RIGHT) && v < 10 {
                v += 1;
                out.push(Out::Sound(0x3D));
            }
            if hit(Pad::LEFT) && v > 0 {
                v -= 1;
                out.push(Out::Sound(0x3D));
            }
            if self.row == 0 {
                self.sfx = v;
            } else {
                self.music = v;
            }
            if v != level {
                out.push(Out::Volumes { music: self.music, sfx: self.sfx });
            }
        }
        if resume || hit(Pad::START) {
            if !resume {
                out.push(Out::Sound(0x3E));
            }
            out.push(Out::Resume);
        }
        out
    }

    /// 0x49FD40's menu: a title in yellow, the rows (the chosen one pulsing yellow), the hint.
    pub fn quads(&self, sprites: &[Option<SpriteDef>]) -> Vec<Quad> {
        let u = if self.clock > 0x1F { 0x3F - self.clock } else { self.clock };
        let pulse = (u * 2 + 0x40) as u8;
        let mut q = Vec::new();
        box_quads(&mut q, sprites, 0xA0, 0x51, 0xC0000, 0x41000, [0x80, 0, 0x80]);
        let (mut bars_sfx, mut bars_bgm) = (String::from("sfx "), String::from("bgm "));
        bars_sfx.push_str(&bar(self.sfx));
        bars_bgm.push_str(&bar(self.music));
        let (title, rows, first, step): (&str, Vec<&str>, i32, i32) = match self.menu {
            0 => ("pause menu", vec!["continue", "camera mode", "volume control", "exit level"], 0x60, 8),
            1 => ("camera mode", vec!["passive camera", "active camera"], 100, 8),
            2 => ("volume control", vec![&bars_sfx, &bars_bgm], 100, 8),
            _ => ("are you sure?", vec!["no", "yes"], 100, 8),
        };
        text(&mut q, sprites, 0x54, title, [0x80, 0x80, 0]);
        for (k, s) in rows.iter().enumerate() {
            let c = if k as i32 == self.row { [pulse, pulse, 0] } else { [0x80; 3] };
            text(&mut q, sprites, first + step * k as i32, s, c);
        }
        text(&mut q, sprites, 0x86, "jump to select", [0x80; 3]);
        q
    }
}

/// 0x49FD40: "demo mode" (y 0x70, yellow).
pub fn demo_text(sprites: &[Option<SpriteDef>]) -> Vec<Quad> {
    let mut q = Vec::new();
    text(&mut q, sprites, 0x70, "demo mode", [0x80, 0x80, 0]);
    q
}

fn bar(v: i32) -> String {
    (0..10).map(|k| if k < v { '*' } else { ' ' }).collect()
}

/// 0x401B60: a box of sprite 6 cell 1: four black edges and a translucent fill.
fn box_quads(q: &mut Vec<Quad>, sprites: &[Option<SpriteDef>], x: i32, y: i32, sx: i32, sy: i32, fill: [u8; 3]) {
    let Some(Some(def)) = sprites.get(6) else { return };
    let mut put = |x: i32, y: i32, sx: i32, sy: i32, color: [u8; 3], alpha: u8| {
        q.push(Quad {
            sprite: 6,
            cell: 1,
            pos: [x as f32, y as f32],
            size: [(def.size[0] as i32 * sx >> 12) as f32, (def.size[1] as i32 * sy >> 12) as f32],
            width: 512.0,
            color,
            alpha,
        });
    };
    put(x, y, 0x2000, sy, [0; 3], 0xFF);
    put((sx >> 12) - 2 + x, y, 0x2000, sy, [0; 3], 0xFF);
    put(x, y, sx, 0x1000, [0; 3], 0xFF);
    put(x, (sy >> 12) - 1 + y, sx, 0x1000, [0; 3], 0xFF);
    put(x + 2, y + 1, sx - 0x4000, sy - 0x2000, fill, 0x40);
}

/// 0x401FB0 + 0x49B630: centred on x 0xA0 of 320, one half-size cell of sprite 0x14 every 8.
fn text(q: &mut Vec<Quad>, sprites: &[Option<SpriteDef>], y: i32, s: &str, color: [u8; 3]) {
    let Some(Some(def)) = sprites.get(0x14) else { return };
    let mut x = 0xA0 - s.len() as i32 * 4;
    for c in s.bytes() {
        if let Some(cell) = cell(c) {
            q.push(Quad {
                sprite: 0x14,
                cell,
                pos: [x as f32, y as f32],
                size: [(def.size[0] as i32 * 0x800 >> 12) as f32, (def.size[1] as i32 * 0x800 >> 12) as f32],
                width: 320.0,
                color,
                alpha: 0xFF,
            });
        }
        x += 8;
    }
}

/// 0x49B630's characters.
fn cell(c: u8) -> Option<usize> {
    Some(match c {
        b' ' => return None,
        b'!' => 0x29,
        b'\'' => 0x30,
        b'*' => 0x32,
        b',' => 0x25,
        b'-' => 0x31,
        b'.' => 0x24,
        b'>' => 0x2D,
        b'?' => 0x28,
        c if c < 0x3A => (c - 0x16) as usize,
        c => (c.wrapping_add(0x9F)) as usize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(p: &mut Pause, b: u32) -> Vec<Out> {
        p.tick(Pad(0));
        p.tick(Pad(b))
    }

    #[test]
    fn continue_resumes() {
        let mut p = Pause::new(8, 8);
        assert_eq!(press(&mut p, Pad::JUMP), vec![Out::Sound(0x3E), Out::Resume]);
    }

    #[test]
    fn start_resumes_too() {
        let mut p = Pause::new(8, 8);
        assert!(p.tick(Pad(Pad::START)).is_empty(), "the press that opened it");
        assert!(press(&mut p, Pad::START).contains(&Out::Resume));
    }

    #[test]
    fn camera_mode_sets_the_bit_and_returns() {
        let mut p = Pause::new(8, 8);
        press(&mut p, Pad::DOWN);
        press(&mut p, Pad::JUMP);
        assert_eq!(p.menu, 1);
        press(&mut p, Pad::DOWN);
        assert!(press(&mut p, Pad::JUMP).contains(&Out::Camera(true)));
        assert_eq!((p.menu, p.row), (0, 0));
    }

    #[test]
    fn volume_is_heard_and_cancel_restores_it() {
        let mut p = Pause::new(8, 8);
        for _ in 0..2 {
            press(&mut p, Pad::DOWN);
        }
        press(&mut p, Pad::JUMP);
        assert_eq!(p.menu, 2);
        assert!(press(&mut p, Pad::LEFT).contains(&Out::Volumes { music: 8, sfx: 7 }));
        assert!(press(&mut p, Pad::CANCEL).contains(&Out::Volumes { music: 8, sfx: 8 }));
        assert_eq!(p.kept, (8, 8));
    }

    #[test]
    fn exit_level_asks_first() {
        let mut p = Pause::new(8, 8);
        for _ in 0..3 {
            press(&mut p, Pad::DOWN);
        }
        press(&mut p, Pad::JUMP);
        assert_eq!(p.menu, 3);
        assert!(!press(&mut p, Pad::JUMP).contains(&Out::ExitLevel), "no is the first row");
        assert_eq!(p.menu, 0);
        for _ in 0..3 {
            press(&mut p, Pad::DOWN);
        }
        press(&mut p, Pad::JUMP);
        press(&mut p, Pad::DOWN);
        assert!(press(&mut p, Pad::JUMP).contains(&Out::ExitLevel));
    }

    #[test]
    fn cells() {
        assert_eq!(cell(b'a'), Some(0));
        assert_eq!(cell(b'z'), Some(25));
        assert_eq!(cell(b'0'), Some(0x1A));
        assert_eq!(cell(b'*'), Some(0x32));
        assert_eq!(cell(b' '), None);
    }
}
