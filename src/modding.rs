//! Mods for the viewer: the `mod.toml` manifest (docs/opents2-design.md section 3) and the
//! `[play_as]` table (docs/play-as-mods.md), plus the pure maths of "play as another creature":
//! matching the creature's parts to Buzz's and retargeting Buzz's move-table poses onto them.
//!
//! Deviation (mod): everything here only runs when a mod asks for it. With no mods the
//! viewer never calls into this module.
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The manifest file name inside a mod folder.
pub const MANIFEST: &str = "mod.toml";

/// A parsed `mod.toml`. Unknown keys are an error (the launcher's own reader is more
/// tolerant: it only needs the common fields).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub engine: String,
    /// Draw the player as another creature.
    #[serde(default)]
    pub play_as: Option<PlayAs>,
}

/// How the substitute creature is animated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnimationMode {
    /// Buzz's move-table animations mapped onto the creature's parts ([`Retarget`]).
    #[default]
    Retarget,
    /// The bounce gait with the creature's own animations (`idle`, `hop`). Parsed and
    /// validated here; the gait itself belongs to a later piece (the extension point is the
    /// viewer's `build_play_as` and the `AnimationMode::Hop` arm of its play-as drawing).
    Hop,
}

/// The `[play_as]` table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayAs {
    /// Name in creatures.cfg (matched case-insensitively against the model file).
    pub creature: String,
    /// Where to load the model, relative to the game folder; default: the current level, then
    /// every level's `.ngn`, first match wins.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub animation: AnimationMode,
    /// Drawn size relative to the model; collision stays Buzz's.
    #[serde(default = "one")]
    pub scale: f32,
    /// False: no firing, no charge, no first-person helmet and arms.
    #[serde(default = "yes")]
    pub laser: bool,
    /// `hop` mode: the creature's idle animation number.
    #[serde(default)]
    pub idle: Option<usize>,
    /// `hop` mode: the creature's hop animation number.
    #[serde(default)]
    pub hop: Option<usize>,
    /// `hop` mode: the walking hop's height as a multiple of the default low hop (about a
    /// third of Buzz's jump). 4.0 is a little higher than Buzz's full jump.
    #[serde(default = "one")]
    pub hop_height: f32,
    /// Touching an enemy hits it as Buzz's spin does, and harmful ones no longer hurt the
    /// player; the reach grows with the drawn size.
    #[serde(default)]
    pub touch_damage: bool,
    /// The follow camera's distance as a multiple of Buzz's (for a bigger character).
    #[serde(default = "one")]
    pub camera_distance: f32,
    /// `retarget` mode: overrides to the automatic node map, `[creature_node, buzz_node]`.
    #[serde(default)]
    pub map: Vec<[usize; 2]>,
    /// `retarget` mode: parts turned out of the creature's own rest pose first, `[[play_as.rest]]`.
    #[serde(default)]
    pub rest: Vec<RestPose>,
    /// `retarget` mode: parts posed from the part they hang off (`[[play_as.joints]]`) so a
    /// joint never comes apart when the creature's limbs are longer than Buzz's.
    #[serde(default)]
    pub joints: Vec<Joint>,
}

/// A joint of the creature: `node` hangs off `parent` at `at` (model units, in the creature's
/// own rest pose, before `rest` corrections).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Joint {
    pub node: usize,
    pub parent: usize,
    pub at: [f32; 3],
}

/// Turns some nodes of the creature about a pivot before Buzz's motion applies (Woody's arms
/// are held out in his rest pose; Buzz's hang down).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestPose {
    /// Creature nodes turned together.
    pub nodes: Vec<usize>,
    /// Pivot, model units (the joint: a shoulder).
    pub pivot: [f32; 3],
    /// Degrees about X, Y, Z (applied in that order, as the animations do).
    pub angles: [f32; 3],
}

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

/// Parses manifest text.
pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    toml::from_str(text).map_err(|e| e.to_string())
}

/// A mod folder with its parsed manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct Mod {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

/// Reads `dir/mod.toml`; every error names the folder.
pub fn load_mod(dir: &Path) -> Result<Mod, String> {
    let file = dir.join(MANIFEST);
    let text = std::fs::read_to_string(&file).map_err(|e| format!("mod {}: {e}", file.display()))?;
    let manifest = parse_manifest(&text).map_err(|e| format!("mod {}: {e}", file.display()))?;
    Ok(Mod { dir: dir.to_path_buf(), manifest })
}

