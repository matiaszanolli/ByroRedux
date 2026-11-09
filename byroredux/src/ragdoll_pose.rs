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
//! **Per-rig verification (#6065).** The bone-id mapping is skeleton-independent,
//! but the entry rotation frames and Euler mode are not established per rig, so a
//! rig is added only when `scripts/xrgd-fo4-rigs.py` passes it on both of #5015's
//! checks: each entry's offset length equals its bone's own NIF offset (the
//! verified rigs sit within 3 % on every bone's median: human 17/17, ghoul
//! 16/16), and forward kinematics ranks intrinsic XYZ with negated angles first
//! by lying-body extent. **Which entry poses which body is rig-dependent too.** The bone-id claim
//! below is right where the ids differ (human, ghoul, super mutant), but most creature rigs author
//! id 0 on nearly every body (a radroach 23 of 25, a stingwing 31 of 31), where it degenerates to the
//! blob order, and the entries do not follow that: with it brahmin's entry offsets matched the NIF on
//! 2 of 37 bones, a radroach's on 4 of 21. Entry k poses the k-th ragdoll body bone in the skeleton's
//! own depth-first order ([`entries_by_tree_order`]), and then every measured bone matches
//! (brahmin 37/37, radstag 29/29, yaoguai 16/16, mirelurk 22/22, molerat 17/17, stingwing 30/30,
//! dog 21/21, radroach 20/21 with one 3 % near-miss). **Brahmin, radstag, yaoguai and mirelurk** then
//! also rank the shipped Euler convention first by lying-body extent, and the radroach does too; they
//! are admitted ([`FO4_BRAHMIN_RAGDOLL_BODIES`] and friends). **Molerat, dog and stingwing** have exact
//! offsets but another convention flattens their corpses more (7th, 11th, 11th of 24) — the test that
//! discriminates weakly for a quadruped — and were admitted on the owner's call (2026-11-08), not on
//! the verified bar: if their corpses look wrong when lying, that is the first suspect. The joint
//! pivot as a second offset reference changes no verdict. The cat (17/22), Handy, the FEV hound (one
//! corpse) and the rigs below stay refused. Under the id claim alone only the super mutant
//! passed (15/16 bones — `SPINE1`, a 3-unit bone, is 10 % off — and rank 1 over 7 corpses), which
//! is what hid the creature rigs above: the molerat's offsets were already exact, the radstag and
//! brahmin ranked first with most offsets "off" (they were being compared against the wrong bones).
//! The flatness test discriminates weakly for a quadruped, so the molerat, dog, stingwing and cat
//! failures mean "not established", not "wrong". Also unresolved: the three brahmin corpses with
//! 41 / 42 entries for 39 bodies; the cat's offsets (17/22, with or without the node order); the
//! ghoul poses with 20
//! entries — two carry an all-zero entry and a constant `(0, 40.321, 100)`
//! entry rotated `(-π, π/2, -π)` after the 18 bodies, six are a debug cell whose
//! bone ids are all 0, and four (`NationalGuardTrainingYard01`) carry two
//! varying extras — and the rigs the id set cannot tell apart from a humanoid
//! (Protectron, Handy and Assaultron match no skeleton's ids).
//!
//! **Placement tilt (#6070, open).** The entries are poses relative to the
//! reference, but 466 of the 971 posed FO4 human / ghoul corpses carry a large X
//! or Y rotation on the reference itself (the CK writes the settled root
//! orientation into it; the Skyrim corpses are nearly all upright). Composed
//! the usual way (`REFR_ROTATION_MODE_SHIP`, the same mode as the entries) the
//! pose of such a corpse is not a lying one — MedTekResearch01's
//! `Loot_CorpseFeralGhoul01` (REFR 00106A72, rotation -0.79 / 0.90 / 4.65) seeds
//! 121 units tall with its legs 60 below the centre of mass, and the solver
//! throws it ~2,700 units — and none of the 48 axis-order / angle-sign
//! alternatives (or their inverses) restores the untilted corpses' lying extent
//! (best 32.5 against 25.1 for a human; the shipped mode is not among the
//! top eight). Corpses with |X|, |Y| under ~0.25 rad are unaffected, and
//! `docs/smoke-tests/fo4-ragdoll.sh` gates those. The composition for the rest
//! is not established.
//!
//! ## Other Skyrim rigs (#6068, #6069)
//!
//! The order rule was derived on one skeleton, so every further Skyrim rig is
//! gated on its own evidence: `scripts/xrgd-skyrim-rigs.py` repeats #5015's two
//! checks per rig. **Offsets**: each entry's offset length matches its body's
//! bone length within 3 % (median over the rig's corpses) — the NIF bone's
//! own local offset or the ragdoll `hkaSkeleton`'s bone; the humanoid needs
//! both (calves, forearms and head follow the ragdoll skeleton's lengths, the
//! upper arms the NIF's), 17/17 that way. **FK**: forward kinematics on the
//! NIF bone tree ranks intrinsic XYZ with negated angles first of the 24
//! conventions by the lying body's median vertical extent (humanoid: 24.6
//! against 42.1 for the runner-up, 114.3 standing). Passed:
//!
//! - **Draugr** (`draugr\…\skeleton.nif` and `skeletonf.nif`, DraugrRace and
//!   RigidSkeletonRace): the humanoid order, but the left foot is
//!   `NPC L Foot [LLft ]`, so [`HUMANOID_RAGDOLL_BODIES`] never matched it and
//!   these corpses were refused until [`DRAUGR_RAGDOLL_BODIES`]. 200 of 203
//!   corpses carry 18 entries; offsets 17/17 within 0.4 % of the NIF's; extent
//!   28.8 against 45.3 (126.5 standing). The 11 whose upper-arm offset is
//!   `skeletonf.nif`'s 8.76 rather than `skeleton.nif`'s 11.35 pass on
//!   `skeletonf.nif` alone (17/17, extent 23.4 against 49.1).
//! - **SkeletonS** (`draugr\…\skeletons.nif`, SkeletonRace): the draugr order
//!   with `NPC Head [Jaw]` as body 12, [`SKELETON_S_RAGDOLL_BODIES`]. 53 of 120
//!   corpses carry 19 entries; offsets 18/18 within 0.6 % (the jaw's 2.490
//!   against 2.491); extent 26.0 against 41.1. The other 67 are refused by the
//!   count gate: the 65 18-entry poses fall in two families — 36 list the
//!   draugr order without the jaw, 29 drop the neck instead (and carry
//!   exploded offsets, up to 15 000 units) — that only the unused `bone_id`
//!   tells apart, and the two 20-entry poses end in a constant entry, offset
//!   `(0, -8.26, 3.28)` and rotated `(-π/2, 0, 0)`, that no body explains.
//! - **Falmer** (FalmerRace): the humanoid order with bracketless names,
//!   [`FALMER_RAGDOLL_BODIES`]. 20 of 20 corpses; 17/17 within 0.4 %; extent
//!   27.8 against 53.8.
//! - **Skeever** (SkeeverRace, SkeeverWhiteRace), [`SKEEVER_RAGDOLL_BODIES`]:
//!   54 of 54 corpses carry 21 entries for 21 bodies; 19/19 within 1.0 % of
//!   the NIF's (`SpineLowerSpine` has zero length); extent 26.0 against 34.4
//!   (39.2 standing).
//! - **Deer** (DeerRace, ElkRace), [`DEER_RAGDOLL_BODIES`]: 30 of 31 corpses
//!   carry 24 entries for 24 bodies; 21/22 within 2.8 % of the NIF's, and
//!   `ElkScull` (56 % off the NIF) within 0.1 % of the ragdoll skeleton's, as
//!   the humanoid head; extent 53.4 against 102.2 (152.7 standing).
//!
//! Not established, so refused: the **dwarven spider** (3 of 65 corpses carry
//! its 30 bodies' 30 entries — 29/29, rank 1, too few — 51 carry 34 and 10
//! carry 58, whose first 30 match 18/29 and 19/29 bones); the **frostbite
//! spider** (all 12 carry 58 entries for 48 bodies: the first 48 match 47/47,
//! the 10 after carry real rotations no body explains); the **goat** (8 of 16
//! carry 22 entries, 8 carry 23; `Goat_Spine2`'s offset is 0.2 against 13.4,
//! though FK ranks first, 23.4 against 36.7); the **horse** (8 of 14 carry 27,
//! 6 carry 26; `SaddleBone` 70.1 against 26.8; rank 1, 40.4 against 93.4); the
//! **wolf** (12 of 12 carry 22; both `FrontLeg1` offsets are 15.4 against 8.9
//! in the NIF and 26.9 in the ragdoll skeleton; rank 1, 32.7 against 46.6) and
//! the dog, whose skeleton carries the same bodies and fails the same two (3
//! corpses, rank 3); the **dwarven sphere centurion** (20 of 20, 12/13 —
//! `NPC Pelvis` 20 % off — but rank 13: the non-negated angles lay it flatter,
//! 49.0 against 54.9); the **chaurus** (7 of 7, 24/25 — `Ribcage[Spn3]` 4.2 %
//! off — rank 1 by only 63.5 against 74.4); the mudcrab and slaughterfish,
//! whose offsets match (28/28, 5/5) but whose flat bodies the extent cannot
//! rank (11th, 3rd); the cow (22/22, but 3 corpses and 50.3 against 50.4).
//!
//! ## Fallout 3 / New Vegas (#6252)
//!
//! A FO3/FNV corpse is marked on its base record (`DATA` health <= 0,
//! [`NpcRecord::starts_dead`](byroredux_plugin::esm::records::NpcRecord)), not
//! the placement, and its `XRGD` is pose-only data. The humanoid rig
//! (`characters\_male\skeleton.nif`, the same 18 bodies in both games,
//! [`FALLOUT_HUMANOID_RAGDOLL_BODIES`]) passes #5015's checks by order, as the
//! Skyrim humanoid does: over 202 FNV and 310 FO3 dead-base humanoid corpses
//! (every one with 18 entries but three FO3 19-entry poses, which the count gate
//! refuses, so 202 and 307 are posed — both pinned by the `--ignored`
//! `fnv_xrgd_…` / `fo3_xrgd_…` tests, #6463), each entry's offset matches its body's bone within 3 % for 17/17
//! bones — the NIF's local offset for the legs, spine, neck, head and upper
//! arms, the elbow and wrist hinges' child pivot (19.591, 16.170) for the
//! forearms and hands, whose NIF offsets (17.364, 18.129) put the elbow 2.2
//! units higher — and forward kinematics ranks intrinsic XYZ with negated
//! angles first by lying extent (FNV 20.9 against 34.2, FO3 21.1 against 37.5;
//! 105.7 standing). The `bone_id` field is again not the order (the same
//! permutation on every corpse). Entry 0's body (`Bip01 NonAccum`, at the
//! pelvis) belongs at the reference's position like Skyrim's COM: placed corpses
//! stand a median 10.9 units above their nearest live NPC, and the lying body's
//! lowest bone is 8.9 under that root. Unlike Skyrim's COM, its parent (`Bip01`)
//! carries the 67.8-unit hip height, so the root body is placed at the skeleton
//! root rather than given a zero local translation ([`BonePose::at_placement`]).
//! Creatures stay refused until each rig is verified.
//!
//! **Scope:** only the verified ragdolls are posed — the Skyrim humanoid,
//! draugr, SkeletonS, falmer, skeever and deer rigs, the FO4 human,
//! feral-ghoul and super-mutant rigs, and the FO3/FNV humanoid rig. A Skyrim rig is pinned by its bodies'
//! bone names in order ([`is_skyrim_posable_ragdoll`]): a skeleton whose
//! ragdoll bodies are anything else — another creature, the beast races'
//! tailed rig — is refused rather than posed from a guess, and so is a pose
//! whose entry count (a trailing marker entry aside, see [`body_entries`])
//! differs from the body count. Such a corpse keeps today's behaviour: it
//! collapses from the spawn pose.

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::{
    collect_subtree, Children, EntityId, GlobalTransform, Parent, Transform, World,
};
use byroredux_core::math::coord::euler_zup_to_quat_yup_mode;
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm::cell::RagdollPoseBone;

