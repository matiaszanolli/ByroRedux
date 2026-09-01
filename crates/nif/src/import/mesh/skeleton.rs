//! External-skeleton bone-name resolution for Starfield `BSSkin` (#3549).
//!
//! # The defect
//!
//! Starfield apparel and body meshes carry a `BSSkin::Instance` whose
//! `bone_refs` are **all NULL**. Census over 68,459 vanilla NIFs: 5,896
//! `BSSkin::Instance` blocks, 107,717 bone refs, **78,587 (73%) NULL**;
//! 3,738 skins (63%) have every ref NULL, 2,072 (35%) resolve fully in-file
//! (creatures ship their own skeleton), 86 are mixed. `resolve_node_name`
//! then returned `None` and the caller synthesized the literal name
//! `Bone{i}` — manufacturing the very placeholders it was later reported as
//! failing to match, so every Starfield NPC and every piece of apparel
//! rendered in bind pose.
//!
//! The identity is nowhere in the file. The header string table of such a
//! mesh carries only `ExportScene`, `BSX`, the mesh name and material paths,
//! and no `NiExtraData` block holds bone names. The external skeleton is
//! therefore mandatory.
//!
//! # The recovery, and why it is derived rather than assumed
//!
//! `BsSkinBoneTrans` stores the world→bone bind inverse, so a bone's bind
//! world position is `-Rᵀ·t`. Measured against the bone's own node on skins
//! whose refs DO resolve, that lands on `node_world + C` with **C a single
//! constant per file** — identical across all 50 bones of one mesh, across
//! the several skins in one file, and different between files (piglet
//! `[0, -1.2975, 0.6617]`, loxodonta `[0, -1.4826, 2.0571]`, trapmaw
//! `[0, -1.0579, 1.3137]`). It is not the skeleton root, the owning
//! geometry's transform, or the root chain — all measured at the origin.
//!
//! C never has to be known. A skin with N bones and one unknown C (3 DOF) is
//! massively overdetermined, so [`solve_bone_names`] anchors C on each
//! candidate pairing of bone 0 to a skeleton node and counts how many of the
//! remaining bones then land on a node. A **unique** full-match C is a
//! solved answer.
//!
//! Validated against ground truth — 346 in-file skins where the true mapping
//! is known: unique C solved on 239 (69%), ambiguous on 49, no full match on
//! 58. Of the 9,057 bone names recovered where C was unique, **8,708 exact,
//! 349 position-coincident (colocated twist bones, identical bind), and ZERO
//! WRONG.** That is the property this module is built around: it either
//! resolves correctly or declines, and declining is exactly the prior
//! behaviour.

use crate::blocks::node::NiNode;
use crate::blocks::skin::BsSkinBoneTrans;
use crate::scene::NifScene;
use std::sync::Arc;

/// Match tolerance in skeleton units (metres for Starfield). 2 mm is far
/// below the closest distinct joint spacing and far above f32 composition
/// error over a ~10-deep parent chain.
const MATCH_EPSILON: f32 = 0.002;

/// Minimum bones before a solve is attempted. The anchor-and-count argument
/// is only overdetermined when N is comfortably above C's 3 degrees of
/// freedom; below this a coincidental full match is plausible.
const MIN_BONES_TO_SOLVE: usize = 8;

/// A skeleton's named joints with their bind-pose world translations.
pub struct SkeletonBones {
    joints: Vec<([f32; 3], Arc<str>)>,
}

impl SkeletonBones {
    pub fn is_empty(&self) -> bool {
        self.joints.is_empty()
    }

