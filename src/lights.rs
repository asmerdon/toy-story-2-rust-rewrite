//! The light on Buzz: slots at 0x830D60 (0x18 bytes each), picked each frame by 0x49EEE0.
//!
//! - Slot 0 sits at a fixed offset from Buzz (the level's table 0x50387C) with the level's colour.
//! - Slot 1 is the nearest lamp, when the level script has one (0x830D84 set).
//! - Slots 2–5 are short flashes from 0x49EE50 (explosions, deaths, sparks), each with a timer.
//! - The nearest lit slot wins; a change of slot eases over 0x40 ticks from where the old one was
//!   to the new (slot 6 keeps the old values), unless the new one is a flash, which takes over
//!   at once. The result is a direction (0x52F324) and a colour fading with distance (0x52F334).
//!
//! 0x4CE6C0 hands them to the fourth of the hardware lights (0x4CE910 sets up three fixed white
//! ones, a third of a turn apart). Models only use those lights on parts flagged 4 (0x4CAA60);
//! the rest keep their baked colours. See `scene::ModelLight`.

use crate::formats::exe::Exe;

/// 0x50387C: 0x14 bytes a level: offset x, y, z (× 0x10), a word, colour 0xRRGGBB (at +0x10).
const TABLE: u32 = 0x50387C;

/// A slot: object units, colour (0xFF = bright), ticks left, whose it is (0x830D70).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Light {
    pub pos: [i32; 3],
    pub rgb: [i32; 3],
    pub timer: i32,
    pub id: i64,
}

/// A flash for 0x49EE50: position, colour 0xRRGGBB, ticks, owner.
pub type Flash = ([i32; 3], u32, i32, i64);

#[derive(Debug, Clone, Default)]
pub struct Lights {
    /// Slot 0's offset from Buzz, level units × 0x10 scale as stored.
    offset: [i32; 3],
    pub slots: [Light; 7],
    /// 0x830D3C, 0x830D54.
    current: usize,
    blend: i32,
    /// 0x52F324: towards the light from Buzz, in 256-object-unit steps.
    pub direction: [i32; 3],
    /// 0x52F334: its colour at Buzz (0x80 = 1.0 when lit).
    pub colour: [i32; 3],
}

fn split(rgb: u32) -> [i32; 3] {
    [(rgb >> 16 & 0xFF) as i32, (rgb >> 8 & 0xFF) as i32, (rgb & 0xFF) as i32]
}

impl Lights {
    /// 0x49F350 for internal level `level`.
    pub fn new(exe: Option<&Exe>, level: usize) -> Lights {
        let word = |k: u32| exe.and_then(|e| e.at(TABLE + level as u32 * 0x14 + 4 * k, 4).ok()).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let mut l = Lights { offset: [word(0), word(1), word(2)], ..Default::default() };
        l.slots[0].rgb = split(word(4) as u32);
        l
    }

    /// Level 3 points slot 0 at a place and changes its colour (0x41AA10).
    pub fn set_base(&mut self, offset: [i32; 3], rgb: u32) {
        self.offset = offset;
        self.slots[0].rgb = split(rgb);
    }

    /// Slot 1, the script's lamp (0x830D78–0x830D8E; `None` clears 0x830D84).
    pub fn set_lamp(&mut self, lamp: Option<([i32; 3], [i32; 3], i64)>) {
        match lamp {
            Some((pos, rgb, id)) => self.slots[1] = Light { pos, rgb, timer: 1, id },
            None => self.slots[1].timer = 0,
        }
    }

    /// 0x49EE50: a flash takes the slot with the least time left.
    pub fn add(&mut self, f: Flash) {
        let (pos, rgb, timer, id) = f;
        let k = (2..6).min_by_key(|&k| self.slots[k].timer).unwrap_or(2);
        self.slots[k] = Light { pos, rgb: split(rgb), timer, id };
    }

    fn distance(a: [i32; 3], b: [i32; 3]) -> i32 {
        let d = [0, 1, 2].map(|k| (a[k] - b[k]) >> 8);
        d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
    }

