//! Text: the speech box of cutscenes (0x401A00 lays the text out, 0x401C30 types and draws
//! it) and plain strings (0x401FB0 / 0x49B630), in the HUD's font: sprite 0x14, 16-pixel cells
//! drawn at half size (8 pixels a character in the 320-wide screen).
//!
//! - Layout: words wrapped into lines of 36 characters, each line padded with spaces; `^`
//!   switches highlighting and takes no room.
//! - Typing: one character every 2 ticks into a two-line, 72-cell ring; a new line clears its
//!   row and scrolls. When both rows are new, it waits ("press jump to continue", blinking);
//!   jump goes on (sound 0x3D). Jump while typing fills the rows at once. At the end, jump
//!   closes it (sound 0x3E); Square closes it at any time.
//! - The box (0x401B60): black edges, a half-transparent red inside, growing from the middle
//!   of the screen top as it opens and shrinking as it closes (0x100 a tick of 0x1000).

use crate::hud::Quad;

const COLUMNS: usize = 36;
const CELLS: usize = 72;
const FONT: usize = 0x14;

/// Font cell of a character (0x49B630); None for a space or anything it doesn't draw.
pub fn glyph(c: u8) -> Option<usize> {
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
        b'0'..=b'9' => (c - 0x16) as usize,
        b'a'..=b'z' => (c - b'a') as usize,
        b'A'..=b'Z' => (c - b'A') as usize,
        _ => return None,
    })
}

/// 0x401A00: the text in 36-character lines (padded), `^` kept as a switch.
pub fn layout(text: &str) -> Vec<u8> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        // Up to 36 visible characters.
        let (mut j, mut n) = (i, 0);
        while j < b.len() && n < COLUMNS {
            if b[j] != b'^' {
                n += 1;
            }
            j += 1;
        }
        let mut end = j;
        let mut next = j;
        if j < b.len() {
            if b[j] == b' ' {
                next = j + 1;
            } else {
                // Back to the last space.
                let mut k = j;
                while k > i && b[k - 1] != b' ' {
                    k -= 1;
                }
                if k > i {
                    end = k - 1;
                    while end > i && b[end - 1] == b' ' {
                        end -= 1;
                    }
                    next = k;
                }
            }
        }
        let mut line: Vec<u8> = b[i..end].to_vec();
        // Wrapped lines are padded to the full width; the last one isn't.
        if next < b.len() {
            let visible = line.iter().filter(|&&c| c != b'^').count();
            line.extend(std::iter::repeat_n(b' ', COLUMNS.saturating_sub(visible)));
        }
        out.extend(line);
        i = next;
    }
    out
}

/// What the speech box asks for this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Sound(u16),
    Closed,
}

#[derive(Debug, Clone)]
pub struct Dialog {
    text: Vec<u8>,
    at: usize,
    highlight: bool,
    /// 0x50A1FC: the ring of cells (character, highlighted).
    cells: [(u8, bool); CELLS],
    /// 0x50A114 row start, 0x50A4C4 column, 0x50A4D8 first cell shown (may be negative).
    row: usize,
    col: usize,
    shown: i32,
    /// 0x50A4C0: 0 typing, 1 waiting for jump, 2 all typed.
    state: u8,
    countdown: i32,
    /// 0x50A4EC: 0x1000 open; rising from 0 opening, rising from −0x1000 closing.
    open: i32,
    clock: u32,
    /// A jump press waiting for the next typing step (the original reads the pad once a frame,
    /// and its frames are two ticks, so it never misses one).
    fast: bool,
}

impl Dialog {
    pub fn new(text: &str) -> Dialog {
        Dialog { text: layout(text), at: 0, highlight: false, cells: [(b' ', false); CELLS], row: 0, col: 0, shown: -0x48, state: 0, countdown: 2, open: 0, clock: 0, fast: false }
    }

    pub fn open(&self) -> bool {
        self.open != 0 || self.at == 0
    }

    /// One tick of 0x401C30.
    pub fn tick(&mut self, jump_pressed: bool, square_pressed: bool) -> Vec<Event> {
        let mut ev = Vec::new();
        self.clock = self.clock.wrapping_add(1);
        if self.open == 0x1000 {
            if square_pressed {
                self.open = -0x1000;
                ev.push(Event::Sound(0x3E));
            }
            if self.state == 0 {
                self.fast |= jump_pressed;
                self.countdown -= 1;
                if self.countdown < 1 {
                    let mut fast = std::mem::take(&mut self.fast);
                    self.countdown = 2;
                    loop {
                        if self.col == 0 {
                            for k in 0..COLUMNS {
                                self.cells[(self.row + k) % CELLS] = (b' ', false);
                            }
                            self.shown += COLUMNS as i32;
                            if self.shown > 0x47 {
                                self.shown = 0;
                            }
                        }
                        // The next character, `^` switching highlighting.
                        let mut c = b' ';
                        while self.at < self.text.len() {
                            c = self.text[self.at];
                            self.at += 1;
                            if c == b'^' {
                                self.highlight = !self.highlight;
                                continue;
                            }
                            break;
                        }
                        self.cells[self.row + self.col] = (c, self.highlight);
                        self.col += 1;
                        if self.at >= self.text.len() {
                            self.state = 2;
                            break;
                        }
                        if self.col >= COLUMNS {
                            self.col = 0;
                            self.row += COLUMNS;
                            if self.row >= CELLS {
                                self.row = 0;
                                self.state = 1;
                                break;
                            }
                        }
                        if !fast {
                            break;
                        }
                        fast = true;
                    }
                }
            } else if jump_pressed {
                if self.state == 2 {
                    self.open = -0x1000;
                    ev.push(Event::Sound(0x3E));
                } else {
                    self.state = 0;
                    ev.push(Event::Sound(0x3D));
                }
            }
        } else if self.open < 0 {
            self.open += 0x100;
            if self.open >= 0 {
                self.open = 0;
                self.at = usize::MAX;
                ev.push(Event::Closed);
            }
        } else if self.at != usize::MAX {
            self.open = (self.open + 0x100).min(0x1000);
        }
        ev
    }