/// The folders to load, in load order: `OPENTS2_MODS` (a path list, as the launcher sets it),
/// then the `--mod` arguments.
pub fn mod_dirs(env: Option<&std::ffi::OsStr>, args: &[PathBuf]) -> Vec<PathBuf> {
    env.map(|v| std::env::split_paths(v).filter(|p| !p.as_os_str().is_empty()).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain(args.iter().cloned())
        .collect()
}

/// The `[play_as]` in force: the last mod in load order that sets one.
pub fn active_play_as(mods: &[Mod]) -> Option<(&Mod, &PlayAs)> {
    mods.iter().rev().find_map(|m| m.manifest.play_as.as_ref().map(|p| (m, p)))
}

/// Axis-aligned bounds of a part's rest-pose mesh (model units, game axes: +Y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub lo: [f32; 3],
    pub hi: [f32; 3],
}

impl Bounds {
    pub fn of(points: impl IntoIterator<Item = [f32; 3]>) -> Option<Bounds> {
        let mut it = points.into_iter();
        let first = it.next()?;
        Some(it.fold(Bounds { lo: first, hi: first }, |b, p| Bounds {
            lo: std::array::from_fn(|k| b.lo[k].min(p[k])),
            hi: std::array::from_fn(|k| b.hi[k].max(p[k])),
        }))
    }

    pub fn centre(&self) -> [f32; 3] {
        std::array::from_fn(|k| (self.lo[k] + self.hi[k]) / 2.0)
    }

    pub fn size(&self) -> [f32; 3] {
        std::array::from_fn(|k| self.hi[k] - self.lo[k])
    }

    fn union(self, o: Bounds) -> Bounds {
        Bounds { lo: std::array::from_fn(|k| self.lo[k].min(o.lo[k])), hi: std::array::from_fn(|k| self.hi[k].max(o.hi[k])) }
    }
}

/// Height (the Y extent) of all the parts together.
pub fn height(parts: &[Option<Bounds>]) -> f32 {
    parts.iter().flatten().copied().reduce(Bounds::union).map_or(0.0, |b| b.size()[1])
}

/// The lowest point (largest Y) of all the parts: the feet.
fn feet(parts: &[Option<Bounds>]) -> f32 {
    parts.iter().flatten().map(|b| b.hi[1]).fold(f32::MIN, f32::max)
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>().sqrt()
}

/// Which of Buzz's nodes each node of the other creature follows, by matching rest-pose
/// bounding boxes once both models are brought to the same height and stand on the same floor.
/// A part picks the Buzz part whose centre is nearest and whose size is most alike (a side
/// mismatch costs extra: left stays left). Parts without a mesh follow the nearest mapped part
/// by node number; the result has one entry per `other` node.
pub fn auto_node_map(buzz: &[Option<Bounds>], other: &[Option<Bounds>]) -> Vec<usize> {
    let s = height(other) / height(buzz).max(f32::EPSILON);
    let (fb, fo) = (feet(buzz), feet(other));
    // Everything in Buzz-scale coordinates with the feet at zero.
    let centre_b = |b: &Bounds| {
        let c = b.centre();
        [c[0], c[1] - fb, c[2]]
    };
    let centre_o = |b: &Bounds| {
        let c = b.centre();
        [c[0] / s, (c[1] - fo) / s, c[2] / s]
    };
    let size_o = |b: &Bounds| b.size().map(|v| v / s);
    let mut map: Vec<Option<usize>> = other
        .iter()
        .map(|o| {
            let o = o.as_ref()?;
            let (co, so) = (centre_o(o), size_o(o));
            buzz.iter()
                .enumerate()
                .filter_map(|(i, b)| b.as_ref().map(|b| (i, b)))
                .map(|(i, b)| {
                    let cb = centre_b(b);
                    let side = if co[0] * cb[0] < 0.0 && co[0].abs() > 20.0 && cb[0].abs() > 20.0 { 1000.0 } else { 0.0 };
                    (i, dist(co, cb) + 0.5 * dist(so, b.size()) + side)
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        })
        .collect();
    let mapped: Vec<usize> = (0..map.len()).filter(|&i| map[i].is_some()).collect();
    for i in 0..map.len() {
        if map[i].is_none() {
            map[i] = mapped.iter().min_by_key(|&&m| m.abs_diff(i)).and_then(|&m| map[m]);
        }
    }
    map.into_iter().map(|m| m.unwrap_or(0)).collect()
}

/// 4x4 row-vector node matrix as the animations produce them (`v' = v * M`, translation in row 3).
pub type Matrix = [[f32; 4]; 4];

#[cfg(test)]
const IDENTITY: Matrix = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

/// `a` then `b` (row vectors: `v * a * b`).
fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..4).map(|k| a[i][k] * b[k][j]).sum()))
}

