//! Special objects at run time: the `.ngn` instances that level scripts and the pickup code
//! move, turn, scale and hide through ids (chunk 0x105, table of 0x74-byte entries built by
//! 0x4C3EB0). The setters and getters follow the exe's 0x4CCxxx family:
//!
//! | exe | here |
//! |---|---|
//! | 0x4CCE30 position (level units) | `set_pos` |
//! | 0x4CCEF0 position (object units, × 32) | `pos_obj` |
//! | 0x4CCC70 angles = own + arg × 16 | `turn_from_own` |
//! | 0x4CCCE0 angles = arg × 16 | `set_angles` |
//! | 0x4CCD40 angles / 16 | `angles` |
//! | 0x4CCB20 scale = arg / 4096 × own; all zero hides | `set_scale` |
//! | 0x4CCFF0(dst, src): dst position scaled about src's position by src's scale | `scale_about` |
//!
//! The matrix (0x4CCBE0) is scale, then Z, Y, X rotations (16-bit angles), then translation,
//! as `scene::instance_matrix` builds it.

use crate::formats::ngn::Ngn;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Special {
    pub layer: usize,
    pub instance: usize,
    /// Level units.
    pub pos: [f32; 3],
    /// 16-bit angles about X, Y, Z.
    pub angles: [u32; 3],
    pub scale: [f32; 3],
    pub hidden: bool,
    own_angles: [u32; 3],
    own_scale: [f32; 3],
    own_pos: [f32; 3],
}

#[derive(Debug, Clone, Default)]
pub struct Specials {
    pub objects: HashMap<u16, Special>,
}

impl Specials {
    pub fn new(ngn: &Ngn) -> Specials {
        let mut objects = HashMap::new();
        for s in &ngn.specials {
            let Some(inst) = ngn.layers.get(s.layer).and_then(|l| l.instances.get(s.instance)) else { continue };
            objects.entry(s.id).or_insert(Special {
                layer: s.layer,
                instance: s.instance,
                pos: inst.pos,
                angles: inst.rot.map(|r| r & 0xFFFF),
                scale: inst.scale,
                hidden: false,
                own_angles: inst.rot.map(|r| r & 0xFFFF),
                own_scale: inst.scale,
                own_pos: inst.pos,
            });
        }
        Specials { objects }
    }

    pub fn get(&self, id: i32) -> Option<&Special> {
        u16::try_from(id).ok().and_then(|id| self.objects.get(&id))
    }

    fn get_mut(&mut self, id: i32) -> Option<&mut Special> {
        u16::try_from(id).ok().and_then(|id| self.objects.get_mut(&id))
    }

    /// 0x4CCE30.
    pub fn set_pos(&mut self, id: i32, pos: [i32; 3]) {
        if let Some(s) = self.get_mut(id) {
            s.pos = pos.map(|v| v as f32);
        }
    }

    /// 0x4CCEF0: position in object units.
    pub fn pos_obj(&self, id: i32) -> Option<[i32; 3]> {
        self.get(id).map(|s| s.pos.map(|v| (v * 32.0) as i32))
    }

    /// 0x4CCF70: where the instance was placed in the level, object units.
    pub fn home_obj(&self, id: i32) -> Option<[i32; 3]> {
        self.get(id).map(|s| s.own_pos.map(|v| (v * 32.0) as i32))
    }

    /// 0x4CCC70: the instance's own angles plus `a` (12-bit) × 16.
    pub fn turn_from_own(&mut self, id: i32, a: [i32; 3]) {
        if let Some(s) = self.get_mut(id) {
            s.angles = std::array::from_fn(|k| s.own_angles[k].wrapping_add((a[k] as u32) << 4) & 0xFFFF);
        }
    }

    /// 0x4CCCE0: absolute angles, 12-bit.
    pub fn set_angles(&mut self, id: i32, a: [i32; 3]) {
        if let Some(s) = self.get_mut(id) {
            s.angles = a.map(|v| ((v as u32) << 4) & 0xFFFF);
        }
    }

    /// 0x4CCD40: angles, 12-bit.
    pub fn angles(&self, id: i32) -> Option<[i32; 3]> {
        self.get(id).map(|s| s.angles.map(|v| (v >> 4) as i32))
    }

    /// 0x4CCB20: scale relative to the instance's own (0x1000 = 1); all zero hides it.
    pub fn set_scale(&mut self, id: i32, s: [i32; 3]) {
        if let Some(o) = self.get_mut(id) {
            if s == [0; 3] {
                o.hidden = true;
                return;
            }
            o.hidden = false;
            o.scale = std::array::from_fn(|k| s[k] as f32 * o.own_scale[k] / 4096.0);
        }
    }

    /// 0x4CCFF0(dst, src): moves `dst` towards `src`'s position by `src`'s scale, per axis.
    pub fn scale_about(&mut self, dst: i32, src: i32) {
        let Some(s) = self.get(src).cloned() else { return };
        if let Some(d) = self.get_mut(dst) {
            for k in 0..3 {
                d.pos[k] = (d.pos[k] - s.pos[k]) * s.scale[k] + s.pos[k];
            }
        }
    }

    /// Instances these ids draw, so the static level can leave them out.
    pub fn instances(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.objects.values().map(|s| (s.layer, s.instance))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one() -> Specials {
        let s = Special { layer: 0, instance: 0, pos: [100.0, 0.0, 0.0], angles: [0x4000, 0, 0], scale: [2.0; 3], hidden: false, own_angles: [0x4000, 0, 0], own_scale: [2.0; 3], own_pos: [100.0, 0.0, 0.0] };
        Specials { objects: [(4u16, s)].into_iter().collect() }
    }

    #[test]
    fn setters_follow_the_exe() {
        let mut s = one();
        s.turn_from_own(4, [0x200, 0, 0]);
        assert_eq!(s.get(4).unwrap().angles, [0x6000, 0, 0]);
        assert_eq!(s.angles(4), Some([0x600, 0, 0]));
        s.set_scale(4, [0x800, 0x1000, 0x1000]);
        assert_eq!(s.get(4).unwrap().scale, [1.0, 2.0, 2.0]);
        s.set_scale(4, [0; 3]);
        assert!(s.get(4).unwrap().hidden);
        s.set_pos(4, [1, 2, 3]);
        assert_eq!(s.pos_obj(4), Some([32, 64, 96]));
    }
}
