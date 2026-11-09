//! The keyboard controls (0x49C420's "control configuration", the table 0x500E58).
//!
//! The rest of the engine reads fixed keys (the port's defaults). Each row binds one physical
//! key to one of them, so the input layer only has to translate a key as it arrives. As in
//! the original, binding a key takes it from any row that had it and frees the row's old key
//! (0x4151E0, 0x415210, 0x415180).

/// The rows, in the original's order (0x500E58: name, pad bit).
pub const ACTIONS: [(&str, u32); 13] = [
    ("up", 0x10),
    ("down", 0x40),
    ("left", 0x80),
    ("right", 0x20),
    ("jump", 0x4000),
    ("fire", 0x8000),
    ("spin", 0x2000),
    ("camera left", 0x100),
    ("camera right", 0x200),
    ("visor toggle", 0x400),
    ("target lock", 0x800),
    ("menu", 8),
    ("cancel", 0x1000),
];

/// The physical key each row is on; `K` is the window system's key code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binds<K> {
    defaults: Vec<K>,
    pub keys: Vec<Option<K>>,
}

impl<K: Copy + PartialEq> Binds<K> {
    /// `defaults`: the fixed key behind each row of [`ACTIONS`].
    pub fn new(defaults: Vec<K>) -> Binds<K> {
        let keys = defaults.iter().copied().map(Some).collect();
        Binds { defaults, keys }
    }

    /// What the engine should see for physical key `k`: the fixed key of the row it's bound
    /// to; nothing if it's a fixed key whose row has moved elsewhere; otherwise itself.
    pub fn translate(&self, k: K) -> Option<K> {
        if let Some(i) = self.keys.iter().position(|b| *b == Some(k)) {
            return Some(self.defaults[i]);
        }
        if self.defaults.contains(&k) {
            return None;
        }
        Some(k)
    }

    /// Puts `row` on `k`, taking `k` from whichever row had it.
    pub fn bind(&mut self, row: usize, k: K) {
        for b in self.keys.iter_mut().filter(|b| **b == Some(k)) {
            *b = None;
        }
        if let Some(b) = self.keys.get_mut(row) {
            *b = Some(k);
        }
    }

    /// Back to the port's defaults.
    pub fn reset(&mut self) {
        self.keys = self.defaults.iter().copied().map(Some).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binds() -> Binds<u32> {
        Binds::new((100..113).collect())
    }

    #[test]
    fn unbound_keys_pass_through_and_defaults_are_themselves() {
        let b = binds();
        assert_eq!(b.translate(100), Some(100));
        assert_eq!(b.translate(7), Some(7));
    }

    #[test]
    fn a_moved_row_reads_its_new_key_and_frees_the_old() {
        let mut b = binds();
        b.bind(4, 7);
        assert_eq!(b.translate(7), Some(104));
        assert_eq!(b.translate(104), None, "jump's old key does nothing now");
    }

    #[test]
    fn binding_takes_the_key_from_another_row() {
        let mut b = binds();
        b.bind(4, 100);
        assert_eq!(b.keys[0], None, "up lost its key");
        assert_eq!(b.translate(100), Some(104));
        b.reset();
        assert_eq!(b.translate(100), Some(100));
    }

    #[test]
    fn the_table_has_the_originals_rows() {
        assert_eq!(ACTIONS.len(), 13);
        assert_eq!(ACTIONS[7], ("camera left", 0x100));
    }
}