/// The humanoid ragdoll's bodies in the order the `XRGD` entries list them
/// (the skeleton NIF's `bhkRigidBody` order), by bone name. See the module
/// docs: the order was established from this skeleton; each other Skyrim set
/// below was verified on its own corpses (#6068, #6069).
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

/// The draugr ragdoll's bodies (`draugr\…\skeleton.nif`, `skeletonf.nif`):
/// [`HUMANOID_RAGDOLL_BODIES`] with the left foot named `[LLft ]` (#6068).
pub(crate) const DRAUGR_RAGDOLL_BODIES: [&str; 18] = [
    "NPC COM [COM ]",
    "NPC L Thigh [LThg]",
    "NPC L Calf [LClf]",
    "NPC L Foot [LLft ]",
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

/// The skeleton-pile ragdoll's bodies (`draugr\…\skeletons.nif`,
/// SkeletonRace): [`DRAUGR_RAGDOLL_BODIES`] with the jaw after the head (#6068).
pub(crate) const SKELETON_S_RAGDOLL_BODIES: [&str; 19] = [
    "NPC COM [COM ]",
    "NPC L Thigh [LThg]",
    "NPC L Calf [LClf]",
    "NPC L Foot [LLft ]",
    "NPC R Thigh [RThg]",
    "NPC R Calf [RClf]",
    "NPC R Foot [Rft ]",
    "NPC Spine [Spn0]",
    "NPC Spine1 [Spn1]",
    "NPC Spine2 [Spn2]",
    "NPC Neck [Neck]",
    "NPC Head [Head]",
    "NPC Head [Jaw]",
    "NPC R UpperArm [RUar]",
    "NPC R Forearm [RLar]",
    "NPC R Hand [RHnd]",
    "NPC L UpperArm [LUar]",
    "NPC L Forearm [LLar]",
    "NPC L Hand [LHnd]",
];

/// The falmer ragdoll's bodies: the humanoid order, bracketless names (#6069).
pub(crate) const FALMER_RAGDOLL_BODIES: [&str; 18] = [
    "NPC COM [COM ]",
    "NPC L Thigh",
    "NPC L Calf",
    "NPC L Foot",
    "NPC R Thigh",
    "NPC R Calf",
    "NPC R Foot",
    "NPC Spine",
    "NPC Spine1",
    "NPC Spine2",
    "NPC Neck",
    "NPC Head",
    "NPC R UpperArm",
    "NPC R Forearm",
    "NPC R Hand",
    "NPC L UpperArm",
    "NPC L Forearm",
    "NPC L Hand",
];

/// The skeever ragdoll's bodies in NIF `bhkRigidBody` order (#6069).
pub(crate) const SKEEVER_RAGDOLL_BODIES: [&str; 21] = [
    "COG",
    "LLeg1",
    "LLeg3",
    "LLegAnkle",
    "RLeg1",
    "RLeg3",
    "RLegAnkle",
    "SpineLowerSpine",
    "SpineUpperSpine",
    "LArm_Upper",
    "LArm_Forearm",
    "LArm_Palm",
    "Neck",
    "HEAD",
    "RArm_Upper",
    "RArm_Forearm",
    "RArm_Palm",
    "Tail1",
    "Tail2",
    "Tail3",
    "Tail4",
];

/// The deer / elk ragdoll's bodies in NIF `bhkRigidBody` order (#6069).
pub(crate) const DEER_RAGDOLL_BODIES: [&str; 24] = [
    "Elk_COM",
    "ElkSpine1",
    "ElkSpine3",
    "ElkLHumerus",
    "ElkLRadius",
    "ElkLMetacarpus",
    "ElkLFrontHoof",
    "ElkRHumerus",
    "ElkRRadius",
    "ElkRMetacarpus",
    "ElkRFrontHoof",
    "ElkNeck1",
    "ElkNeck3",
    "ElkNeck4",
    "ElkScullBase",
    "ElkScull",
    "ElkLFemur",
    "ElkLTibia",
    "ElkLLargeCannon",
    "ElkLRearHoof",
    "ElkRFemur",
    "ElkRTibia",
    "ElkRLargeCannon",
    "ElkRRearHoof",
];

/// Every Skyrim body set whose `XRGD` entry order is verified (entry `k`
/// poses body `k`); see the module docs for each rig's evidence.
const SKYRIM_POSABLE_RAGDOLLS: [&[&str]; 6] = [
    &HUMANOID_RAGDOLL_BODIES,
    &DRAUGR_RAGDOLL_BODIES,
    &SKELETON_S_RAGDOLL_BODIES,
    &FALMER_RAGDOLL_BODIES,
    &SKEEVER_RAGDOLL_BODIES,
    &DEER_RAGDOLL_BODIES,
];

/// The FO3 / FNV humanoid ragdoll's bodies in the skeleton NIF's
/// `bhkRigidBody` order, which the `XRGD` entries follow (#6252, see the
/// module docs).
pub(crate) const FALLOUT_HUMANOID_RAGDOLL_BODIES: [&str; 18] = [
    "Bip01 NonAccum",
    "Bip01 L Thigh",
    "Bip01 L Calf",
    "Bip01 L Foot",
    "Bip01 R Thigh",
    "Bip01 R Calf",
    "Bip01 R Foot",
    "Bip01 Spine",
    "Bip01 Spine1",
    "Bip01 Spine2",
    "Bip01 Neck1",
    "Bip01 Head",
    "Bip01 L UpperArm",
    "Bip01 L Forearm",
    "Bip01 L Hand",
    "Bip01 R UpperArm",
    "Bip01 R Forearm",
    "Bip01 R Hand",
];

/// #6490 — Oblivion's humanoid ragdoll (`meshes\characters\_male\skeleton.nif`)
/// as the importer extracts it: 18 bodies like FO3/FNV's, but spine-first,
/// without `Bip01 Neck1` and with `SideWeapon`, so it matches no verified set
/// and [`pose_skeleton`] refuses it. NOT a verified set — no `XRGD` order has
/// been established for it (Oblivion.esm's 125 posed dead-base humanoid
/// corpses author 18 entries 110 times, 22 entries 13 times, 17 twice). Kept
/// so a test pins that the rig gate, not a game check, keeps it out.
#[cfg(test)]
pub(crate) const OBLIVION_HUMANOID_RAGDOLL_BODIES: [&str; 18] = [
    "Bip01 NonAccum",
    "Bip01 Spine",
    "Bip01 Spine1",
    "Bip01 Spine2",
    "Bip01 Head",
    "Bip01 L UpperArm",
    "Bip01 L Forearm",
    "Bip01 L Hand",
    "Bip01 R UpperArm",
    "Bip01 R Forearm",
    "Bip01 R Hand",
    "Bip01 L Thigh",
    "Bip01 L Calf",
    "Bip01 L Foot",
    "Bip01 R Thigh",
    "Bip01 R Calf",
    "Bip01 R Foot",
    "SideWeapon",
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

/// The FO4 super mutant ragdoll's bodies in blob (body-id) order, by bone name
/// (#6065): 17 bodies, no `SPINE2`. Verified by `scripts/xrgd-fo4-rigs.py` —
/// see the module docs.
pub(crate) const FO4_SUPERMUTANT_RAGDOLL_BODIES: [&str; 17] = [
    "COM",
    "SPINE1",
    "LLeg_Thigh",
    "RLeg_Thigh",
    "Chest",
    "LLeg_Calf",
    "RLeg_Calf",
    "LArm_UpperArm",
    "Neck",
    "RArm_UpperArm",
    "LLeg_Foot",
    "RLeg_Foot",
    "LArm_ForeArm1",
    "HEAD",
    "RArm_ForeArm1",
    "LArm_Hand",
    "RArm_Hand",
];

// ── FO4 creature rigs whose entries follow the skeleton's node order (#6065) ──────────────────────────
//
// Their bodies author bone id 0 almost everywhere (a radroach 23 of 25, a stingwing 31 of 31), so the bone-id
// claim ([`entries_by_bone_id`]) degenerates to the blob order, which is not the entries' order. Entry k poses the
// k-th ragdoll body bone in the skeleton's own depth-first order ([`entries_by_tree_order`]); on every measured
// bone the entry's offset length then equals the NIF bone's (`scripts/xrgd-fo4-rigs.py`, `tree` mapping).
// Each list is the body set in blob (template) order, by bone name.

/// Brahmin: 39 bodies.
pub(crate) const FO4_BRAHMIN_RAGDOLL_BODIES: [&str; 39] = [
    "Pelvis", "RLeg1", "LLeg1", "Tail1", "SPINE2", "Sack", "RLeg2", "LLeg2", "Tail2", "Spine4", "RLeg3", "LLeg3",
    "Tail3", "HubSpine", "RLegToe", "LLegToe", "Tail4", "RNeck1", "LArm1", "LNeck1", "RArm1", "RNeck2", "LArm2",
    "LNeck2", "RArm2", "RNeck3", "LArm3", "LNeck3", "RArm3", "RHead", "LArmPalm", "LHead", "RArmPalm", "RREar",
    "RJaw", "RLEar", "LJaw", "LREar", "LLEar",
];

/// Rad stag: 31 bodies, including the four phantom-leg bodies.
pub(crate) const FO4_RADSTAG_RAGDOLL_BODIES: [&str; 31] = [
    "COM", "RLeg_Rear_Thigh", "LLeg_Rear_Thigh", "SPINE1", "RLeg_Rear_Knee", "LLeg_Rear_Knee", "SPINE2",
    "RLeg_Rear_Ankle", "LLeg_Rear_Ankle", "Spine3", "Ribcage", "LPhantomLeg1", "RPhantomLeg1", "RLeg_Front_Thigh",
    "LLeg_Front_Thigh", "LNeck1", "RNeck1", "LPhantomLeg2", "RPhantomLeg2", "RLeg_Front_Knee", "LLeg_Front_Knee",
    "LNeck2", "RNeck2", "RLeg_Front_Ankle", "LLeg_Front_Ankle", "LNeck3", "RNeck3", "LNeck4", "RNeck4", "LNeck5",
    "RNeck5",
];

/// Yao guai: 18 bodies.
pub(crate) const FO4_YAOGUAI_RAGDOLL_BODIES: [&str; 18] = [
    "COM", "RLeg_Rear_Thigh", "SPINE1", "LLeg_Rear_Thigh", "RLeg_Rear_Knee", "Spine3", "LLeg_Rear_Knee",
    "RLeg_Rear_Ankle", "Ribcage", "LLeg_Rear_Ankle", "RLeg_Front_Thigh", "LLeg_Front_Thigh", "Neck1",
    "RLeg_Front_Knee", "LLeg_Front_Knee", "HEAD", "RLeg_Front_Ankle", "LLeg_Front_Ankle",
];

/// Mirelurk (the common one): 23 bodies.
pub(crate) const FO4_MIRELURK_RAGDOLL_BODIES: [&str; 23] = [
    "Pelvis", "RFrontLegThigh", "LFrontLegThigh", "SPINE1", "LBackLegThigh", "RBackLegThigh", "RFrontLegCalf",
    "LFrontLegCalf", "SPINE2", "LBackLegCalf", "RBackLegCalf", "RFrontLegFoot", "LFrontLegFoot", "Rib",
    "LBackLegfoot", "RBackLegFoot", "HEAD", "LArmUpper", "RArmUpper", "LArmFore", "RArmFore", "LArmPalm",
    "RArmPalm",
];

/// Rad roach: 25 bodies (the root is the skeleton's own misspelt `Pelivis`).
pub(crate) const FO4_RADROACH_RAGDOLL_BODIES: [&str; 25] = [
    "Pelivis", "2RLeg1", "2LLeg1", "SPINE1", "RadRoachTail1", "2RLeg2", "2LLeg2", "RWing", "LWing", "1LLeg1",
    "1RLeg1", "Nick", "0RArm1", "0LArm1", "2RLeg3", "2LLeg3", "1LLeg2", "1RLeg2", "HEAD", "0RArm2", "0LArm2",
    "1LLeg3", "1RLeg3", "0RArm3", "0LArm3",
];

/// Molerat: 20 bodies. Admitted on exact offsets (17/17) although its flatness rank is weak (7th of 24).
pub(crate) const FO4_MOLERAT_RAGDOLL_BODIES: [&str; 20] = [
    "Pelvis", "Back_LLeg1", "Tail1", "SPINE1", "Back_RLeg1", "Back_LLeg2", "Tail2", "Spine3", "Back_RLeg2",
    "Back_LLegAnkle", "Tail3", "Neck", "Front_RLeg1", "Front_LLeg1", "Back_RLegAnkle", "HEAD", "Front_RLeg2",
    "Front_LLeg2", "Front_RLegAnkle", "Front_LLegAnkle",
];

/// Vicious dog / Dogmeat: 24 bodies. Admitted on exact offsets (21/21) although its flatness rank is weak (11th).
pub(crate) const FO4_DOG_RAGDOLL_BODIES: [&str; 24] = [
    "Dogmeat_COM", "Dogmeat_Rear_RLeg1", "Dogmeat_Tail1", "Dogmeat_Spine1", "Dogmeat_Rear_LLeg1",
    "Dogmeat_Rear_RLeg2", "Dogmeat_Tail3", "Dogmeat_Ribcage", "Dogmeat_Rear_LLeg2", "Dogmeat_Rear_RLegAnkle",
    "Dogmeat_Tail5", "Dogmeat_Neck1", "Dogmeat_Front_RLeg1", "Dogmeat_Front_LLeg1", "Dogmeat_Rear_LLegAnkle",
    "Dogmeat_Rear_RLegToe1", "Dogmeat_Head", "Dogmeat_Front_RLeg2", "Dogmeat_Front_LLeg2",
    "Dogmeat_Rear_LLegToe1", "Dogmeat_Front_RLegAnkle", "Dogmeat_Front_LLegAnkle", "Dogmeat_Front_RLegToe1",
    "Dogmeat_Front_LLegToe1",
];

/// Stingwing: 31 bodies. Admitted on exact offsets (30/30) although its flatness rank is weak (11th) and only three
/// corpses author it.
pub(crate) const FO4_STINGWING_RAGDOLL_BODIES: [&str; 31] = [
    "Torso", "R RearWing", "Tail1", "HEAD", "R LArm1", "L FrontWing", "R FrontWing", "M LArm1", "R RArm1",
    "F RArm1", "M RArm1", "F LArm1", "L RearWing", "Tail2", "R LArm2", "M LArm2", "R RArm2", "F RArm2",
    "M RArm2", "F LArm2", "Tail3", "R LArmPalm", "M LArmPalm", "R RArmPalm", "F RArmPalm", "M RArmPalm",
    "F LArmPalm", "Tail4", "Tail5", "Tail6", "Tail7",
];

/// The FO4 creature rigs posed by node order.
const FO4_TREE_ORDERED_RAGDOLLS: [&[&str]; 8] = [
    &FO4_BRAHMIN_RAGDOLL_BODIES,
    &FO4_RADSTAG_RAGDOLL_BODIES,
    &FO4_YAOGUAI_RAGDOLL_BODIES,
    &FO4_MIRELURK_RAGDOLL_BODIES,
    &FO4_RADROACH_RAGDOLL_BODIES,
    &FO4_MOLERAT_RAGDOLL_BODIES,
    &FO4_DOG_RAGDOLL_BODIES,
    &FO4_STINGWING_RAGDOLL_BODIES,
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
/// rotation (Y-up), and for the root body (the COM) a local translation that
/// puts it at the actor's placement — zero where its parents sit at the
/// placement origin (Skyrim, FO4), the inverse of `Bip01`'s hip height on
/// FO3/FNV (#6252).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BonePose {
    pub(crate) rotation: Quat,
    pub(crate) at_placement: bool,
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
                at_placement: k == 0,
            })
        })
        .collect()
}

