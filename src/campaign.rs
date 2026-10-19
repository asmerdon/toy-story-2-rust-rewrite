//! Campaign order is distinct from the executable's level/script IDs.
//! InitLevelPlay (0x452FC0) stores IDs 11..15 in level01..05/level1.*.
//! The retail campaign plays internal ID 6 (Bombs Away) third, and ID 3
//! (Slime Time) sixth. Never swap script IDs without also swapping their assets.
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub struct Level {
    pub number: usize,
    pub internal_id: usize,
    pub name: &'static str,
}

pub const LEVELS: [Level; 15] = [
    Level {
        number: 1,
        internal_id: 1,
        name: "Andy's House",
    },
    Level {
        number: 2,
        internal_id: 2,
        name: "Andy's Neighborhood",
    },
    Level {
        number: 3,
        internal_id: 6,
        name: "Bombs Away",
    },
    Level {
        number: 4,
        internal_id: 4,
        name: "Construction Yard",
    },
    Level {
        number: 5,
        internal_id: 5,
        name: "Alleys and Gullies",
    },
    Level {
        number: 6,
        internal_id: 3,
        name: "Slime Time",
    },
    Level {
        number: 7,
        internal_id: 7,
        name: "Al's Toy Barn",
    },
    Level {
        number: 8,
        internal_id: 8,
        name: "Al's Space Land",
    },
    Level {
        number: 9,
        internal_id: 9,
        name: "Toy Barn Encounter",
    },
    Level {
        number: 10,
        internal_id: 10,
        name: "Elevator Hop",
    },
    Level {
        number: 11,
        internal_id: 11,
        name: "Al's Penthouse",
    },
    Level {
        number: 12,
        internal_id: 12,
        name: "The Evil Emperor Zurg",
    },
    Level {
        number: 13,
        internal_id: 13,
        name: "Airport Infiltration",
    },
    Level {
        number: 14,
        internal_id: 14,
        name: "Tarmac Trouble",
    },
    Level {
        number: 15,
        internal_id: 15,
        name: "Prospector Showdown",
    },
];

pub fn level(number: usize) -> Option<&'static Level> {
    number.checked_sub(1).and_then(|i| LEVELS.get(i))
}

impl Level {
    pub fn path(&self, game: &Path) -> PathBuf {
        let (folder, stem) = if self.internal_id > 10 {
            (self.internal_id - 10, "level1")
        } else {
            (self.internal_id, "level")
        };
        game.join("data")
            .join(format!("level{folder:02}"))
            .join(format!("{stem}.ngn"))
    }

    pub fn script_available(&self) -> bool {
        self.internal_id <= 10 || self.internal_id == 12
    }
}