    /// Compose every named `NiNode`'s world translation.
    ///
    /// Full rotation+translation composition down the parent chain —
    /// translation-only composition is meaningless once a joint rotates, and
    /// produces a blob near the origin rather than a figure.
    pub fn from_scene(scene: &NifScene) -> Self {
        let n = scene.blocks.len();
        let mut parent = vec![usize::MAX; n];
        for i in 0..n {
            if let Some(node) = scene.get_as::<NiNode>(i) {
                for child in &node.children {
                    if let Some(ci) = child.index() {
                        if ci < n {
                            parent[ci] = i;
                        }
                    }
                }
            }
        }

        let mut joints = Vec::new();
        for i in 0..n {
            let Some(node) = scene.get_as::<NiNode>(i) else {
                continue;
            };
            let Some(name) = node.name().map(Arc::<str>::from) else {
                continue;
            };
            let mut chain = Vec::new();
            let (mut cur, mut guard) = (i, 0usize);
            while cur != usize::MAX && guard < 128 {
                chain.push(cur);
                cur = parent[cur];
                guard += 1;
            }
            let mut rot = [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
            let mut pos = [0.0f32; 3];
            for &idx in chain.iter().rev() {
                let Some(nd) = scene.get_as::<NiNode>(idx) else {
                    continue;
                };
                let lr = nd.transform().rotation.rows;
                let lt = nd.transform().translation;
                let next = [
                    pos[0] + rot[0][0] * lt.x + rot[0][1] * lt.y + rot[0][2] * lt.z,
                    pos[1] + rot[1][0] * lt.x + rot[1][1] * lt.y + rot[1][2] * lt.z,
                    pos[2] + rot[2][0] * lt.x + rot[2][1] * lt.y + rot[2][2] * lt.z,
                ];
                let mut nr = [[0.0f32; 3]; 3];
                for a in 0..3 {
                    for b in 0..3 {
                        nr[a][b] =
                            rot[a][0] * lr[0][b] + rot[a][1] * lr[1][b] + rot[a][2] * lr[2][b];
                    }
                }
                rot = nr;
                pos = next;
            }
            joints.push((pos, name));
        }
        Self { joints }
    }

    /// Test-only constructor.
    #[cfg(test)]
    pub fn from_joints(joints: Vec<([f32; 3], Arc<str>)>) -> Self {
        Self { joints }
    }

    fn nearest(&self, p: [f32; 3]) -> Option<&Arc<str>> {
        self.joints
            .iter()
            .find(|(s, _)| {
                (s[0] - p[0]).abs() < MATCH_EPSILON
                    && (s[1] - p[1]).abs() < MATCH_EPSILON
                    && (s[2] - p[2]).abs() < MATCH_EPSILON
            })
            .map(|(_, n)| n)
    }
}

/// A bone's bind-pose world position: `BsSkinBoneTrans` holds the world→bone
/// inverse, so inverting it is `-Rᵀ·t`.
pub fn bind_world_position(bone: &BsSkinBoneTrans) -> [f32; 3] {
    let (r, t) = (bone.rotation, bone.translation);
    [
        -(r[0][0] * t[0] + r[1][0] * t[1] + r[2][0] * t[2]),
        -(r[0][1] * t[0] + r[1][1] * t[1] + r[2][1] * t[2]),
        -(r[0][2] * t[0] + r[1][2] * t[1] + r[2][2] * t[2]),
    ]
}

/// Resolve every bone's name by solving the per-file offset C against
/// `skeleton`.
///
/// Returns `Some(names)` only when a **unique** anchor yields a full match —
/// the condition under which the measured error rate over 9,057 ground-truth
/// bones is zero. Every other outcome returns `None` so the caller keeps its
/// existing behaviour: ambiguous (several anchors fit) and unsolved (none
/// fits) are both declines, never guesses.
#[cfg(test)]
pub fn solve_bone_names(binds: &[[f32; 3]], skeleton: &SkeletonBones) -> Option<Vec<Arc<str>>> {
    solve_bone_names_with_offset(binds, skeleton).map(|(names, _)| names)
}

/// [`solve_bone_names`], also returning the solved offset so a caller can
/// memoise it per skeleton.
pub fn solve_bone_names_with_offset(
    binds: &[[f32; 3]],
    skeleton: &SkeletonBones,
) -> Option<(Vec<Arc<str>>, [f32; 3])> {
    if binds.len() < MIN_BONES_TO_SOLVE || skeleton.is_empty() {
        return None;
    }
    let anchor = binds[0];
    let mut solution: Option<(Vec<Arc<str>>, [f32; 3])> = None;
    let mut full_matches = 0usize;
    for (joint, _) in &skeleton.joints {
        let c = [
            anchor[0] - joint[0],
            anchor[1] - joint[1],
            anchor[2] - joint[2],
        ];
        if let Some(names) = names_at_offset(binds, skeleton, c) {
            full_matches += 1;
            if full_matches > 1 {
                // Ambiguous: more than one C fits every bone. Decline rather
                // than pick — the zero-wrong guarantee is conditioned on
                // uniqueness.
                return None;
            }
            solution = Some((names, c));
        }
    }
    solution
}

/// Every bone's name at a **known** offset, or `None` unless all of them land
/// on a skeleton joint.
///
/// The all-or-nothing requirement is the half of the zero-wrong result that
/// does not depend on how C was obtained, which is what makes it safe to
/// reuse an offset another mesh established by a unique solve.
pub fn names_at_offset(
    binds: &[[f32; 3]],
    skeleton: &SkeletonBones,
    c: [f32; 3],
) -> Option<Vec<Arc<str>>> {
    let mut names = Vec::with_capacity(binds.len());
    for b in binds {
        names.push(
            skeleton
                .nearest([b[0] - c[0], b[1] - c[1], b[2] - c[2]])?
                .clone(),
        );
    }
    Some(names)
}

/// Per-skeleton offset, populated by the first unique solve against it.
///
/// C is a property of the skeleton, not of the mesh: measured near-identical
/// across every outfit that solves against the human skeleton
/// (~`[0, 0.0018, -1.632]`). Reusing it lets a mesh whose own anchor search
/// is ambiguous still resolve — with C fixed there is nothing left to
/// disambiguate, and the all-bones-must-match guard still applies.
fn offset_memo() -> &'static std::sync::Mutex<std::collections::HashMap<&'static str, [f32; 3]>> {
    static MEMO: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<&'static str, [f32; 3]>>,
    > = std::sync::OnceLock::new();
    MEMO.get_or_init(Default::default)
}

/// Skeleton NIFs a NULL-ref Starfield skin is resolved against, in probe
/// order.
///
/// Scoped to the human skeletons deliberately. The reported population is
/// *"all SF actors and apparel"* — 3,738 of 5,896 skins with every ref NULL —
/// and those are body and clothing meshes authored against these two. The
/// 2,072 skins that resolve in-file are creatures shipping their own
/// skeleton and need nothing here.
///
/// Trying several is safe because [`solve_bone_names`] declines unless a
/// UNIQUE anchor produces a full match: a wrong skeleton does not fit every
/// bone at one offset, so it returns `None` rather than a plausible-looking
/// wrong answer.
const SKELETON_CANDIDATES: [&str; 2] = [
    r"meshes\actors\human\characterassets\skeleton.nif",
    r"meshes\actors\human\_1stperson\characterassets\skeleton.nif",
];

/// Process-lifetime cache of parsed skeletons, keyed by archive path.
///
/// Without it every skinned mesh in a cell would re-extract and re-parse a
/// 116-node skeleton NIF — thousands of times per load. Mirrors the
/// `sf_cdb_cache` shape in `asset_provider/material.rs`: module-scope, keyed
/// by path, and caching the *negative* result too so a missing skeleton is
/// probed once.
type SkeletonMap = std::collections::HashMap<&'static str, Option<Arc<SkeletonBones>>>;
type SkeletonCache = std::sync::Mutex<SkeletonMap>;

fn skeleton_cache() -> &'static SkeletonCache {
    static CACHE: std::sync::OnceLock<SkeletonCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Resolve a NULL-ref skin's bone names against the external skeletons.
///
/// Returns `None` when no candidate skeleton yields a unique full match —
/// which is the caller's cue to keep its existing `Bone{i}` behaviour.
pub fn resolve_external_bone_names(
    binds: &[[f32; 3]],
    resolver: &dyn crate::import::MeshResolver,
) -> Option<Vec<Arc<str>>> {
    for path in SKELETON_CANDIDATES {
        let skeleton = {
            let mut cache = skeleton_cache().lock().unwrap_or_else(|e| e.into_inner());
            cache
                .entry(path)
                .or_insert_with(|| {
                    let bytes = resolver.resolve(path)?;
                    let scene = crate::parse_nif(&bytes).ok()?;
                    let bones = SkeletonBones::from_scene(&scene);
                    (!bones.is_empty()).then(|| Arc::new(bones))
                })
                .clone()
        };
        let Some(skeleton) = skeleton else { continue };
        if let Some((names, c)) = solve_bone_names_with_offset(binds, &skeleton) {
            offset_memo()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entry(path)
                .or_insert(c);
            return Some(names);
        }
        // This mesh's own anchor search was ambiguous or found nothing. If an
        // earlier mesh established C for this skeleton by a UNIQUE solve,
        // apply it directly — see `offset_memo`.
        let known = offset_memo()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(path)
            .copied();
        if let Some(names) = known.and_then(|c| names_at_offset(binds, &skeleton, c)) {
            return Some(names);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skel(joints: &[([f32; 3], &str)]) -> SkeletonBones {
        SkeletonBones::from_joints(joints.iter().map(|(p, n)| (*p, Arc::from(*n))).collect())
    }

    /// The whole method: bind positions offset by an arbitrary unknown C must
    /// recover the skeleton's own names.
    #[test]
    fn a_unique_offset_recovers_every_bone_name() {
        let joints: Vec<([f32; 3], &str)> = vec![
            ([0.0, 0.0, 0.0], "Root"),
            ([0.0, 0.0, 1.0], "Spine"),
            ([0.3, 0.0, 1.4], "R_Shoulder"),
            ([-0.3, 0.0, 1.4], "L_Shoulder"),
            ([0.5, 0.0, 1.2], "R_Elbow"),
            ([-0.5, 0.0, 1.2], "L_Elbow"),
            ([0.2, 0.0, 0.5], "R_Knee"),
            ([-0.2, 0.0, 0.5], "L_Knee"),
            ([0.2, 0.0, 0.1], "R_Foot"),
            ([-0.2, 0.0, 0.1], "L_Foot"),
        ];
        let s = skel(&joints);
        // An arbitrary per-file constant, the shape the census found.
        let c = [0.0, -1.4826, 2.0571];
        let binds: Vec<[f32; 3]> = joints
            .iter()
            .map(|(p, _)| [p[0] + c[0], p[1] + c[1], p[2] + c[2]])
            .collect();

        let names = solve_bone_names(&binds, &s).expect("a unique C must solve");
        let expected: Vec<&str> = joints.iter().map(|(_, n)| *n).collect();
        let got: Vec<&str> = names.iter().map(|n| &**n).collect();
        assert_eq!(got, expected);
    }

    /// A skin whose bones do not all lie on the skeleton must DECLINE, not
    /// partially resolve. Declining is the prior behaviour; a wrong bone name
    /// animates geometry by the wrong joint.
    #[test]
    fn a_skin_that_does_not_fit_the_skeleton_declines() {
        let s = skel(&[
            ([0.0, 0.0, 0.0], "Root"),
            ([0.0, 0.0, 1.0], "Spine"),
            ([0.3, 0.0, 1.4], "R_Shoulder"),
            ([-0.3, 0.0, 1.4], "L_Shoulder"),
            ([0.5, 0.0, 1.2], "R_Elbow"),
            ([-0.5, 0.0, 1.2], "L_Elbow"),
            ([0.2, 0.0, 0.5], "R_Knee"),
            ([-0.2, 0.0, 0.5], "L_Knee"),
        ]);
        // One bone deliberately off-skeleton.
        let mut binds: Vec<[f32; 3]> = vec![
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.3, 0.0, 1.4],
            [-0.3, 0.0, 1.4],
            [0.5, 0.0, 1.2],
            [-0.5, 0.0, 1.2],
            [0.2, 0.0, 0.5],
            [-0.2, 0.0, 0.5],
        ];
        binds[3] = [9.0, 9.0, 9.0];
        assert!(solve_bone_names(&binds, &s).is_none());
    }

    /// A translationally symmetric skeleton admits more than one C. The
    /// zero-wrong guarantee is conditioned on uniqueness, so ambiguity must
    /// decline rather than pick the first fit.
    #[test]
    fn an_ambiguous_offset_declines_rather_than_picking() {
        // Evenly spaced colinear joints: shifting by one spacing maps the
        // set onto a subset of itself, so two anchors both fit.
        let joints: Vec<([f32; 3], &str)> = (0..12)
            .map(|i| ([0.0, 0.0, i as f32], "j"))
            .collect::<Vec<_>>()
            .iter()
            .map(|(p, n)| (*p, *n))
            .collect();
        let s = skel(&joints);
        let binds: Vec<[f32; 3]> = (0..10).map(|i| [0.0, 0.0, i as f32]).collect();
        assert!(
            solve_bone_names(&binds, &s).is_none(),
            "several offsets fit — must decline"
        );
    }

    /// A mesh whose own anchor search is ambiguous still resolves once C is
    /// known from another mesh's unique solve — with the offset fixed there
    /// is nothing left to disambiguate, and the all-bones-must-match guard
    /// (the half of the zero-wrong result independent of how C was obtained)
    /// still applies. This is what lifts apparel coverage from ~21% to ~47%.
    #[test]
    fn a_known_offset_resolves_what_an_ambiguous_search_cannot() {
        let joints: Vec<([f32; 3], &str)> = (0..12).map(|i| ([0.0, 0.0, i as f32], "j")).collect();
        let s = skel(&joints);
        // Evenly spaced and colinear: several offsets fit, so the search
        // itself must decline...
        let binds: Vec<[f32; 3]> = (0..10).map(|i| [0.0, 0.0, i as f32]).collect();
        assert!(solve_bone_names(&binds, &s).is_none());
        // ...but with C supplied, every bone lands and it resolves.
        assert_eq!(
            names_at_offset(&binds, &s, [0.0, 0.0, 0.0]).map(|n| n.len()),
            Some(10)
        );
        // A C that does NOT place every bone on a joint still declines.
        assert!(names_at_offset(&binds, &s, [0.0, 0.0, 0.5]).is_none());
    }

    /// Too few bones to be overdetermined against C's 3 DOF.
    #[test]
    fn a_tiny_bone_set_is_not_solved() {
        let s = skel(&[([0.0, 0.0, 0.0], "Root"), ([0.0, 0.0, 1.0], "Spine")]);
        assert!(solve_bone_names(&[[0.0, 0.0, 0.0]], &s).is_none());
    }

    /// An empty skeleton is a decline, not a panic.
    #[test]
    fn an_empty_skeleton_declines() {
        let s = skel(&[]);
        let binds: Vec<[f32; 3]> = (0..10).map(|i| [0.0, 0.0, i as f32]).collect();
        assert!(solve_bone_names(&binds, &s).is_none());
    }
}