/// Whether a ragdoll's bodies, by bone name in template order, are exactly
/// `set` (case-insensitive, like the template's own name resolution).
fn is_body_set(body_bone_names: &[Option<String>], set: &[&str]) -> bool {
    body_bone_names.len() == set.len()
        && body_bone_names
            .iter()
            .zip(set)
            .all(|(name, expected)| name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(expected)))
}

/// Whether a ragdoll's bodies, by bone name in template order, are exactly one
/// of the verified Skyrim sets (humanoid, draugr, SkeletonS, falmer, skeever,
/// deer), whose `XRGD` entries pose them by order.
pub(crate) fn is_skyrim_posable_ragdoll(body_bone_names: &[Option<String>]) -> bool {
    SKYRIM_POSABLE_RAGDOLLS
        .into_iter()
        .any(|set| is_body_set(body_bone_names, set))
}

/// Whether a ragdoll's bodies, by bone name in template order, are the FO3 /
/// FNV humanoid set, whose `XRGD` entries pose them by order (#6252).
pub(crate) fn is_fallout_posable_ragdoll(body_bone_names: &[Option<String>]) -> bool {
    is_body_set(body_bone_names, &FALLOUT_HUMANOID_RAGDOLL_BODIES)
}

/// Whether a spawn parks a placed actor's `XRGD` pose for its death
/// reconciliation: the actor is a corpse by its placement (`Starts Dead`,
/// Skyrim+) or by its base record. Which base marks a corpse is decided per
/// game at the record boundary — Oblivion's header bit (#5013), FO3/FNV's
/// DATA Base Health ≤ 0 (#5005); no other game sets it — so a pose on a live
/// FO3/FNV base (Fisto, `VHDKimballRangerAmbushed01`) is not parked (#6252).
///
/// Whether the pose can be *applied* is the rig's question, not the game's
/// (#6490): [`pose_skeleton`] admits only the verified body sets and refuses
/// the rest, and a refused pose falls back to the collapse. Oblivion's
/// humanoid rig ([`OBLIVION_HUMANOID_RAGDOLL_BODIES`]) is not one of them, so
/// its 125 posed corpses park and are refused until that rig is verified.
pub(crate) fn parks_authored_pose(
    placement_starts_dead: bool,
    base_dead: bool,
    pose: &[RagdollPoseBone],
) -> bool {
    !pose.is_empty() && (placement_starts_dead || base_dead)
}