    /// What 0x401C30 draws: the box, the two rows and the prompt.
    pub fn quads(&self) -> Vec<Quad> {
        let mut q = Vec::new();
        if self.open == 0 {
            return q;
        }
        let o = self.open.abs();
        let w = (o * 0x1DA).max(0x4000);
        let h = (o << 5).max(0x2000);
        boxed(&mut q, 0x101 - (w >> 13), 0x2C - (h >> 13), w, h, [0x80, 0, 0]);
        if self.open != 0x1000 {
            return q;
        }
        if self.state != 0 && self.clock % 32 < 0x10 {
            text(&mut q, (0x28 - 22) * 4, 0x30, b"press jump to continue", [0xFF; 3]);
        }
        let mut k = self.shown.max(0) as usize;
        let (mut x, mut y) = (0x10, 0x20);
        for _ in 0..CELLS {
            let (c, hi) = self.cells[k % CELLS];
            if let Some(g) = glyph(c) {
                q.push(char_quad(x, y, g, if hi { [0, 0x80, 0] } else { [0x80, 0x80, 0] }));
            }
            k += 1;
            x += 8;
            if x > 0x12F {
                x = 0x10;
                y += 8;
            }
        }
        q
    }
}

fn char_quad(x: i32, y: i32, cell: usize, color: [u8; 3]) -> Quad {
    Quad { sprite: FONT, cell, pos: [x as f32, y as f32], size: [8.0, 8.0], width: 320.0, color }
}

/// A string in the 320-wide screen at (x, y), 8 pixels a character (0x401FB0's drawing).
pub fn text(q: &mut Vec<Quad>, x: i32, y: i32, s: &[u8], color: [u8; 3]) {
    for (k, &c) in s.iter().enumerate() {
        if let Some(g) = glyph(c) {
            q.push(char_quad(x + 8 * k as i32, y, g, color));
        }
    }
}

/// 0x401B60: a box with 2-pixel black edges and a half-transparent inside, in the 512-wide
/// screen; `w` and `h` are 16.16-ish sizes (>> 12 = pixels).
pub fn boxed(q: &mut Vec<Quad>, x: i32, y: i32, w: i32, h: i32, inside: [u8; 3]) {
    let px = |x: i32, y: i32, ww: f32, hh: f32, color: [u8; 3]| Quad { sprite: 6, cell: 1, pos: [x as f32, y as f32], size: [ww, hh], width: 512.0, color };
    let (wp, hp) = ((w >> 12) as f32, (h >> 12) as f32);
    q.push(px(x, y, 2.0, hp, [0; 3]));
    q.push(px(x + (w >> 12) - 2, y, 2.0, hp, [0; 3]));
    q.push(px(x, y, wp, 1.0, [0; 3]));
    q.push(px(x, y + (h >> 12) - 1, wp, 1.0, [0; 3]));
    q.push(Quad { sprite: 6, cell: 1, pos: [(x + 2) as f32, (y + 1) as f32], size: [((w - 0x4000) >> 12) as f32, ((h - 0x2000) >> 12) as f32], width: 512.0, color: inside });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_wraps_on_spaces() {
        let l = layout("hi buzz, please find my ^five^ missing sheep for me!");
        assert_eq!(l.iter().filter(|&&c| c != b'^').count(), 36 + "sheep for me!".len());
        let s = String::from_utf8(l).unwrap();
        assert!(s.starts_with("hi buzz, please find my ^five^"), "{s:?}");
    }

    #[test]
    fn typing_then_closing() {
        let mut d = Dialog::new("well done buzz!");
        for _ in 0..20 {
            d.tick(false, false);
        }
        assert_eq!(d.open, 0x1000);
        // 15 characters, one every 2 ticks.
        for _ in 0..40 {
            d.tick(false, false);
        }
        assert_eq!(d.state, 2);
        assert!(d.tick(true, false).contains(&Event::Sound(0x3E)));
        let mut closed = false;
        for _ in 0..20 {
            closed |= d.tick(false, false).contains(&Event::Closed);
        }
        assert!(closed);
    }
}
