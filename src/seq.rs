//! Frame-list playback, as the original steps object animations (0x405C80 / 0x401000).
//! A list holds frame numbers (< 0x80) and event markers (≥ 0x80, e.g. footsteps; skipped
//! here until their meaning is known). After the list, the end code decides what happens:
//! 1 = hold the last frame, n > 1 = loop back n entries, 0 = stop.

#[derive(Debug, Clone, Default)]
pub struct SeqPlayer {
    /// Index into the frame list.
    pos: usize,
    /// Fractional progress towards the next entry (1.0 = one entry).
    accum: f32,
    pub finished: bool,
}

impl SeqPlayer {
    pub fn restart(&mut self) {
        *self = SeqPlayer::default();
    }

    /// Current frame number (skipping markers), or 0 for an empty list.
    pub fn frame(&self, frames: &[u8]) -> f32 {
        frames[self.pos.min(frames.len().saturating_sub(1))..]
            .iter()
            .chain(frames.iter().rev())
            .find(|&&f| f < 0x80)
            .map_or(0.0, |&f| f as f32)
    }

    /// Advances by `entries` frame-list entries (may be fractional).
    pub fn advance(&mut self, frames: &[u8], end: u8, entries: f32) {
        if frames.is_empty() || self.finished {
            return;
        }
        self.accum += entries.max(0.0);
        while self.accum >= 1.0 {
            self.accum -= 1.0;
            self.pos += 1;
            if self.pos >= frames.len() {
                match end {
                    0 | 1 => {
                        self.pos = frames.len() - 1;
                        self.finished = true;
                        self.accum = 0.0;
                        return;
                    }
                    n => self.pos = frames.len().saturating_sub(n as usize),
                }
            }
        }
    }

    /// Frame number with sub-frame interpolation towards the next entry, when both are frames.
    pub fn frame_smooth(&self, frames: &[u8]) -> f32 {
        let f = self.frame(frames);
        match frames.get(self.pos + 1) {
            Some(&next) if next < 0x80 && (next as f32 - f).abs() <= 1.0 => f + (next as f32 - f) * self.accum,
            _ => f,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loops_back_n_entries() {
        let frames = [0, 1, 2, 3];
        let mut s = SeqPlayer::default();
        s.advance(&frames, 4, 5.0);
        assert_eq!(s.frame(&frames), 1.0); // 0→1→2→3→(loop to 0)→1
    }

    #[test]
    fn holds_last_frame() {
        let frames = [6, 7, 8];
        let mut s = SeqPlayer::default();
        s.advance(&frames, 1, 10.0);
        assert_eq!(s.frame(&frames), 8.0);
        assert!(s.finished);
    }

    #[test]
    fn skips_event_markers() {
        let frames = [0, 1, 130, 2];
        let mut s = SeqPlayer::default();
        s.advance(&frames, 4, 2.0);
        assert_eq!(s.frame(&frames), 2.0);
    }
}