/// Whether a ragdoll's bodies, by bone name in template order, are exactly one
/// of the verified FO4 sets ([`FO4_HUMAN_RAGDOLL_BODIES`],
/// [`FO4_GHOUL_RAGDOLL_BODIES`], [`FO4_SUPERMUTANT_RAGDOLL_BODIES`]).
pub(crate) fn is_fo4_posable_ragdoll(body_bone_names: &[Option<String>]) -> bool {
    let sets: [&[&str]; 3] = [
        &FO4_HUMAN_RAGDOLL_BODIES,
        &FO4_GHOUL_RAGDOLL_BODIES,
        &FO4_SUPERMUTANT_RAGDOLL_BODIES,
    ];
    sets.into_iter().any(|set| is_body_set(body_bone_names, set))
}

/// Whether a ragdoll's bodies, by bone name in template order, are exactly one of the FO4 creature sets whose
/// entries pose the bodies in the skeleton's node order ([`entries_by_tree_order`], #6065).
pub(crate) fn is_fo4_tree_ordered_ragdoll(body_bone_names: &[Option<String>]) -> bool {
    FO4_TREE_ORDERED_RAGDOLLS.into_iter().any(|set| is_body_set(body_bone_names, set))
}

/// FO4 creature rigs: for each ragdoll body (by index), the `XRGD` entry that poses it, when entry k poses the
/// k-th body bone in the skeleton's depth-first order. `ranks[b]` is body `b`'s position in that order
/// ([`body_tree_ranks`]). The entries must be exactly the bodies (a trailing marker of the [`body_entries`] family
/// aside, the one `scripts/xrgd-fo4-rigs.py` strips) and the first one must
/// pose body 0, the root the placement is applied to; anything else is refused.
pub(crate) fn entries_by_tree_order(
    entries: &[RagdollPoseBone],
    ranks: &[usize],
) -> Result<Vec<usize>, String> {
    let bodies = body_entries(entries);
    if bodies.len() != ranks.len() {
        return Err(format!(
            "{} pose entries for {} ragdoll bodies - the entry set does not cover this skeleton",
            bodies.len(),
            ranks.len()
        ));
    }
    let mut seen = vec![false; ranks.len()];
    for &rank in ranks {
        match seen.get_mut(rank) {
            Some(slot) if !*slot => *slot = true,
            _ => return Err("the ragdoll bodies' node order is not a permutation".to_owned()),
        }
    }
    if ranks.first() != Some(&0) {
        return Err("the first ragdoll body is not the first body in the skeleton's node order".to_owned());
    }
    Ok(ranks.to_vec())
}

