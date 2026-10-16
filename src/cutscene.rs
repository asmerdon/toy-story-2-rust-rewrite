//! Talking to a character (0x4027F0 starts it, 0x402A10 plays it): Buzz and the character are
//! put on the first two points of a `level.dat` list, facing each other; the camera glides
//! along the rest of the list (its position from the middle point on, looking at the points
//! from the third on) while the speech box (`dialog`) runs; Buzz can't move. When the box
//! closes the scene ends (sound 0x1E) and, if the character gave one, a token appears
//! (0x4A0DB0).
//!
//! Units as the game: object units, 12-bit angles.

use crate::dialog::{self, Dialog};

#[derive(Debug, Clone)]
pub struct Cutscene {
    /// The list's points, object units.
    points: Vec<[i32; 3]>,
    /// Camera position and look-at segments (0x50A514 / 0x50A4BC), the position along them
    /// (0x50A53C) and its length (0x50A1F0), speed (0x50A144) and the next length (0x50A0CC).
    a: usize,
    b: usize,
    t: i32,
    len: i32,
    speed: i32,
    next_len: i32,
    moving: bool,
    pub dialog: Dialog,
    /// The token the character hands over, if any.
    pub token: Option<usize>,
}

/// What the cutscene asks of the game.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    Sound(u16),
    /// It ended; reveal this token if any.
    Ended(Option<usize>),
}

fn dist(p: [i32; 3], q: [i32; 3]) -> i32 {
    let d = [0, 1, 2].map(|k| ((p[k] - q[k]) >> 5) as f64);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() as i32
}

impl Cutscene {
    /// 0x4027F0 with the default script (0x4DF6CC): `points` from the list (level units).
    /// Returns the scene, where Buzz and the character go and their facings.
    pub fn new(points: &[[i32; 3]], text: &str, token: Option<usize>) -> (Cutscene, [[i32; 3]; 2], [i32; 2]) {
        let pts: Vec<[i32; 3]> = points.iter().map(|p| p.map(|v| v * 32)).collect();
        let at = |k: usize| pts.get(k).copied().unwrap_or_default();
        // Facings from the first two points (param −1 of 0x4027F0).
        let a = crate::boxes::angle12((at(1)[0] - at(0)[0]) as f32, (at(1)[2] - at(0)[2]) as f32);
        let facings = [a & 0xFFF, (a + 0x800) & 0xFFF];
        let n = pts.len();
        let b = 2.min(n.saturating_sub(1));
        let cam = if n > 2 { (n - b) / 2 + b } else { b };
        let len = if cam + 1 < n { dist(at(cam), at(cam + 1)).max(1) } else { 1 };
        let next = if cam + 2 < n { dist(at(cam + 1), at(cam + 2)).max(1) } else { 0 };
        let c = Cutscene { points: pts.clone(), a: cam, b, t: 0, len, speed: 0, next_len: next.max(len), moving: n > 3, dialog: Dialog::new(text), token };
        (c, [at(0), at(1)], facings)
    }

    /// The camera's position and the point it looks at (object units).
    pub fn camera(&self) -> ([i32; 3], [i32; 3]) {
        let at = |k: usize| self.points.get(k).copied().unwrap_or_default();
        let lerp = |p: [i32; 3], q: [i32; 3]| [0, 1, 2].map(|k| p[k] + ((q[k] - p[k]) as i64 * self.t as i64 / self.len.max(1) as i64) as i32);
        let n = self.points.len();
        // 0x402A10 command 10 → 0x402030(target, position): the camera rides the track from
        // point 2 (`b`), looking at the track from (n − 2) / 2 + 2 (`a`).
        let look = if self.a + 1 < n { lerp(at(self.a), at(self.a + 1)) } else { at(self.a) };
        let pos = if self.b + 1 < n { lerp(at(self.b), at(self.b + 1)) } else { at(self.b) };
        (pos, look)
    }

    /// One tick: the camera glides, the box types; with jump / Square presses.
    pub fn tick(&mut self, jump_pressed: bool, square_pressed: bool) -> Vec<Event> {
        let mut ev = Vec::new();
        if self.moving {
            self.speed -= (self.speed - self.next_len) / 16;
            self.speed = self.speed.max(0x80);
            self.t += self.speed >> 5;
            if self.t >= self.len {
                if self.a + 2 >= self.points.len() {
                    self.t = self.len - 1;
                    self.moving = false;
                } else {
                    self.t -= self.len;
                    self.len = self.next_len.max(1);
                    self.a += 1;
                    self.b += 1;
                    let n = self.points.len();
                    self.next_len = if self.a + 2 < n { dist(self.points[self.a + 1], self.points[self.a + 2]).max(1) } else { 0 };
                }
            }
        }
        for e in self.dialog.tick(jump_pressed, square_pressed) {
            match e {
                dialog::Event::Sound(id) => ev.push(Event::Sound(id)),
                dialog::Event::Closed => {
                    ev.push(Event::Sound(0x1E));
                    ev.push(Event::Ended(self.token));
                }
            }
        }
        ev
    }
}
