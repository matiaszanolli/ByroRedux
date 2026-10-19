//! Authored corpse poses — the `XRGD` ragdoll pose of a placed actor (#5015).
//!
//! Skyrim and FO4 author a placed corpse as an ACHR with `Starts Dead` plus an
//! `XRGD` pose. Without it the corpse spawns upright and ragdoll-collapses
//! from its idle pose instead of lying where it was authored. This module
//! poses the freshly spawned skeleton from the decoded entries
//! ([`PlacedRef::ragdoll_pose`](byroredux_plugin::esm::cell::PlacedRef)) just
//! before [`crate::ragdoll::activate_ragdoll`] seeds its bodies.
//!
//! ## What the entries mean (derived empirically, not sourced)
//!
//! No document names the layout, so it was recovered from the vanilla
//! `Skyrim.esm` corpus (433 humanoid corpses with 18 entries) against the
//! character skeleton (`skeleton.nif` ragdoll bodies, `skeleton.hkx`'s ragdoll
//! `hkaSkeleton`). Every rule below is an **interpretation** that the data
//! supports; the evidence is reproducible with
//! `crates/plugin/examples/xrgd_dump.rs`:
//!
//! 1. **Entry `k` is the skeleton's ragdoll body `k`**, in the order the
//!    skeleton NIF lists its `bhkRigidBody`s (COM, L leg, R leg, spine, neck,
//!    head, R arm, L arm for the humanoid). The per-entry bone offsets match
//!    the ragdoll skeleton's own bone lengths (thigh 6.615, calf 31.317,
//!    foot 27.44, spine1 8.749 …), and the `bone_id` field is NOT that order
//!    — it is not used.
//! 2. **The rotation is the bone's parent-local rotation in the NIF bone
//!    frames**: the forearm's median deviation from its NIF rest rotation is
//!    5°, the upper arm's 35° against the *NIF* parent (a clavicle) where it
//!    is 120° against the ragdoll skeleton's.
//! 3. **Euler convention**: the Gamebryo `XYZ` product with negated angles,
//!    `Rx(-rx)·Ry(-ry)·Rz(-rz)` in Z-up — exactly
//!    [`euler_zup_to_quat_yup_mode`] mode 0. Of 24 candidate conventions it
//!    alone puts a lying corpse's lowest joint ≈ 12.6 units under the COM
//!    (a body resting on the floor) with a median vertical extent of 38.
//! 4. **Entry 0 is the centre of mass at the reference's position**: its
//!    offset is `(0, 0, 0)`, its rotation lies the body down relative to the
//!    reference's own yaw, and placed corpses stand a median 11.6 units above
//!    the floor of their live neighbours.
//!
//! The pose is applied to the NIF skeleton's own local rotations; the NIF's
//! bone lengths are kept (the ragdoll skeleton's are shorter), so the
//! skinned mesh and the constraint pivots stay in one geometry.
//!
//! ## Fallout 4 (entries named by bone id)
//!
//! An FO4 ragdoll blob stores, per body, the ragdoll-skeleton bone it drives
//! (a `u8`, [`RagdollTemplateBody::pose_bone_id`]), and the `XRGD` entries'
//! `bone_id` is that same id: the id multiset of a corpse's entries equals the
//! blob's for every human (18 of 18 entries) and feral-ghoul corpse, which
//! pins the entry↔body mapping without assuming an order — the entries run
//! COM, L leg, R leg, spine, L arm, neck/head, R arm on the human rig but
//! COM, legs, spine, neck, head, arms on the ghoul's, and both resolve through
//! the ids. Two ids repeat (the COM and the first spine body are both bone 2),
//! so entries claim bodies in order. The entry offsets equal the body-to-body
//! distances to three decimals (thigh 6.614 / 6.615, calf 31.592 / 31.595),
//! the rotation convention is the Skyrim one — on the real NIF bone tree,
//! intrinsic XYZ with negated angles has a median lying-body extent of 41 units
//! on 656 human corpses against 56 for the runner-up (45 against 54 on 303
//! ghouls) — and the trailing marker is any last unmatched entry with a zero
//! rotation. Reproducible with `crates/plugin/examples/xrgd_dump.rs` and
//! `scripts/xrgd-fo4-analysis.py`.
//!
//! **Scope:** only the verified ragdolls are posed — the Skyrim/draugr
//! humanoid, and the FO4 human and feral-ghoul rigs. The entry↔body order is
//! established for the 18-body humanoid skeleton (Skyrim's `skeleton.nif`,
//! also the draugr's), and [`HUMANOID_RAGDOLL_BODIES`] pins it by bone name: a
//! skeleton whose ragdoll bodies are anything else — a creature, the beast
//! races' tailed rig — is refused rather than posed from a guess, and so is a
//! pose whose entry count (a trailing marker entry aside, see
//! [`body_entries`]) differs from the body count. Such a corpse keeps today's
//! behaviour: it collapses from the spawn pose.

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::{
    collect_subtree, EntityId, GlobalTransform, Parent, Transform, World,
};
use byroredux_core::math::coord::euler_zup_to_quat_yup_mode;
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm::cell::RagdollPoseBone;