    /// 0x49EEE0, one tick; `buzz` in object units.
    pub fn tick(&mut self, buzz: [i32; 3]) {
        let centre = [buzz[0], buzz[1] - 0x2000, buzz[2]];
        self.slots[0].pos = [self.offset[0] * 0x10 + buzz[0], self.offset[1] * 0x10 + centre[1], self.offset[2] * 0x10 + buzz[2]];
        let mut best = if self.slots[1].timer == 0 { i32::MAX } else { Self::distance(self.slots[1].pos, centre) };
        let mut pick = usize::from(self.slots[1].timer != 0);
        for k in 2..6 {
            if self.slots[k].timer > 0 {
                self.slots[k].timer -= 1;
                if self.slots[k].timer < 1 {
                    self.slots[k].timer = 0;
                } else {
                    let d = Self::distance(self.slots[k].pos, centre);
                    if d < 0x8001 {
                        if d < best {
                            pick = k;
                            best = d;
                        }
                    } else {
                        self.slots[k].timer = 0;
                    }
                }
            }
        }
        // Keep what the blend has reached if the slot changes.
        let lerp = |to: i32, from: i32, b: i32| ((to - from) * b) / 64 + from;
        let mut direct = false;
        if self.slots[self.current].id == self.slots[pick].id {
            direct = self.blend == 0;
        } else {
            let (old, cur) = (self.slots[6], self.slots[self.current]);
            let b = self.blend;
            self.slots[6].pos = [0, 1, 2].map(|k| lerp(old.pos[k], cur.pos[k], b));
            self.slots[6].rgb = [0, 1, 2].map(|k| lerp(old.rgb[k], cur.rgb[k], b));
            self.current = pick;
            if pick > 1 {
                self.blend = 0;
                direct = true;
            } else {
                self.blend = 0x40;
            }
        }
        let cur = self.slots[self.current];
        let (pos, rgb) = if direct {
            (cur.pos, cur.rgb)
        } else {
            let (old, b) = (self.slots[6], self.blend);
            self.blend = (self.blend - 1).max(0);
            ([0, 1, 2].map(|k| lerp(old.pos[k], cur.pos[k], b)), [0, 1, 2].map(|k| lerp(old.rgb[k], cur.rgb[k], b)))
        };
        let rel = [pos[0] - buzz[0], pos[1] - centre[1], pos[2] - buzz[2]].map(|v| v >> 8);
        let d = (rel[0] * rel[0] + rel[1] * rel[1] + rel[2] * rel[2]).min(0x10000);
        let fade = 0x10000 - d;
        self.direction = rel;
        self.colour = rgb.map(|c| (fade as i64 * c as i64 >> 16) as i32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lights() -> Lights {
        let mut l = Lights { offset: [0x400, -0x400, 0x400], ..Default::default() };
        l.slots[0].rgb = [0x90, 0x80, 0x60];
        l
    }

    /// Slot 0 hangs off Buzz, so he's always lit in the level's colour.
    #[test]
    fn the_level_light_follows_buzz() {
        let mut l = lights();
        l.tick([50000, 1000, -3000]);
        assert_eq!(l.direction, [0x40, -0x40, 0x40]);
        let fade = 0x10000 - 3 * 0x1000;
        assert_eq!(l.colour, [0x90, 0x80, 0x60].map(|c| fade * c >> 16));
    }

    /// A flash takes over at once and goes when its timer does, easing back.
    #[test]
    fn a_flash_takes_over_then_eases_back() {
        let mut l = lights();
        l.tick([0; 3]);
        l.add(([0, -0x2000, 0x2000], 0xF08000, 3, 9));
        l.tick([0; 3]);
        assert_eq!(l.direction, [0, 0, 0x20]);
        assert_eq!(l.colour[0], (0x10000 - 0x400) * 0xF0 >> 16);
        for _ in 0..2 {
            l.tick([0; 3]);
        }
        let during = l.direction;
        assert_eq!(during[2], 0x20, "still at the flash as the blend starts");
        for _ in 0..0x41 {
            l.tick([0; 3]);
        }
        assert_eq!(l.direction, [0x40, -0x40, 0x40], "back on the level's");
    }

    /// Too far away and it goes out.
    #[test]
    fn distant_flashes_die() {
        let mut l = lights();
        l.tick([0; 3]);
        l.add(([0x100000, 0, 0], 0xFF, 9, 1));
        l.tick([0; 3]);
        assert_eq!(l.slots.iter().skip(2).take(4).filter(|s| s.timer > 0).count(), 0);
    }

    /// The nearest lamp wins over the level's light.
    #[test]
    fn a_lamp_is_used_while_set() {
        let mut l = lights();
        l.tick([0; 3]);
        l.set_lamp(Some(([0x1000, -0x2000, 0], [0xFF; 3], 7)));
        for _ in 0..0x42 {
            l.tick([0; 3]);
        }
        assert_eq!(l.direction, [0x10, 0, 0]);
        l.set_lamp(None);
        for _ in 0..0x42 {
            l.tick([0; 3]);
        }
        assert_eq!(l.direction, [0x40, -0x40, 0x40]);
    }
}