/// Each ragdoll body's position among the body bones in the skeleton's depth-first order from `skeleton_root`,
/// children in their stored order (the NIF's `children` order, which is also the order the importer spawns them in).
/// The core `collect_subtree` visits children in reverse, so it cannot be used for this.
pub(crate) fn body_tree_ranks(
    world: &World,
    skeleton_root: EntityId,
    bones: &[EntityId],
) -> Result<Vec<usize>, String> {
    let body_of: std::collections::HashMap<EntityId, usize> =
        bones.iter().enumerate().map(|(b, &bone)| (bone, b)).collect();
    let mut ranks = vec![usize::MAX; bones.len()];
    let mut visited = std::collections::HashSet::new();
    let mut next = 0usize;
    // The visited set and the budget are the project's hierarchy-walk discipline (#5513): a cyclic or duplicated
    // `Children` graph from a corrupt save yields a finite answer instead of a hang.
    let mut guard = byroredux_core::ecs::HierarchyTraversalGuard::new(world.next_entity_id() as usize, 0);
    let mut stack = vec![skeleton_root];
    while let Some(entity) = stack.pop() {
        if !visited.insert(entity) {
            continue;
        }
        if !guard.step() {
            return Err("the skeleton's hierarchy traversal budget ran out (a cyclic Children graph?)".to_owned());
        }
        if let Some(&body) = body_of.get(&entity) {
            ranks[body] = next;
            next += 1;
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.0.iter().rev().copied());
        }
    }
    if next != bones.len() {
        return Err(format!(
            "only {next} of {} ragdoll bones are in the skeleton's hierarchy",
            bones.len()
        ));
    }
    Ok(ranks)
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
                at_placement: k == 0,
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
    let poses = if is_skyrim_posable_ragdoll(&names) || is_fallout_posable_ragdoll(&names) {
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
    } else if is_fo4_tree_ordered_ragdoll(&names) {
        let ranks = body_tree_ranks(world, skeleton_root, &bones)?;
        let mapping = entries_by_tree_order(entries, &ranks)?;
        bone_poses_mapped(entries, &mapping)?
    } else {
        return Err(
            "the ragdoll is not a body set the XRGD entry order is established for".to_owned(),
        );
    };

    // Resolved before the write: the root body's parents are not ragdoll
    // bodies, so the pose leaves their globals as they are.
    let placements: Vec<Option<Vec3>> = bones
        .iter()
        .zip(&poses)
        .map(|(&bone, pose)| pose.at_placement.then(|| placement_local(world, skeleton_root, bone)))
        .collect();
    {
        let mut transforms = world
            .query_mut::<Transform>()
            .ok_or("Transform storage not registered")?;
        for ((&bone, pose), placement) in bones.iter().zip(&poses).zip(placements) {
            let Some(local) = transforms.get_mut(bone) else {
                return Err(format!("ragdoll bone {bone} has no Transform"));
            };
            local.rotation = pose.rotation;
            if let Some(translation) = placement {
                local.translation = translation;
            }
        }
    }
    recompute_globals(world, skeleton_root);
    Ok(bones.len())
}

/// The local translation that puts `bone` at `skeleton_root`'s position: the
/// root's world position in the bone's parent frame. Zero when the parents sit
/// at the placement origin (the Skyrim and FO4 COM); without the globals to
/// read, zero as before (#6252).
fn placement_local(world: &World, skeleton_root: EntityId, bone: EntityId) -> Vec3 {
    let parent = world.get::<Parent>(bone).map(|parent| parent.0);
    let (Some(parent), Some(globals)) = (parent, world.query::<GlobalTransform>()) else {
        return Vec3::ZERO;
    };
    let (Some(parent), Some(root)) = (globals.get(parent), globals.get(skeleton_root)) else {
        return Vec3::ZERO;
    };
    if parent.scale.abs() < f32::EPSILON {
        return Vec3::ZERO;
    }
    parent.rotation.inverse() * (root.translation - parent.translation) / parent.scale
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