/// The humanoid ragdoll's bodies in the order the `XRGD` entries list them
/// (the skeleton NIF's `bhkRigidBody` order), by bone name. See the module
/// docs: the order was established from this skeleton alone.
pub(crate) const HUMANOID_RAGDOLL_BODIES: [&str; 18] = [
    "NPC COM [COM ]",
    "NPC L Thigh [LThg]",
    "NPC L Calf [LClf]",
    "NPC L Foot [Lft ]",
    "NPC R Thigh [RThg]",
    "NPC R Calf [RClf]",
    "NPC R Foot [Rft ]",
    "NPC Spine [Spn0]",
    "NPC Spine1 [Spn1]",
    "NPC Spine2 [Spn2]",
    "NPC Neck [Neck]",
    "NPC Head [Head]",
    "NPC R UpperArm [RUar]",
    "NPC R Forearm [RLar]",
    "NPC R Hand [RHnd]",
    "NPC L UpperArm [LUar]",
    "NPC L Forearm [LLar]",
    "NPC L Hand [LHnd]",
];

/// The FO4 human ragdoll's bodies in blob (body-id) order, by bone name.
pub(crate) const FO4_HUMAN_RAGDOLL_BODIES: [&str; 18] = [
    "COM",
    "LLeg_Thigh",
    "RLeg_Thigh",
    "SPINE1",
    "LLeg_Calf",
    "RLeg_Calf",
    "SPINE2",
    "LLeg_Foot",
    "RLeg_Foot",
    "Chest",
    "LArm_UpperArm",
    "Neck",
    "RArm_UpperArm",
    "LArm_ForeArm1",
    "HEAD",
    "RArm_ForeArm1",
    "LArm_Hand",
    "RArm_Hand",
];

/// The FO4 feral-ghoul ragdoll's bodies in blob (body-id) order, by bone name.
pub(crate) const FO4_GHOUL_RAGDOLL_BODIES: [&str; 18] = [
    "COM",
    "RTHIGH",
    "SpineRoot",
    "LTHIGH",
    "RCALF",
    "SPINE1",
    "LCALF",
    "RFOOT",
    "SPINE2",
    "LFOOT",
    "RUPPERARM",
    "Neck",
    "LUPPERARM",
    "RForeArm1",
    "HEAD",
    "LForeArm1",
    "RHAND",
    "LHAND",
];

/// An authored corpse's `XRGD` pose, parked on the actor between spawn and
/// its death reconciliation, which consumes it. Derived from plugin data on
/// every spawn, so it is not part of a save.
#[derive(Debug, Clone)]
pub(crate) struct AuthoredRagdollPose(pub(crate) Vec<RagdollPoseBone>);

impl Component for AuthoredRagdollPose {
    type Storage = SparseSetStorage<Self>;
}

/// `euler_zup_to_quat_yup_mode`'s mode for the `XRGD` rotation convention:
/// the Gamebryo XYZ product with negated angles (see the module docs).
const XRGD_ROTATION_MODE: u8 = 0;

/// The trailing marker entry some poses carry after their bodies: `bone_id`
/// 0, a zero rotation (to float noise), `x` 0 and a nonzero `(y, z)` offset.
/// Measured over the vanilla `Skyrim.esm` corpse poses, 262 of the 19-entry
/// ones end in one and 260 of those list the humanoid body order in their
/// first 18 entries; the offset is one of a handful of constants shared by
/// hundreds of poses — `(17, 10)` ×123, `(32.45, 104.06)` ×120 (the draugr
/// skeleton's), `(34, 10)` ×13 — so it is not pose data. The entry counts only
/// line up with the ragdoll's body counts once it is set aside. What it encodes
/// is not known — it is skipped, not interpreted. (An all-zero last entry is
/// not matched: that is as likely a body at rest.)
fn is_trailing_marker(entry: &RagdollPoseBone) -> bool {
    /// The skeleton-race markers carry `3e-6` of float noise in `x`.
    const NOISE: f32 = 1.0e-4;
    entry.bone_id == 0
        && entry.position[0].abs() < NOISE
        && (entry.position[1].abs() >= NOISE || entry.position[2].abs() >= NOISE)
        && has_zero_rotation(entry)
}

