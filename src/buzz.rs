//! Buzz's animation states, driven by the move table read from the user's toy2.exe
//! (0x4DF3F0) the way the original's player update (0x4011D0) picks them.

use crate::formats::exe::{BuzzMove, TICKS_PER_SECOND};
use crate::game::Motion;
use crate::seq::SeqPlayer;

/// State numbers in the move table (`DAT_0052F390` in the original).
pub mod state {
    pub const RUN: usize = 0;
    pub const IDLE: usize = 1;
    pub const JUMP: usize = 2;
    pub const FALL: usize = 3;
    pub const LAND: usize = 4;
}

pub struct BuzzAnimator {
    pub moves: Vec<BuzzMove>,
    pub state: usize,
    seq: SeqPlayer,
}

/// What to draw this frame: (animation index, frame) for the base layer and the overlay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuzzPose {
    pub base: (usize, f32),
    pub overlay: (usize, f32),
}

impl BuzzAnimator {
    pub fn new(moves: Vec<BuzzMove>) -> BuzzAnimator {
        BuzzAnimator { moves, state: state::IDLE, seq: SeqPlayer::default() }
    }

    fn set(&mut self, s: usize) {
        if s != self.state {
            self.state = s;
            self.seq.restart();
        }
    }

    /// `horizontal_speed` in level units per second.
    pub fn update(&mut self, motion: Motion, horizontal_speed: f32, dt: f32) -> BuzzPose {
        // Transitions as in 0x4011D0: rising → falling when moving down; falling → landing on
        // touchdown; landing plays out before standing/running.
        let next = match (motion, self.state) {
            (Motion::Air { rising: true }, _) => state::JUMP,
            (Motion::Air { rising: false }, _) => state::FALL,
            (_, state::FALL | state::JUMP) => state::LAND,
            (Motion::Idle, state::LAND) if !self.seq.finished => state::LAND,
            (Motion::Run, state::LAND) if !self.seq.finished => state::LAND,
            (Motion::Idle, _) => state::IDLE,
            (Motion::Run, _) => state::RUN,
        };
        self.set(next);

        let m = &self.moves[self.state];
        let ticks = dt * TICKS_PER_SECOND;
        let per_tick = if m.rate_raw >= 0 {
            m.rate_raw as f32 / 65536.0
        } else {
            // Speed-scaled: -raw × speed (object units per tick) / 65536.
            let obj_per_tick = horizontal_speed * 32.0 / TICKS_PER_SECOND;
            -(m.rate_raw as f32) * obj_per_tick / 65536.0
        };
        self.seq.advance(&m.frames, m.end, per_tick * ticks);
        let f = self.seq.frame_smooth(&m.frames);
        BuzzPose { base: (m.base_anim.max(0) as usize, f), overlay: (m.anim.max(0) as usize, f) }
    }
}