/// Rotation by `[x, y, z]` degrees about the model axes in the order the animations use (X,
/// then Y, then Z; 4x4 row-vector form).
fn rotation_deg([ax, ay, az]: [f32; 3]) -> Matrix {
    let (sx, cx) = ax.to_radians().sin_cos();
    let (sy, cy) = ay.to_radians().sin_cos();
    let (sz, cz) = az.to_radians().sin_cos();
    let rx = [[1.0, 0.0, 0.0, 0.0], [0.0, cx, sx, 0.0], [0.0, -sx, cx, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let ry = [[cy, 0.0, -sy, 0.0], [0.0, 1.0, 0.0, 0.0], [sy, 0.0, cy, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let rz = [[cz, sz, 0.0, 0.0], [-sz, cz, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
    mul(&mul(&rx, &ry), &rz)
}

fn apply(m: &Matrix, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|j| p[0] * m[0][j] + p[1] * m[1][j] + p[2] * m[2][j] + m[3][j])
}

/// Retargets Buzz's per-node matrices onto another creature's nodes (docs/play-as-mods.md).
#[derive(Debug, Clone)]
pub struct Retarget {
    /// For each of the other creature's nodes, the Buzz node it follows.
    pub map: Vec<usize>,
    /// Rest-pose part centres: Buzz's, and the other creature's after its rest correction
    /// (zero for a part without mesh).
    centre_buzz: Vec<[f32; 3]>,
    centre_other: Vec<[f32; 3]>,
    /// Per node of the other creature: its rest-pose correction (`None` = none).
    rest: Vec<Option<Matrix>>,
    /// Height of the other creature over Buzz's.
    pub s: f32,
    /// Joints in parent-before-child order: (node, parent, joint point after the rest correction).
    joints: Vec<(usize, usize, [f32; 3])>,
}

impl Retarget {
    /// Builds the map automatically, then applies `overrides` (`[other_node, buzz_node]`;
    /// out-of-range entries are ignored). `rest` turns parts from the creature's own rest
    /// pose (Woody stands with his arms out) towards Buzz's before Buzz's motion applies.
    pub fn new(buzz: &[Option<Bounds>], other: &[Option<Bounds>], overrides: &[[usize; 2]], rest: &[RestPose], joints: &[Joint]) -> Retarget {
        let mut map = auto_node_map(buzz, other);
        for &[o, b] in overrides {
            if o < map.len() && b < buzz.len() {
                map[o] = b;
            }
        }
        let c = |p: &Option<Bounds>| p.as_ref().map_or([0.0; 3], Bounds::centre);
        let mut rest_m: Vec<Option<Matrix>> = vec![None; other.len()];
        let mut centre_other: Vec<[f32; 3]> = other.iter().map(c).collect();
        for r in rest {
            // v' = (v - pivot) * R + pivot
            let mut q = rotation_deg(r.angles);
            let moved = apply(&q, r.pivot);
            (0..3).for_each(|j| q[3][j] = r.pivot[j] - (moved[j] - q[3][j]));
            for &n in r.nodes.iter().filter(|&&n| n < other.len()) {
                rest_m[n] = Some(rest_m[n].map_or(q, |old| mul(&old, &q)));
                centre_other[n] = apply(&q, centre_other[n]);
            }
        }
        // Parents before children; a joint whose parent never resolves (a loop, or out of
        // range) is dropped and that part keeps the centre-anchored pose.
        let n = other.len();
        let mut done = vec![true; n];
        joints.iter().filter(|j| j.node < n).for_each(|j| done[j.node] = false);
        let mut ordered = Vec::new();
        loop {
            let before = ordered.len();
            for j in joints {
                if j.node >= n || j.parent >= n || done[j.node] || !done[j.parent] {
                    continue;
                }
                let at = rest_m[j.node].map_or(j.at, |q| apply(&q, j.at));
                ordered.push((j.node, j.parent, at));
                done[j.node] = true;
            }
            if ordered.len() == before {
                break;
            }
        }
        Retarget {
            map,
            centre_buzz: buzz.iter().map(c).collect(),
            centre_other,
            rest: rest_m,
            s: height(other) / height(buzz).max(f32::EPSILON),
            joints: ordered,
        }
    }

    /// The other creature's node matrices for Buzz's pose `buzz` (`None` = at rest). The part
    /// turns as Buzz's does about its own centre; its centre moves by Buzz's displacement,
    /// scaled by `s`: `t = c - c*R + s*(c_b*R + t_b - c_b)`, after the rest correction.
    ///
    /// A part with a joint instead turns as Buzz's does and is placed so its joint point stays
    /// where its parent's pose puts that point: `t = J*P - J*R`.
    pub fn matrices(&self, buzz: &[Option<Matrix>]) -> Vec<Option<Matrix>> {
        let identity = || {
            let mut m = [[0.0; 4]; 4];
            (0..4).for_each(|i| m[i][i] = 1.0);
            m
        };
        // Per node: the motion after the rest correction (`None` = Buzz's part is not animated).
        let mut delta: Vec<Option<Matrix>> = self
            .map
            .iter()
            .enumerate()
            .map(|(w, &b)| {
                let m = buzz.get(b).copied().flatten()?;
                let (cb, cw) = (self.centre_buzz[b], self.centre_other[w]);
                let rot = |c: [f32; 3], j: usize| c[0] * m[0][j] + c[1] * m[1][j] + c[2] * m[2][j];
                let mut d = m;
                for j in 0..3 {
                    let moved = rot(cb, j) + m[3][j] - cb[j];
                    d[3][j] = cw[j] - rot(cw, j) + self.s * moved;
                }
                Some(d)
            })
            .collect();
        for &(w, parent, at) in &self.joints {
            let Some(m) = buzz.get(self.map[w]).copied().flatten() else { continue };
            let p = delta[parent].unwrap_or_else(identity);
            let held = apply(&p, at);
            let mut d = m;
            for j in 0..3 {
                d[3][j] = held[j] - (at[0] * m[0][j] + at[1] * m[1][j] + at[2] * m[2][j]);
            }
            delta[w] = Some(d);
        }
        delta
            .into_iter()
            .enumerate()
            .map(|(w, d)| match d {
                Some(d) => Some(self.rest[w].map_or(d, |q| mul(&q, &d))),
                None => self.rest[w],
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WOODY: &str = r#"
id = "play-as-woody"
name = "Play as Woody"
version = "0.1.0"
authors = ["lewiloon2k"]
description = "Run through any level as Woody."
engine = "0.1"

[play_as]
creature = "WOODY"
model = "data/level05/level1.ngn"
animation = "retarget"
scale = 1.0
laser = false
map = [[2, 9]]

[[play_as.rest]]
nodes = [5, 2, 3]
pivot = [45.0, -355.0, 5.0]
angles = [0.0, 0.0, 80.0]
"#;

    #[test]
    fn manifest_with_and_without_play_as() {
        let m = parse_manifest(WOODY).unwrap();
        let p = m.play_as.unwrap();
        assert_eq!((p.creature.as_str(), p.animation, p.laser, p.scale), ("WOODY", AnimationMode::Retarget, false, 1.0));
        assert_eq!(p.model.as_deref(), Some("data/level05/level1.ngn"));
        assert_eq!(p.map, [[2, 9]]);
        assert_eq!(p.rest[0].nodes, [5, 2, 3]);

        let plain = parse_manifest("id = \"hd-buzz\"\nname = \"HD Buzz\"\nrequires = [\"base\"]\n").unwrap();
        assert!(plain.play_as.is_none());
        assert_eq!(plain.requires, ["base"]);
    }

    #[test]
    fn hop_keys_parse_and_defaults_apply() {
        let m = parse_manifest("id = \"t\"\n[play_as]\ncreature = \"TINMAN\"\nanimation = \"hop\"\nscale = 0.5\nidle = 0\nhop = 1\n").unwrap();
        let p = m.play_as.unwrap();
        assert_eq!((p.animation, p.scale, p.idle, p.hop, p.laser, p.hop_height, p.touch_damage), (AnimationMode::Hop, 0.5, Some(0), Some(1), true, 1.0, false));
        assert!(p.model.is_none() && p.map.is_empty());
    }

    #[test]
    fn unknown_keys_and_modes_are_errors() {
        assert!(parse_manifest("id = \"a\"\nbogus = 1\n").is_err());
        assert!(parse_manifest("id = \"a\"\n[play_as]\ncreature = \"X\"\nbogus = 1\n").is_err());
        assert!(parse_manifest("id = \"a\"\n[play_as]\ncreature = \"X\"\nanimation = \"fly\"\n").is_err());
        assert!(parse_manifest("id = \"a\"\n[play_as]\nscale = 1.0\n").is_err(), "creature is required");
    }

    #[test]
    fn load_errors_name_the_folder_and_last_play_as_wins() {
        let root = std::env::temp_dir().join(format!("opents2-modding-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let write = |name: &str, body: &str| {
            std::fs::create_dir_all(root.join(name)).unwrap();
            std::fs::write(root.join(name).join(MANIFEST), body).unwrap();
        };
        write("a", "id = \"a\"\n[play_as]\ncreature = \"WOODY\"\n");
        write("b", "id = \"b\"\n");
        write("c", "id = \"c\"\n[play_as]\ncreature = \"TINMAN\"\n");
        write("bad", "id = \"bad\"\nnope = 1\n");
        let err = load_mod(&root.join("bad")).unwrap_err();
        assert!(err.contains("bad") && err.contains("nope"), "{err}");
        assert!(load_mod(&root.join("missing")).unwrap_err().contains("missing"));
        let mods: Vec<Mod> = ["a", "b", "c"].iter().map(|n| load_mod(&root.join(n)).unwrap()).collect();
        assert_eq!(active_play_as(&mods).unwrap().1.creature, "TINMAN");
        assert_eq!(active_play_as(&mods[..2]).unwrap().1.creature, "WOODY");
        assert!(active_play_as(&mods[1..2]).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dirs_come_from_the_environment_then_arguments() {
        let env = std::env::join_paths(["x", "y"]).unwrap();
        let dirs = mod_dirs(Some(&env), &[PathBuf::from("z")]);
        assert_eq!(dirs, [PathBuf::from("x"), PathBuf::from("y"), PathBuf::from("z")]);
        assert!(mod_dirs(None, &[]).is_empty());
    }

    fn part(lo: [f32; 3], hi: [f32; 3]) -> Option<Bounds> {
        Some(Bounds { lo, hi })
    }

    /// A stick figure: head, torso, and a limb on each side; y is negative upward, feet at 0.
    fn figure(k: f32) -> Vec<Option<Bounds>> {
        let sc = |p: Option<Bounds>| p.map(|b| Bounds { lo: b.lo.map(|v| v * k), hi: b.hi.map(|v| v * k) });
        [
            part([-20.0, -400.0, -20.0], [20.0, -340.0, 20.0]),   // 0 head
            part([-40.0, -340.0, -20.0], [40.0, -200.0, 20.0]),   // 1 torso
            part([-100.0, -330.0, -10.0], [-40.0, -280.0, 10.0]), // 2 left arm
            part([40.0, -330.0, -10.0], [100.0, -280.0, 10.0]),   // 3 right arm
            part([-30.0, -200.0, -10.0], [-5.0, 0.0, 10.0]),      // 4 left leg
            part([5.0, -200.0, -10.0], [30.0, 0.0, 10.0]),        // 5 right leg
            None,                                                 // 6 no mesh
        ]
        .map(sc)
        .to_vec()
    }

    #[test]
    fn node_map_matches_like_with_like_at_another_size() {
        let buzz = figure(1.0);
        let other = figure(1.2);
        let map = auto_node_map(&buzz, &other);
        assert_eq!(map.len(), other.len(), "every node gets a partner");
        assert_eq!(&map[..6], &[0, 1, 2, 3, 4, 5]);
        assert!(map[6] < buzz.len());
        // Left stays left: a low part on the left follows the left leg.
        let mut odd = figure(1.0);
        odd[2] = part([-100.0, -100.0, -10.0], [-40.0, -50.0, 10.0]);
        assert_eq!(auto_node_map(&buzz, &odd)[2], 4);
        let r = Retarget::new(&buzz, &other, &[[2, 3]], &[], &[]);
        assert_eq!(r.map[2], 3, "overrides win");
        assert!((r.s - 1.2).abs() < 1e-4);
    }

    #[test]
    fn joints_stay_together_when_limbs_are_longer_than_buzz() {
        let buzz = figure(1.0);
        let mut other = figure(1.0);
        other[2] = part([-260.0, -330.0, -10.0], [-40.0, -280.0, 10.0]); // a much longer left arm
        let shoulder = [-40.0, -305.0, 0.0];
        let rest = [RestPose { nodes: vec![2], pivot: shoulder, angles: [0.0, 0.0, 30.0] }];
        let joints = [Joint { node: 2, parent: 1, at: shoulder }];
        let r = Retarget::new(&buzz, &other, &[], &rest, &joints);
        // Buzz: torso shifted and leaning, left arm swung 90 degrees about its own centre.
        let mut pose: Vec<Option<Matrix>> = vec![None; buzz.len()];
        let mut torso = rotation_deg([0.0, 0.0, 10.0]);
        torso[3] = [5.0, -3.0, 2.0, 1.0];
        pose[1] = Some(torso);
        pose[2] = Some(rotation_deg([0.0, 0.0, 90.0]));
        let m = r.matrices(&pose);
        let (arm, body) = (m[2].unwrap(), m[1].unwrap());
        let (a, b) = (apply(&arm, shoulder), apply(&body, shoulder));
        assert!((0..3).all(|k| (a[k] - b[k]).abs() < 1e-3), "shoulder {a:?} vs {b:?}");
    }

    #[test]
    fn retarget_turns_about_the_part_centre_and_scales_the_move() {
        let buzz = figure(1.0);
        let other = figure(2.0);
        let r = Retarget::new(&buzz, &other, &[], &[], &[]);
        // Buzz's head: identity rotation, moved by (10, 0, 0): the other head moves by s * 10.
        let mut pose: Vec<Option<Matrix>> = vec![None; buzz.len()];
        let mut m = [[0.0; 4]; 4];
        for i in 0..4 {
            m[i][i] = 1.0;
        }
        m[3][0] = 10.0;
        pose[0] = Some(m);
        let out = r.matrices(&pose);
        assert_eq!(out[0].unwrap()[3], [20.0, 0.0, 0.0, 1.0]);
        assert!(out[1].is_none(), "a part Buzz leaves at rest stays at rest");
        // A half turn about Y of the head: the head's own centre stays put.
        let mut h = [[0.0; 4]; 4];
        (h[0][0], h[1][1], h[2][2], h[3][3]) = (-1.0, 1.0, -1.0, 1.0);
        pose[0] = Some(h);
        let out = r.matrices(&pose)[0].unwrap();
        let c = other[0].unwrap().centre();
        let moved = [0, 1, 2].map(|j| c[0] * out[0][j] + c[1] * out[1][j] + c[2] * out[2][j] + out[3][j]);
        assert!(dist(moved, c) < 1e-3, "{moved:?} vs {c:?}");
    }

    #[test]
    fn rest_correction_swings_a_part_about_its_pivot_before_the_motion() {
        let buzz = figure(1.0);
        let other = figure(1.0);
        // Turn the right arm (node 3) 90 degrees about Z around the shoulder at (40, -305, 0):
        // its far end (100, -305) goes straight down to (40, -245).
        let rest = [RestPose { nodes: vec![3], pivot: [40.0, -305.0, 0.0], angles: [0.0, 0.0, 90.0] }];
        let r = Retarget::new(&buzz, &other, &[], &rest, &[]);
        let mut pose: Vec<Option<Matrix>> = vec![None; buzz.len()];
        let q = r.matrices(&pose)[3].unwrap();
        let end = apply(&q, [100.0, -305.0, 0.0]);
        assert!(dist(end, [40.0, -245.0, 0.0]) < 1e-3, "{end:?}");
        assert!(r.matrices(&pose)[2].is_none(), "other parts stay at rest");
        // Buzz's identity motion leaves the corrected pose as it was.
        pose[3] = Some(IDENTITY);
        let q2 = r.matrices(&pose)[3].unwrap();
        assert!(dist(apply(&q2, [100.0, -305.0, 0.0]), end) < 1e-3);
    }
}