/// A rotation of zero to float noise (the markers carry `-0.0`s and `1e-8`s).
fn has_zero_rotation(entry: &RagdollPoseBone) -> bool {
    entry.rotation.iter().all(|v| v.abs() < 1.0e-4)
}

/// The pose entries that correspond to ragdoll bodies: `entries` without a
/// trailing marker entry.
pub(crate) fn body_entries(entries: &[RagdollPoseBone]) -> &[RagdollPoseBone] {
    match entries.split_last() {
        Some((last, rest)) if is_trailing_marker(last) => rest,
        _ => entries,
    }
}

/// What the pose writes onto one ragdoll body's bone: its parent-local
/// rotation (Y-up), and for the root (the COM) a zeroed local translation so
/// the centre of mass sits at the actor's placement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BonePose {
    pub(crate) rotation: Quat,
    pub(crate) zero_translation: bool,
}

/// The per-body bone poses an `XRGD` pose asks for, or why it cannot be
/// applied. Entry `k` poses ragdoll body `k`.
pub(crate) fn bone_poses(
    entries: &[RagdollPoseBone],
    body_count: usize,
) -> Result<Vec<BonePose>, String> {
    let entries = body_entries(entries);
    if entries.len() != body_count {
        return Err(format!(
            "{} pose entries for {body_count} ragdoll bodies — the entry order is only \
             established for a skeleton whose body count matches",
            entries.len()
        ));
    }
    entries
        .iter()
        .enumerate()
        .map(|(k, entry)| {
            if !entry.position.iter().chain(&entry.rotation).all(|v| v.is_finite()) {
                return Err(format!("pose entry {k} is not finite"));
            }
            let [rx, ry, rz] = entry.rotation;
            Ok(BonePose {
                rotation: euler_zup_to_quat_yup_mode(XRGD_ROTATION_MODE, rx, ry, rz),
                zero_translation: k == 0,
            })
        })
        .collect()
}

/// Whether a ragdoll's bodies, by bone name in template order, are exactly
/// [`HUMANOID_RAGDOLL_BODIES`] (case-insensitive, like the template's own
/// name resolution).
pub(crate) fn is_humanoid_ragdoll(body_bone_names: &[Option<String>]) -> bool {
    body_bone_names.len() == HUMANOID_RAGDOLL_BODIES.len()
        && body_bone_names
            .iter()
            .zip(HUMANOID_RAGDOLL_BODIES)
            .all(|(name, expected)| name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(expected)))
}

/// Whether a ragdoll's bodies, by bone name in template order, are exactly one
/// of the verified FO4 sets ([`FO4_HUMAN_RAGDOLL_BODIES`],
/// [`FO4_GHOUL_RAGDOLL_BODIES`]).
pub(crate) fn is_fo4_posable_ragdoll(body_bone_names: &[Option<String>]) -> bool {
    [&FO4_HUMAN_RAGDOLL_BODIES, &FO4_GHOUL_RAGDOLL_BODIES]
        .into_iter()
        .any(|set| {
            body_bone_names.len() == set.len()
                && body_bone_names
                    .iter()
                    .zip(set)
                    .all(|(name, expected)| name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(expected)))
        })
}

/// FO4: for each ragdoll body (by index), the `XRGD` entry that poses it.
///
/// Entries name the bone they pose by `bone_id`, and each body carries the id
/// of the bone it drives, so an entry claims the first unclaimed body with its
/// id (two bodies can share one: the COM and the first spine body are both bone
/// 2). Every body must be claimed, and at most one entry may be left over: the
/// trailing marker, which has a zero rotation (see [`is_trailing_marker`]).
/// Anything else — a body no entry names, a stray entry — is refused.
pub(crate) fn entries_by_bone_id(
    entries: &[RagdollPoseBone],
    body_ids: &[Option<u8>],
) -> Result<Vec<usize>, String> {
    let mut claimed: Vec<Option<usize>> = vec![None; body_ids.len()];
    let mut leftover = Vec::new();
    for (k, entry) in entries.iter().enumerate() {
        match (0..body_ids.len()).find(|&b| claimed[b].is_none() && body_ids[b] == Some(entry.bone_id)) {
            Some(b) => claimed[b] = Some(k),
            None => leftover.push(k),
        }
    }
    if let Some(b) = claimed.iter().position(Option::is_none) {
        return Err(format!(
            "no pose entry names ragdoll body {b} (bone id {:?}) — the entry set does not cover \
             this skeleton",
            body_ids[b]
        ));
    }
    match leftover.as_slice() {
        [] => {}
        [k] if *k + 1 == entries.len() && has_zero_rotation(&entries[*k]) => {}
        other => {
            return Err(format!(
                "{} pose entries name no ragdoll body — only a single trailing marker is expected",
                other.len()
            ))
        }
    }
    Ok(claimed.into_iter().flatten().collect())
}

