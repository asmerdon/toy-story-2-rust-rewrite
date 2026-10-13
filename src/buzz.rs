//! Buzz's animation, driven by the move table read from the user's toy2.exe (0x4DF3F0) the
//! way the original's player update (0x4011D0) and frame-list stepper (0x401000) play it.
//! The gameplay code (`game.rs`) picks the state; this steps that state's frame list.

use crate::formats::exe::BuzzMove;

/// State numbers in the move table (`0x52F390` in the original).
pub mod state {
    pub const RUN: u16 = 0;
    pub const IDLE: u16 = 1;
    pub const JUMP: u16 = 2;
    pub const FALL: u16 = 3;
    pub const LAND: u16 = 4;
    /// Teetering on a ledge, facing the drop / with the drop behind (0x4011D0's probes).
    pub const TEETER_FRONT: u16 = 10;
    pub const TEETER_BACK: u16 = 11;
    /// Written directly by the airborne routine 0x4340D0. Overlay anim 22 spreads the wings
    /// (nodes 11/12 and 17/18), base anim 27 tucks the legs. Held until landing.
    pub const DOUBLE_JUMP: u16 = 8;
    /// After a second of falling (fall timer 0x50).
    pub const FALL_PANIC: u16 = 0xC;
    /// Landing from a long fall (70 ticks).
    pub const FALL_RECOVER: u16 = 0xD;
    /// On a pole: climbing up / hanging / sliding down (0x4011D0 from the climb flags).
    pub const CLIMB_UP: u16 = 0xE;
    pub const CLIMB_HANG: u16 = 0xF;
    pub const CLIMB_SLIDE: u16 = 0x10;
    /// Sharp reversal on the ground (26 ticks).
    pub const SKID: u16 = 0x12;
    /// Stomp: hang and slam.
    pub const STOMP: u16 = 0x17;
    /// Stomp impact.
    pub const STOMP_LAND: u16 = 0x18;
    /// Idle fidgets, picked at random while standing.
    pub const FIDGET: u16 = 0x1C;
}

#[derive(Debug, Clone)]
pub struct BuzzAnimator {
    pub moves: Vec<BuzzMove>,
    /// State whose list is playing (`0x52F392`); None forces a restart.
    current: Option<u16>,
    /// List position, 16.16 (`0x52F380`): entry index and progress to the next.
    pos: u32,
    /// Sound ids from the list's event markers since the last drain.
    pub sounds: Vec<u16>,
}

/// Sound for a frame-list event marker (0x401000): 0x83–0x86. 0x81/0x82 are footsteps,
/// which the original turns into surface-dependent sounds elsewhere (not ported yet).
fn marker_sound(code: u8) -> Option<u16> {
    match code {
        0x83 => Some(0x30),
        0x84 => Some(0x10),
        0x85 => Some(0x17),
        0x86 => Some(0x43),
        _ => None,
    }
}

/// What to draw: (animation index, time in frames) for the base layer and the overlay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuzzPose {
    pub base: (usize, f32),
    pub overlay: (usize, f32),
}

impl BuzzAnimator {
    pub fn new(moves: Vec<BuzzMove>) -> BuzzAnimator {
        BuzzAnimator { moves, current: None, pos: 0, sounds: Vec::new() }
    }

    fn list(&self) -> &[u8] {
        self.current.and_then(|s| self.moves.get(s as usize)).map_or(&[], |m| &m.list)
    }

    /// Settles the position on a frame entry, handling codes (0x401000). Returns true when the
    /// list ended with 0xFE, which puts Buzz back in state 0.
    fn settle(&mut self, state: &mut u16) -> bool {
        let at = |s: &Self, i: u32| s.list().get(i as usize).copied().unwrap_or(0xFE);
        let i = self.pos >> 16;
        // A frame followed by "0xFF 1" holds still, so its fraction is cleared.
        if at(self, i + 1) == 0xFF && at(self, i + 2) == 1 {
            self.pos &= 0xFFFF_0000;
        }
        for _ in 0..256 {
            let i = self.pos >> 16;
            match at(self, i) {
                b if b < 0x80 => return false,
                0xFE => {
                    *state = 0;
                    self.current = None;
                    return true;
                }
                // Back n entries (n = 1 holds the last frame).
                0xFF => self.pos = self.pos.wrapping_sub((at(self, i + 1) as u32) << 16),
                code => {
                    self.sounds.extend(marker_sound(code));
                    self.pos += 0x10000;
                }
            }
        }
        false
    }

    /// One game tick of `state`'s list. `speed` = sqrt(forward² + side²) in object units per
    /// tick, for the speed-scaled rows. Returns true if the state changed and must be re-picked.
    pub fn tick(&mut self, state: &mut u16, speed: i32) -> bool {
        if self.current != Some(*state) {
            self.current = Some(*state);
            self.pos = 0;
            return false;
        }
        let Some(m) = self.moves.get(*state as usize) else { return false };
        let mut inc = if m.rate_raw >= 0 { m.rate_raw } else { (-m.rate_raw).saturating_mul(speed) }.max(0);
        while inc >= 0x10000 {
            inc -= 0x10000;
            self.pos += 0x10000;
            if self.settle(state) {
                return true;
            }
        }
        self.pos += inc as u32;
        self.settle(state)
    }

    /// The state whose list is playing and the current entry in it.
    pub fn entry(&self) -> Option<(u16, u32)> {
        Some((self.current?, self.pos >> 16))
    }

    /// The pose for the current list position.
    pub fn pose(&self) -> Option<BuzzPose> {
        let m = self.moves.get(self.current? as usize)?;
        let frame = *m.list.get((self.pos >> 16) as usize)?;
        let t = frame as f32 + (self.pos & 0xFFFF) as f32 / 65536.0;
        Some(BuzzPose { base: (m.base_anim.max(0) as usize, t), overlay: (m.anim.max(0) as usize, t) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(list: &[u8], rate: i32) -> BuzzMove {
        BuzzMove { frames: vec![], end: 0, list: list.to_vec(), anim: 1, base_anim: 2, rate_raw: rate }
    }

    #[test]
    fn holds_loops_and_ends() {
        let mut a = BuzzAnimator::new(vec![mv(&[0, 1, 0xFE], 0x10000), mv(&[5, 0x83, 6, 0xFF, 1], 0x10000), mv(&[0, 1, 2, 0xFF, 3], 0x10000)]);
        let mut s = 1;
        a.tick(&mut s, 0);
        let frames: Vec<f32> = (0..4).map(|_| {
            a.tick(&mut s, 0);
            a.pose().unwrap().base.1
        }).collect();
        assert_eq!(frames, [6.0, 6.0, 6.0, 6.0]);
        s = 2;
        a.tick(&mut s, 0);
        let frames: Vec<f32> = (0..4).map(|_| {
            a.tick(&mut s, 0);
            a.pose().unwrap().base.1
        }).collect();
        assert_eq!(frames, [1.0, 2.0, 0.0, 1.0]);
        s = 0;
        a.tick(&mut s, 0);
        a.tick(&mut s, 0);
        assert!(a.tick(&mut s, 0));
        assert_eq!(s, 0);
    }

    #[test]
    fn half_rate_interpolates() {
        let mut a = BuzzAnimator::new(vec![mv(&[0, 1, 2, 0xFF, 1], 0x8000)]);
        let mut s = 0;
        a.tick(&mut s, 0);
        a.tick(&mut s, 0);
        assert_eq!(a.pose().unwrap().overlay.1, 0.5);
    }
}