/// Per-body poses for FO4: `mapping[b]` is the entry that poses body `b`.
pub(crate) fn bone_poses_mapped(
    entries: &[RagdollPoseBone],
    mapping: &[usize],
) -> Result<Vec<BonePose>, String> {
    mapping
        .iter()
        .map(|&k| {
            let entry = &entries[k];
            if !entry.position.iter().chain(&entry.rotation).all(|v| v.is_finite()) {
                return Err(format!("pose entry {k} is not finite"));
            }
            let [rx, ry, rz] = entry.rotation;
            Ok(BonePose {
                rotation: euler_zup_to_quat_yup_mode(XRGD_ROTATION_MODE, rx, ry, rz),
                zero_translation: k == 0,
            })
        })
        .collect()
}

/// Pose `skeleton_root`'s ragdoll bones from `entries` and recompute the
/// skeleton's world transforms, so the activation that follows seeds its
/// bodies from the authored pose. Returns the number of bones posed.
pub(crate) fn pose_skeleton(
    world: &World,
    skeleton_root: EntityId,
    entries: &[RagdollPoseBone],
) -> Result<usize, String> {
    let bones: Vec<EntityId> = world
        .query::<crate::ragdoll::RagdollTemplate>()
        .and_then(|templates| {
            templates
                .get(skeleton_root)
                .map(|template| template.bodies.iter().map(|body| body.bone).collect())
        })
        .ok_or("no ragdoll template to pose")?;
    let names: Vec<Option<String>> = bones
        .iter()
        .map(|&bone| crate::commands::shared::resolve_entity_name(world, bone))
        .collect();
    let poses = if is_humanoid_ragdoll(&names) {
        bone_poses(entries, bones.len())?
    } else if is_fo4_posable_ragdoll(&names) {
        let ids: Vec<Option<u8>> = world
            .query::<crate::ragdoll::RagdollTemplate>()
            .and_then(|templates| {
                templates
                    .get(skeleton_root)
                    .map(|template| template.bodies.iter().map(|body| body.pose_bone_id).collect())
            })
            .ok_or("no ragdoll template to pose")?;
        let mapping = entries_by_bone_id(entries, &ids)?;
        bone_poses_mapped(entries, &mapping)?
    } else {
        return Err(
            "the ragdoll is not a body set the XRGD entry order is established for".to_owned(),
        );
    };

    {
        let mut transforms = world
            .query_mut::<Transform>()
            .ok_or("Transform storage not registered")?;
        for (&bone, pose) in bones.iter().zip(&poses) {
            let Some(local) = transforms.get_mut(bone) else {
                return Err(format!("ragdoll bone {bone} has no Transform"));
            };
            local.rotation = pose.rotation;
            if pose.zero_translation {
                local.translation = Vec3::ZERO;
            }
        }
    }
    recompute_globals(world, skeleton_root);
    Ok(bones.len())
}

/// Recompose every `GlobalTransform` under `root` from the local transforms,
/// parents first (`collect_subtree` visits a parent before its children).
/// `root` keeps its own global. The regular propagation system recomputes
/// the same values next frame; the ragdoll activation cannot wait for it.
fn recompute_globals(world: &World, root: EntityId) {
    let order = collect_subtree(world, root);
    let (Some(transforms), Some(parents), Some(mut globals)) = (
        world.query::<Transform>(),
        world.query::<Parent>(),
        world.query_mut::<GlobalTransform>(),
    ) else {
        return;
    };
    for entity in order {
        if entity == root {
            continue;
        }
        let (Some(local), Some(parent)) = (transforms.get(entity), parents.get(entity)) else {
            continue;
        };
        let Some(parent_global) = globals.get(parent.0).copied() else {
            continue;
        };
        let composed =
            GlobalTransform::compose(&parent_global, local.translation, local.rotation, local.scale);
        if let Some(global) = globals.get_mut(entity) {
            *global = composed;
        }
    }
}

#[cfg(test)]
#[path = "ragdoll_pose_tests.rs"]
mod tests;
