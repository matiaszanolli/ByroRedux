//! #5015 — the `XRGD` pose: entry→body mapping, rotation convention and the
//! skeleton pose that precedes ragdoll activation.

use super::*;
use byroredux_core::ecs::{Children, World};

fn entry(position: [f32; 3], rotation: [f32; 3]) -> RagdollPoseBone {
    RagdollPoseBone {
        bone_id: 0,
        position,
        rotation,
    }
}

/// The convention the vanilla corpus selects: in Z-up the product
/// `Rx(-rx)·Ry(-ry)·Rz(-rz)`, converted to the engine's Y-up frame by the
/// basis change `C = Rx(-90°)` (`(x, y, z) → (x, z, -y)`).
fn expected_yup(rx: f32, ry: f32, rz: f32) -> Quat {
    let zup = Quat::from_rotation_x(-rx) * Quat::from_rotation_y(-ry) * Quat::from_rotation_z(-rz);
    let c = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    c * zup * c.inverse()
}

fn same_rotation(a: Quat, b: Quat) -> bool {
    a.dot(b).abs() > 1.0 - 1.0e-5
}

#[test]
fn rotation_is_the_gamebryo_xyz_product_with_negated_angles() {
    // Sample angles from a vanilla corpse (C04DeadKodlak's pose entries).
    for rotation in [
        [-1.48, -0.453, 0.048],
        [3.132, 0.258, -2.978],
        [0.538, -0.487, -0.152],
        [1.0, -0.128, 0.24],
        [0.0, 0.0, 0.0],
    ] {
        let poses = bone_poses(&[entry([0.0; 3], rotation)], 1).unwrap();
        assert!(
            same_rotation(poses[0].rotation, expected_yup(rotation[0], rotation[1], rotation[2])),
            "{rotation:?}"
        );
    }
}

/// XRGD and REFR share one convention: since the REFR ship mode went back to
/// Gamebryo's CW + XYZ product (mode 0, 2026-10-16), the pose rotation equals
/// the REFR placement rotation. The order still matters, and a pose with two
/// non-zero angles tells it apart from the ZYX product (diagnostic mode 1).
#[test]
fn rotation_is_the_gamebryo_xyz_product_not_zyx() {
    let rotation = [0.7, 0.4, 0.0];
    let xrgd = bone_poses(&[entry([0.0; 3], rotation)], 1).unwrap()[0].rotation;
    let [rx, ry, rz] = rotation;
    let refr_ship = euler_zup_to_quat_yup_mode(byroredux_core::math::coord::REFR_ROTATION_MODE_SHIP, rx, ry, rz);
    assert!(same_rotation(xrgd, refr_ship));
    let zyx = euler_zup_to_quat_yup_mode(1, rx, ry, rz);
    assert!(!same_rotation(xrgd, zyx));
}

#[test]
fn only_the_root_entry_zeroes_its_translation() {
    let entries = vec![entry([0.0; 3], [0.1, 0.2, 0.3]); 4];
    let poses = bone_poses(&entries, 4).unwrap();
    assert_eq!(
        poses.iter().map(|p| p.zero_translation).collect::<Vec<_>>(),
        [true, false, false, false]
    );
}

/// An entry count that does not match the ragdoll's body count is refused:
/// the entry order is established for one skeleton shape only.
#[test]
fn a_mismatched_entry_count_is_refused() {
    let entries = vec![entry([0.0; 3], [0.0; 3]); 18];
    assert!(bone_poses(&entries, 18).is_ok());
    assert!(bone_poses(&entries, 17).is_err());
    assert!(bone_poses(&entries[..5], 18).is_err());
    assert!(bone_poses(&[], 0).is_ok());
}

/// Some poses end with a marker entry. Set aside, the counts line up
/// (humanoid 18 + 1, beast races 23 + 1); an entry that merely has that
/// position with a rotation is a real bone.
#[test]
fn a_trailing_marker_entry_is_set_aside() {
    let marker = entry([0.0, 17.0, 10.0], [0.0; 3]);
    let mut entries = vec![entry([0.0; 3], [0.1, 0.2, 0.3]); 18];
    assert_eq!(body_entries(&entries).len(), 18);
    entries.push(marker);
    assert_eq!(body_entries(&entries).len(), 18);
    assert!(bone_poses(&entries, 18).is_ok());
    assert!(bone_poses(&entries, 19).is_err(), "the marker is not a body");
    // Only a LAST entry is the marker; a rotated one is a bone.
    let rotated = entry([0.0, 17.0, 10.0], [0.0, 0.0, 0.5]);
    assert_eq!(body_entries(&[rotated]).len(), 1);
    assert_eq!(body_entries(&[marker, entry([0.0; 3], [0.0; 3])]).len(), 2);
    assert!(body_entries(&[]).is_empty());
}

/// #5015 — the markers are a family, not one constant: the draugr skeleton's
/// `(32.45, 104.06)` (with the `3e-6` of `x` noise and `-0.0` rotations the
/// vanilla data carries) and the `(34, 10)` one are set aside the same way,
/// while a bone that merely resembles one is not.
#[test]
fn the_marker_family_is_set_aside_but_real_bones_are_not() {
    let body = entry([0.0; 3], [0.1, 0.2, 0.3]);
    let with = |last: RagdollPoseBone| {
        let mut entries = vec![body; 18];
        entries.push(last);
        body_entries(&entries).len()
    };
    let skeleton = entry([3.2241403e-6, 32.45313, 104.06167], [-0.0, -0.0, -1.0e-8]);
    assert_eq!(with(skeleton), 18, "draugr skeleton marker");
    assert_eq!(with(entry([0.0, 34.0, 10.0], [0.0; 3])), 18, "(34, 10) marker");
    assert_eq!(with(entry([0.0; 3], [0.0; 3])), 19, "an all-zero last entry may be a body at rest");
    assert_eq!(with(entry([0.5, 17.0, 10.0], [0.0; 3])), 19, "x is not 0");
    assert_eq!(with(entry([0.0, 17.0, 10.0], [0.0, 0.01, 0.0])), 19, "rotated");
    let other_bone = RagdollPoseBone { bone_id: 7, ..entry([0.0, 17.0, 10.0], [0.0; 3]) };
    assert_eq!(with(other_bone), 19, "bone_id is not 0");
}

#[test]
fn non_finite_entries_are_refused() {
    let bad = [entry([0.0; 3], [f32::NAN, 0.0, 0.0]), entry([f32::INFINITY, 0.0, 0.0], [0.0; 3])];
    assert!(bone_poses(&bad[..1], 1).is_err());
    assert!(bone_poses(&bad[1..], 1).is_err());
}

/// The humanoid ragdoll's 18 bodies as a chain under a placed root — COM,
/// then each following body parented to the one before it. Posing sets the
/// local rotations, zeroes the COM's local translation, and recomposes the
/// globals the activation will read.
fn humanoid_chain(world: &mut World) -> (EntityId, Vec<EntityId>) {
    bone_chain(world, &HUMANOID_RAGDOLL_BODIES, &[None; 18])
}

/// [`humanoid_chain`] for any body set: `names[i]` is body `i`'s bone and
/// `ids[i]` its FO4 bone id.
fn bone_chain(world: &mut World, names: &[&str], ids: &[Option<u8>]) -> (EntityId, Vec<EntityId>) {
    use crate::ragdoll::{RagdollTemplate, RagdollTemplateBody};
    use byroredux_core::ecs::Name;
    use byroredux_core::string::StringPool;
    world.register::<Transform>();
    world.register::<GlobalTransform>();
    world.register::<Parent>();
    world.register::<Children>();
    world.register::<Name>();
    world.register::<RagdollTemplate>();
    world.insert_resource(StringPool::new());

    let root = world.spawn();
    world.insert(root, Transform::new(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 1.0));
    world.insert(root, GlobalTransform::new(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 1.0));
    let mut bones = Vec::new();
    let mut parent = root;
    for (i, name) in names.iter().enumerate() {
        let bone = world.spawn();
        let translation = if i == 0 { Vec3::new(0.0, 69.0, 0.0) } else { Vec3::new(0.0, 0.0, 6.0) };
        world.insert(bone, Transform::new(translation, Quat::IDENTITY, 1.0));
        world.insert(bone, GlobalTransform::IDENTITY);
        world.insert(bone, Parent(parent));
        let symbol = world.resource_mut::<StringPool>().intern(name);
        world.insert(bone, Name(symbol));
        world.insert(parent, Children(vec![bone]));
        bones.push(bone);
        parent = bone;
    }
    let body = |(bone, pose_bone_id): (EntityId, Option<u8>)| RagdollTemplateBody {
        bone,
        local_translation: Vec3::ZERO,
        local_rotation: Quat::IDENTITY,
        shape: byroredux_core::ecs::components::CollisionShape::Ball { radius: 1.0 },
        mass: 1.0,
        linear_damping: 0.0,
        angular_damping: 0.0,
        friction: 0.5,
        restitution: 0.0,
        pose_bone_id,
    };
    world.insert(
        root,
        RagdollTemplate {
            bodies: bones.iter().copied().zip(ids.iter().copied()).map(body).collect(),
            constraints: Vec::new(),
        },
    );
    (root, bones)
}

fn dead_pose_world(rename_body: Option<usize>) -> (World, EntityId, EntityId) {
    let mut world = World::new();
    let (root, bones) = humanoid_chain(&mut world);
    if let Some(index) = rename_body {
        let symbol = world
            .resource_mut::<byroredux_core::string::StringPool>()
            .intern("Wolf Tail01");
        world.insert(bones[index], byroredux_core::ecs::Name(symbol));
    }
    world.register::<byroredux_core::ecs::components::Dead>();
    world.register::<crate::components::AnimationTarget>();
    world.register::<byroredux_physics::Ragdoll>();
    world.register::<crate::ragdoll::RagdollActive>();
    world.insert_resource(byroredux_physics::PhysicsWorld::new());
    world.insert(
        root,
        crate::components::AnimationTarget {
            skeleton_root: root,
            consumed_idle_serial: 0,
        },
    );
    world.insert(root, byroredux_core::ecs::components::Dead);
    let mut entries = vec![entry([0.0; 3], [0.0; 3]); 18];
    entries[0] = entry([0.0; 3], [-std::f32::consts::FRAC_PI_2, 0.0, 0.0]);
    world.insert(
        root,
        crate::ragdoll_pose::AuthoredRagdollPose(entries),
    );
    (world, root, bones[0])
}

#[test]
fn posing_sets_local_rotations_and_recomposes_globals() {
    let mut world = World::new();
    let (root, bones) = humanoid_chain(&mut world);
    let (com, thigh, calf) = (bones[0], bones[1], bones[2]);

    // Lie the COM down 90° about Z-up X (pitch), leave the limbs at rest.
    let lie_down = [-std::f32::consts::FRAC_PI_2, 0.0, 0.0];
    let mut entries = vec![entry([0.0; 3], [0.0; 3]); 18];
    entries[0] = entry([0.0; 3], lie_down);
    // A trailing marker entry rides along and is ignored.
    entries.push(entry([0.0, 17.0, 10.0], [0.0; 3]));
    assert_eq!(pose_skeleton(&world, root, &entries), Ok(18));

    let local = |e| *world.get::<Transform>(e).unwrap();
    assert_eq!(local(com).translation, Vec3::ZERO, "the COM sits at the placement");
    assert_eq!(local(thigh).translation, Vec3::new(0.0, 0.0, 6.0), "other bone lengths are kept");
    assert!(same_rotation(local(com).rotation, expected_yup(lie_down[0], 0.0, 0.0)));
    assert!(same_rotation(local(thigh).rotation, Quat::IDENTITY));

    // Globals follow: the COM at the root, the thigh offset rotated by the COM.
    let global = |e| *world.get::<GlobalTransform>(e).unwrap();
    assert!((global(com).translation - Vec3::new(10.0, 0.0, 0.0)).length() < 1.0e-4);
    let expected_thigh = Vec3::new(10.0, 0.0, 0.0) + local(com).rotation * Vec3::new(0.0, 0.0, 6.0);
    assert!((global(thigh).translation - expected_thigh).length() < 1.0e-4);
    assert!(same_rotation(global(thigh).rotation, local(com).rotation));
    assert!(same_rotation(global(calf).rotation, local(com).rotation));
}

/// A ragdoll that is not the humanoid body set is refused, untouched — the
/// entry order is established for the humanoid skeleton only.
#[test]
fn a_non_humanoid_ragdoll_is_not_posed() {
    let mut world = World::new();
    let (root, bones) = humanoid_chain(&mut world);
    // Rename one body: the same count, a different skeleton.
    let symbol = world.resource_mut::<byroredux_core::string::StringPool>().intern("Wolf Tail01");
    world.insert(bones[5], byroredux_core::ecs::Name(symbol));
    let entries = vec![entry([0.0; 3], [0.5, 0.0, 0.0]); 18];
    let before = world.get::<Transform>(bones[1]).unwrap().rotation;
    assert!(pose_skeleton(&world, root, &entries).is_err());
    assert_eq!(world.get::<Transform>(bones[1]).unwrap().rotation, before);
}

#[test]
fn death_reconciliation_applies_and_consumes_the_authored_xrgd_pose() {
    let (world, actor, com) = dead_pose_world(None);

    let note = crate::combat::reconcile_dead_actor(&world, actor);

    assert!(note.starts_with("; posed 18 bones from XRGD"), "{note}");
    assert_eq!(world.get::<Transform>(com).unwrap().translation, Vec3::ZERO);
    assert!(world.get::<crate::ragdoll_pose::AuthoredRagdollPose>(actor).is_none());
    assert!(world.get::<crate::ragdoll::RagdollActive>(actor).is_some());
}

#[test]
fn death_reconciliation_consumes_a_refused_pose_and_still_activates() {
    let (world, actor, com) = dead_pose_world(Some(5));

    let note = crate::combat::reconcile_dead_actor(&world, actor);

    assert!(note.starts_with("; XRGD pose not applied"), "{note}");
    assert_eq!(
        world.get::<Transform>(com).unwrap().translation,
        Vec3::new(0.0, 69.0, 0.0),
        "refusal leaves the spawn pose for ordinary ragdoll activation"
    );
    assert!(world.get::<crate::ragdoll_pose::AuthoredRagdollPose>(actor).is_none());
    assert!(world.get::<crate::ragdoll::RagdollActive>(actor).is_some());
}

#[test]
fn starts_dead_spawn_parks_xrgd_before_queuing_reconciliation() {
    let source = include_str!("cell_loader/references/mod.rs");
    let start = source.find("pub(super) fn load_references_budgeted").unwrap();
    let loader = &source[start..];
    let gate = loader
        .find("if placed_ref.starts_dead && !placed_ref.ragdoll_pose.is_empty()")
        .expect("XRGD pose parking stays gated on Starts Dead and a non-empty pose");
    let body = &loader[gate..];
    let insert = body
        .find("AuthoredRagdollPose(")
        .expect("pose is parked on the actor root");
    let reconcile = body
        .find("apply_starts_dead(world, root)")
        .expect("dead reconciliation is queued");
    assert!(insert < reconcile, "the pose must be parked before death reconciliation");
}

#[test]
fn reconciliation_poses_before_ragdoll_activation() {
    let source = include_str!("combat.rs");
    let start = source.find("pub(crate) fn reconcile_dead_actor").unwrap();
    let reconcile = &source[start..source[start..].find("\n}\n").unwrap() + start];
    let pose = reconcile
        .find("pose_skeleton(world, skeleton_root, &pose.0)")
        .expect("the parked XRGD pose is applied");
    let activate = reconcile
        .find("match crate::ragdoll::activate_ragdoll(world, skeleton_root)")
        .expect("death reconciliation activates the ragdoll");
    assert!(pose < activate, "the XRGD pose must be applied before activation seeds bodies");
}

#[test]
fn the_humanoid_body_set_is_matched_by_name_in_order() {
    let names: Vec<Option<String>> = HUMANOID_RAGDOLL_BODIES.iter().map(|n| Some(n.to_string())).collect();
    assert!(is_skyrim_posable_ragdoll(&names));
    let lower: Vec<Option<String>> = HUMANOID_RAGDOLL_BODIES.iter().map(|n| Some(n.to_ascii_lowercase())).collect();
    assert!(is_skyrim_posable_ragdoll(&lower), "case-insensitive");
    let mut swapped = names.clone();
    swapped.swap(1, 4);
    assert!(!is_skyrim_posable_ragdoll(&swapped), "order matters");
    assert!(!is_skyrim_posable_ragdoll(&names[..17]));
    let mut unnamed = names;
    unnamed[3] = None;
    assert!(!is_skyrim_posable_ragdoll(&unnamed));
}

// ---- Other Skyrim rigs (#6068, #6069) --------------------------------------

fn owned_names(set: &[&str]) -> Vec<Option<String>> {
    set.iter().map(|n| Some(n.to_string())).collect()
}

/// Each verified Skyrim rig is recognised only as its full, ordered body set:
/// a truncated or renamed set, an unverified rig (the canine bodies the wolf
/// and dog share) and the FO4 sets are not.
#[test]
fn the_verified_skyrim_body_sets_are_recognised_by_name_only_in_full() {
    let sets: [&[&str]; 6] = [
        &HUMANOID_RAGDOLL_BODIES,
        &DRAUGR_RAGDOLL_BODIES,
        &SKELETON_S_RAGDOLL_BODIES,
        &FALMER_RAGDOLL_BODIES,
        &SKEEVER_RAGDOLL_BODIES,
        &DEER_RAGDOLL_BODIES,
    ];
    for set in sets {
        let names = owned_names(set);
        assert!(is_skyrim_posable_ragdoll(&names), "{:?}", set[1]);
        assert!(!is_skyrim_posable_ragdoll(&names[..names.len() - 1]), "{:?} truncated", set[1]);
        let mut renamed = names.clone();
        renamed[2] = Some("Canine_Tail1".to_owned());
        assert!(!is_skyrim_posable_ragdoll(&renamed), "{:?} renamed", set[1]);
        assert!(!is_fo4_posable_ragdoll(&names), "{:?} is not an FO4 set", set[1]);
    }
    // The draugr rig is the humanoid's but for the left foot's `[LLft ]`, which
    // kept it off the humanoid gate (#6068).
    let differing: Vec<usize> = (0..18)
        .filter(|&i| HUMANOID_RAGDOLL_BODIES[i] != DRAUGR_RAGDOLL_BODIES[i])
        .collect();
    assert_eq!(differing, [3]);
    // SkeletonS is the draugr rig with the jaw after the head.
    let mut with_jaw = DRAUGR_RAGDOLL_BODIES.to_vec();
    with_jaw.insert(12, "NPC Head [Jaw]");
    assert_eq!(with_jaw, SKELETON_S_RAGDOLL_BODIES);
    // The canine rig (wolf, dog) and the FO4 human are not verified Skyrim sets.
    let canine = [
        "Canine_COM", "Canine_Spine1", "Canine_Spine3", "Canine_LFrontLeg1", "Canine_LFrontLeg2",
        "Canine_LFrontLegPalm", "Canine_RFrontLeg1", "Canine_RFrontLeg2", "Canine_RFrontLegPalm",
        "Canine_Neck2", "Canine_Head", "Canine_LBackLeg1", "Canine_LBackLeg2", "Canine_LBackLegPalm",
        "Canine_LBackLegToe", "Canine_RBackLeg1", "Canine_RBackLeg2", "Canine_RBackLegPalm",
        "Canine_RBackLegToe", "Canine_Tail1", "Canine_Tail2", "Canine_Tail3",
    ];
    assert!(!is_skyrim_posable_ragdoll(&owned_names(&canine)));
    assert!(!is_skyrim_posable_ragdoll(&owned_names(&FO4_HUMAN_RAGDOLL_BODIES)));
}

/// Entries carrying rotation `k * 0.01` about Z-up Z, so each posed bone names
/// the entry that posed it.
fn ordered_entries(count: usize) -> Vec<RagdollPoseBone> {
    (0..count).map(|k| entry([0.0, 0.0, 1.0], [0.0, 0.0, 0.01 * (k + 1) as f32])).collect()
}

/// A rig on a verified Skyrim body set is posed by order: entry `k` poses body
/// `k`, a trailing marker aside.
#[test]
fn the_verified_skyrim_creature_rigs_are_posed_by_order() {
    for (set, marker) in [
        (&DRAUGR_RAGDOLL_BODIES[..], None),
        (&SKELETON_S_RAGDOLL_BODIES[..], Some([0.0, 28.71, 108.18])),
        (&FALMER_RAGDOLL_BODIES[..], None),
        (&SKEEVER_RAGDOLL_BODIES[..], None),
        (&DEER_RAGDOLL_BODIES[..], Some([0.0, 32.45, 104.06])),
    ] {
        let mut world = World::new();
        let (root, bones) = bone_chain(&mut world, set, &vec![None; set.len()]);
        let mut entries = ordered_entries(set.len());
        if let Some(position) = marker {
            entries.push(entry(position, [0.0; 3]));
        }
        assert_eq!(pose_skeleton(&world, root, &entries), Ok(set.len()), "{:?}", set[1]);
        let rotation = |bone| world.get::<Transform>(bone).unwrap().rotation;
        for (k, &bone) in bones.iter().enumerate() {
            assert!(same_rotation(rotation(bone), expected_yup(0.0, 0.0, 0.01 * (k + 1) as f32)));
        }
        assert_eq!(world.get::<Transform>(bones[0]).unwrap().translation, Vec3::ZERO);
    }
}

/// #6068 — SkeletonS poses only its 19-entry poses. The 18-entry ones (two
/// families only the unused `bone_id` tells apart: without the jaw, or without
/// the neck) and the 20-entry ones (a constant extra entry no body explains)
/// are refused untouched.
#[test]
fn a_skeleton_s_pose_without_the_jaw_or_with_an_extra_entry_is_refused() {
    let mut world = World::new();
    let (root, bones) = bone_chain(&mut world, &SKELETON_S_RAGDOLL_BODIES, &[None; 19]);
    let before = world.get::<Transform>(bones[12]).unwrap().rotation;
    let mut extra = ordered_entries(19);
    extra.push(entry([0.0, -8.258, 3.28], [-std::f32::consts::FRAC_PI_2, 0.0, 0.0]));
    for entries in [ordered_entries(18), extra] {
        assert!(pose_skeleton(&world, root, &entries).is_err(), "{} entries", entries.len());
        assert_eq!(world.get::<Transform>(bones[12]).unwrap().rotation, before);
    }
}

#[test]
fn posing_without_a_template_or_with_a_wrong_count_changes_nothing() {
    let mut world = World::new();
    world.register::<Transform>();
    world.register::<GlobalTransform>();
    world.register::<Parent>();
    world.register::<Children>();
    world.register::<crate::ragdoll::RagdollTemplate>();
    let root = world.spawn();
    world.insert(root, Transform::IDENTITY);
    world.insert(root, GlobalTransform::IDENTITY);
    let entries = vec![entry([0.0; 3], [0.5, 0.0, 0.0]); 18];
    assert!(pose_skeleton(&world, root, &entries).is_err());
    assert_eq!(world.get::<Transform>(root).unwrap().rotation, Quat::IDENTITY);
}

/// #5015 — the entry↔body claim against the real corpus: every Starts-Dead
/// ACHR of `Skyrim.esm` that authors an `XRGD` pose, grouped by the skeleton
/// its race loads. The humanoid skeleton (the one the mapping was derived
/// from) must line up entry-for-body for the large majority of its corpses;
/// the rest are reported — a mismatch is exactly what the production count
/// gate refuses. #6068 / #6069: the corpses the production gate poses (a
/// verified body set, an entry count equal to its body count) are pinned per
/// skeleton, and every other skeleton poses none.
///
///   cargo test -p byroredux --bin byroredux xrgd_entry_counts -- --ignored --nocapture
#[test]
#[ignore = "needs Skyrim SE game data on disk"]
fn xrgd_entry_counts_match_the_humanoid_ragdoll_bodies() {
    use std::collections::BTreeMap;
    let data = std::env::var("BYROREDUX_SKYRIMSE_DATA")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data".into()
        });
    let (esm, meshes) = (data.join("Skyrim.esm"), data.join("Skyrim - Meshes0.bsa"));
    if !esm.is_file() || !meshes.is_file() {
        assert!(
            std::env::var("BYROREDUX_REQUIRE_GAME_DATA").map_or(true, |v| v == "0"),
            "BYROREDUX_REQUIRE_GAME_DATA is set, but {esm:?} / {meshes:?} are missing"
        );
        return;
    }
    let index = byroredux_plugin::esm::records::parse_esm(&std::fs::read(&esm).unwrap()).unwrap();
    let archive = byroredux_bsa::BsaArchive::open(&meshes).unwrap();
    // skeleton → (ragdoll body count, on a verified Skyrim body set)
    let mut bodies_of: std::collections::HashMap<String, Option<(usize, bool)>> = Default::default();
    // skeleton → (corpses, entry count == body count, distinct entry counts, posed)
    type Tally = (usize, usize, std::collections::BTreeSet<usize>, usize);
    let mut tally: BTreeMap<String, Tally> = BTreeMap::new();

    let mut visit = |r: &byroredux_plugin::esm::cell::PlacedRef| {
        if !r.starts_dead || r.ragdoll_pose.is_empty() {
            return;
        }
        let Some(npc) = index.npcs.get(&r.base_form_id) else { return };
        let gender = usize::from(
            byroredux_plugin::equip::Gender::from_acbs_flags(npc.acbs_flags)
                == byroredux_plugin::equip::Gender::Female,
        );
        let Some(path) = index
            .races
            .get(&npc.race_form_id)
            .map(|race| race.skeleton_models[gender].clone())
            .filter(|p| !p.is_empty())
        else {
            return;
        };
        let path = crate::asset_provider::normalize_mesh_path(&path).to_ascii_lowercase();
        let bodies = *bodies_of.entry(path.clone()).or_insert_with(|| {
            let name = archive.list_files().into_iter().find(|f| f.to_ascii_lowercase() == path)?;
            let scene = byroredux_nif::parse_nif(&archive.extract(name).ok()?).ok()?;
            byroredux_nif::import::collision::extract_ragdoll(&scene).map(|r| {
                let names: Vec<Option<String>> =
                    r.bodies.iter().map(|b| Some(b.bone_name.to_string())).collect();
                (r.bodies.len(), is_skyrim_posable_ragdoll(&names))
            })
        });
        let entry = tally.entry(path).or_default();
        entry.0 += 1;
        let body_entries = body_entries(&r.ragdoll_pose);
        let matching = bodies.is_some_and(|(count, _)| count == body_entries.len());
        entry.1 += usize::from(matching);
        entry.2.insert(body_entries.len());
        entry.3 += usize::from(matching && bodies.is_some_and(|(_, posable)| posable));
    };
    for cell in index.cells.cells.values() {
        cell.references.iter().for_each(&mut visit);
    }
    for cells in index.cells.exterior_cells.values() {
        for cell in cells.values() {
            cell.references.iter().for_each(&mut visit);
        }
    }
    for (skeleton, (corpses, matching, counts, posed)) in &tally {
        eprintln!(
            "{skeleton}: {corpses} corpses, {matching} entry-count == body-count, entry counts {counts:?}, \
             (bodies, verified set) {:?}, posed {posed}",
            bodies_of[skeleton]
        );
    }
    let humanoid = &tally["meshes\\actors\\character\\character assets\\skeleton.nif"];
    let (all, all_matching) = tally.values().fold((0, 0), |(a, m), t| (a + t.0, m + t.1));
    eprintln!("overall: {all_matching} of {all} corpses have an entry count equal to their ragdoll's bodies");
    assert!(humanoid.0 > 250, "the humanoid corpses were found");
    // Measured 2026-10-19: 273 of 300 (262 before the marker family was set
    // aside, #5015). The 27 that differ authored a body set other than the
    // skeleton their race loads (23 entries on the humanoid rig); the
    // production gate refuses those rather than guess.
    assert!(
        humanoid.1 * 100 >= humanoid.0 * 90,
        "at least 90% of humanoid corpses line up entry-for-body, got {} of {}",
        humanoid.1,
        humanoid.0
    );
    // The draugr rig shares the humanoid body set; 165 of its 168 corpses
    // line up once the `(32.45, 104.06)` marker is set aside (78 before).
    let draugr = &tally["meshes\\actors\\draugr\\character assets\\skeleton.nif"];
    assert!(
        draugr.1 * 100 >= draugr.0 * 95,
        "the draugr skeleton's corpses line up entry-for-body, got {} of {}",
        draugr.1,
        draugr.0
    );
    // #6068 / #6069 — the posed corpses per verified skeleton, measured
    // 2026-10-20 (`scripts/xrgd-skyrim-rigs.py` carries each rig's evidence).
    // The draugr rigs were posed by none before #6068: their left foot is
    // `[LLft ]`, not the humanoid's `[Lft ]`.
    let posed: Vec<(&str, usize)> = tally
        .iter()
        .filter(|(_, t)| t.3 > 0)
        .map(|(skeleton, t)| (skeleton.as_str(), t.3))
        .collect();
    assert_eq!(
        posed,
        [
            ("meshes\\actors\\character\\character assets female\\skeleton_female.nif", 100),
            ("meshes\\actors\\character\\character assets\\skeleton.nif", 273),
            ("meshes\\actors\\deer\\character assets\\skeleton.nif", 30),
            ("meshes\\actors\\draugr\\character assets\\skeleton.nif", 165),
            ("meshes\\actors\\draugr\\character assets\\skeletonf.nif", 35),
            ("meshes\\actors\\draugr\\character assets\\skeletons.nif", 53),
            ("meshes\\actors\\falmer\\character assets\\skeleton.nif", 20),
            ("meshes\\actors\\skeever\\character assets\\skeleton.nif", 54),
        ],
        "the corpses posed per skeleton; every other skeleton poses none"
    );
}

// ---- Fallout 4: entries named by bone id (#5015) ---------------------------

/// The human rig's per-body bone ids (blob body order) and a corpse's entry
/// ids, both read off the vanilla data: COM and SPINE1 share bone 2.
const FO4_HUMAN_BODY_IDS: [u8; 18] = [2, 8, 14, 2, 9, 15, 3, 10, 16, 4, 5, 0, 11, 6, 1, 12, 7, 13];
const FO4_HUMAN_ENTRY_IDS: [u8; 18] = [2, 8, 9, 10, 14, 15, 16, 2, 3, 4, 5, 6, 7, 0, 1, 11, 12, 13];
/// The same for the feral ghoul.
const FO4_GHOUL_BODY_IDS: [u8; 18] = [2, 14, 2, 8, 15, 3, 9, 16, 4, 10, 11, 0, 5, 12, 1, 6, 13, 7];
const FO4_GHOUL_ENTRY_IDS: [u8; 18] = [2, 8, 9, 10, 14, 15, 16, 2, 3, 4, 0, 1, 5, 6, 7, 11, 12, 13];

/// The super mutant (#6065): 17 bodies, the entry ids a vanilla corpse authors.
const FO4_SUPERMUTANT_BODY_IDS: [u8; 17] = [2, 2, 8, 14, 3, 9, 15, 5, 0, 11, 10, 16, 6, 1, 12, 7, 13];
const FO4_SUPERMUTANT_ENTRY_IDS: [u8; 17] = [2, 14, 15, 16, 8, 9, 10, 2, 3, 11, 12, 13, 0, 1, 5, 6, 7];

fn id_entry(bone_id: u8, rotation: [f32; 3]) -> RagdollPoseBone {
    RagdollPoseBone {
        bone_id,
        position: [1.0, 2.0, 3.0],
        rotation,
    }
}

fn some_ids(ids: &[u8]) -> Vec<Option<u8>> {
    ids.iter().copied().map(Some).collect()
}

/// An entry claims the first unclaimed body carrying its id, so the human
/// rig's entry order (COM, L leg, R leg, spine, L arm, neck, head, R arm)
/// lands on the right blob bodies even though bone 2 appears twice.
#[test]
fn fo4_entries_map_to_bodies_by_bone_id() {
    let entries: Vec<_> = FO4_HUMAN_ENTRY_IDS.iter().map(|&id| id_entry(id, [0.0; 3])).collect();
    let mapping = entries_by_bone_id(&entries, &some_ids(&FO4_HUMAN_BODY_IDS)).unwrap();
    // Body 0 COM <- entry 0, body 3 SPINE1 (also bone 2) <- entry 7, body 1
    // L thigh <- entry 1, body 4 L calf <- entry 2, body 11 Neck <- entry 13 ...
    assert_eq!(mapping[0], 0);
    assert_eq!(mapping[3], 7);
    assert_eq!(mapping[1], 1);
    assert_eq!(mapping[4], 2);
    assert_eq!(mapping[11], 13);
    assert_eq!(mapping[14], 14, "head");
    assert_eq!(mapping[17], 17, "R hand");
    let mut seen = mapping.clone();
    seen.sort_unstable();
    assert_eq!(seen, (0..18).collect::<Vec<_>>(), "every entry poses exactly one body");

    // The ghoul orders its entries differently and resolves through the same ids.
    let entries: Vec<_> = FO4_GHOUL_ENTRY_IDS.iter().map(|&id| id_entry(id, [0.0; 3])).collect();
    let mapping = entries_by_bone_id(&entries, &some_ids(&FO4_GHOUL_BODY_IDS)).unwrap();
    assert_eq!(mapping[0], 0, "COM");
    assert_eq!(mapping[2], 7, "SpineRoot (bone 2) takes the second bone-2 entry");
    assert_eq!(mapping[3], 1, "LTHIGH");
    assert_eq!(mapping[11], 10, "Neck");
    assert_eq!(mapping[14], 11, "HEAD");
}

#[test]
fn fo4_pose_accepts_one_trailing_marker_and_refuses_anything_else_unmatched() {
    let ids = some_ids(&FO4_HUMAN_BODY_IDS);
    let mut entries: Vec<_> = FO4_HUMAN_ENTRY_IDS.iter().map(|&id| id_entry(id, [0.1; 3])).collect();
    // The FO4 marker: bone id 0, an offset, zero rotation — last, unmatched.
    let marker = RagdollPoseBone {
        bone_id: 0,
        position: [-0.61, 25.3, 0.0],
        rotation: [0.0; 3],
    };
    entries.push(marker);
    assert!(entries_by_bone_id(&entries, &ids).is_ok());
    // A rotated leftover is a stray entry, not a marker.
    entries[18].rotation = [0.0, 0.0, 0.5];
    assert!(entries_by_bone_id(&entries, &ids).is_err());
    // A leftover that is not last is refused too.
    entries[18].rotation = [0.0; 3];
    entries.swap(17, 18);
    assert!(entries_by_bone_id(&entries, &ids).is_err());
    // A body no entry names.
    let short: Vec<_> = FO4_HUMAN_ENTRY_IDS[..17].iter().map(|&id| id_entry(id, [0.0; 3])).collect();
    assert!(entries_by_bone_id(&short, &ids).is_err());
    // Bodies without ids cannot be matched.
    assert!(entries_by_bone_id(&short, &[None; 18]).is_err());
}

fn fo4_pose_world(names: &[&str], body_ids: &[u8]) -> (World, EntityId, Vec<EntityId>) {
    let mut world = World::new();
    let (root, bones) = bone_chain(&mut world, names, &some_ids(body_ids));
    (world, root, bones)
}

/// An FO4 human corpse is posed by id: each ragdoll bone takes the rotation of
/// the entry that names its bone, the COM's translation is zeroed.
#[test]
fn an_fo4_human_corpse_is_posed_by_bone_id() {
    let (world, root, bones) = fo4_pose_world(&FO4_HUMAN_RAGDOLL_BODIES, &FO4_HUMAN_BODY_IDS);
    // Entry k carries rotation k*0.01 about Z-up Z, so each bone's result names
    // the entry that posed it.
    let mut entries: Vec<_> = FO4_HUMAN_ENTRY_IDS
        .iter()
        .enumerate()
        .map(|(k, &id)| id_entry(id, [0.0, 0.0, 0.01 * (k + 1) as f32]))
        .collect();
    entries.push(RagdollPoseBone { bone_id: 0, position: [-0.61, 25.3, 0.0], rotation: [0.0; 3] });
    assert_eq!(pose_skeleton(&world, root, &entries), Ok(18));
    let rotation = |bone| world.get::<Transform>(bone).unwrap().rotation;
    // SPINE1 is body 3, bone 2, the SECOND bone-2 entry (entry 7).
    assert!(same_rotation(rotation(bones[3]), expected_yup(0.0, 0.0, 0.01 * 8.0)));
    // L calf is body 4, entry 2.
    assert!(same_rotation(rotation(bones[4]), expected_yup(0.0, 0.0, 0.01 * 3.0)));
    // HEAD is body 14, entry 14.
    assert!(same_rotation(rotation(bones[14]), expected_yup(0.0, 0.0, 0.01 * 15.0)));
    assert_eq!(world.get::<Transform>(bones[0]).unwrap().translation, Vec3::ZERO, "COM at the placement");
}

#[test]
fn an_fo4_ghoul_corpse_is_posed_by_bone_id() {
    let (world, root, bones) = fo4_pose_world(&FO4_GHOUL_RAGDOLL_BODIES, &FO4_GHOUL_BODY_IDS);
    let entries: Vec<_> = FO4_GHOUL_ENTRY_IDS
        .iter()
        .enumerate()
        .map(|(k, &id)| id_entry(id, [0.0, 0.0, 0.01 * (k + 1) as f32]))
        .collect();
    assert_eq!(pose_skeleton(&world, root, &entries), Ok(18));
    let rotation = |bone| world.get::<Transform>(bone).unwrap().rotation;
    // The ghoul's head (body 14) is the entry named bone 1, entry 11.
    assert!(same_rotation(rotation(bones[14]), expected_yup(0.0, 0.0, 0.01 * 12.0)));
    // LTHIGH (body 3) is entry 1.
    assert!(same_rotation(rotation(bones[3]), expected_yup(0.0, 0.0, 0.01 * 2.0)));
}

/// The super mutant resolves its 17 entries through the same ids: the COM and
/// `SPINE1` share bone 2, and the entries run COM, R leg … rather than the
/// human's order.
#[test]
fn an_fo4_super_mutant_corpse_is_posed_by_bone_id() {
    let (world, root, bones) = fo4_pose_world(&FO4_SUPERMUTANT_RAGDOLL_BODIES, &FO4_SUPERMUTANT_BODY_IDS);
    let entries: Vec<_> = FO4_SUPERMUTANT_ENTRY_IDS
        .iter()
        .enumerate()
        .map(|(k, &id)| id_entry(id, [0.0, 0.0, 0.01 * (k + 1) as f32]))
        .collect();
    assert_eq!(pose_skeleton(&world, root, &entries), Ok(17));
    let rotation = |bone| world.get::<Transform>(bone).unwrap().rotation;
    // SPINE1 (body 1) is the SECOND bone-2 entry, entry 7.
    assert!(same_rotation(rotation(bones[1]), expected_yup(0.0, 0.0, 0.01 * 8.0)));
    // The head (body 13) is the entry named bone 1, entry 13.
    assert!(same_rotation(rotation(bones[13]), expected_yup(0.0, 0.0, 0.01 * 14.0)));
    // LLeg_Thigh (body 2) is entry 4.
    assert!(same_rotation(rotation(bones[2]), expected_yup(0.0, 0.0, 0.01 * 5.0)));
    assert_eq!(world.get::<Transform>(bones[0]).unwrap().translation, Vec3::ZERO, "COM at the placement");
}

/// An FO4 ragdoll whose names are the verified set but whose bodies carry no
/// ids (a classic-path template), or whose names are another skeleton's, is
/// refused untouched.
#[test]
fn an_fo4_pose_without_ids_or_on_another_skeleton_is_refused() {
    let entries: Vec<_> = FO4_HUMAN_ENTRY_IDS.iter().map(|&id| id_entry(id, [0.3; 3])).collect();

    let mut world = World::new();
    let (root, bones) = bone_chain(&mut world, &FO4_HUMAN_RAGDOLL_BODIES, &[None; 18]);
    let before = world.get::<Transform>(bones[1]).unwrap().rotation;
    assert!(pose_skeleton(&world, root, &entries).is_err());
    assert_eq!(world.get::<Transform>(bones[1]).unwrap().rotation, before);

    let mut other: Vec<&str> = FO4_HUMAN_RAGDOLL_BODIES.to_vec();
    other[5] = "Brahmin_Tail01";
    let (world, root, _) = fo4_pose_world(&other, &FO4_HUMAN_BODY_IDS);
    assert!(pose_skeleton(&world, root, &entries).is_err());
}

#[test]
fn the_fo4_body_sets_are_recognised_by_name_only_in_full() {
    let names = |set: &[&str]| set.iter().map(|n| Some(n.to_string())).collect::<Vec<_>>();
    assert!(is_fo4_posable_ragdoll(&names(&FO4_HUMAN_RAGDOLL_BODIES)));
    assert!(is_fo4_posable_ragdoll(&names(&FO4_GHOUL_RAGDOLL_BODIES)));
    assert!(is_fo4_posable_ragdoll(&names(&FO4_SUPERMUTANT_RAGDOLL_BODIES)));
    assert!(!is_fo4_posable_ragdoll(&names(&FO4_SUPERMUTANT_RAGDOLL_BODIES)[..16]));
    assert!(!is_fo4_posable_ragdoll(&names(&HUMANOID_RAGDOLL_BODIES)), "the Skyrim set is not an FO4 set");
    assert!(!is_fo4_posable_ragdoll(&names(&FO4_HUMAN_RAGDOLL_BODIES)[..17]));
}

/// #5015 / #6065 — the FO4 pose claim against the real corpus: every Starts-Dead
/// ACHR of `Fallout4.esm` that authors an `XRGD` pose on the human, feral-ghoul or
/// super-mutant skeleton resolves through the bone-id mapping, and the rig the
/// corpse's race loads is one of the three verified body sets.
///
///   cargo test -p byroredux --bin byroredux fo4_xrgd_poses -- --ignored --nocapture
#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn fo4_xrgd_poses_resolve_against_the_real_skeletons() {
    use std::collections::HashMap;
    let data = std::env::var("BYROREDUX_FO4_DATA")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data".into());
    let (esm, meshes) = (data.join("Fallout4.esm"), data.join("Fallout4 - Meshes.ba2"));
    if !esm.is_file() || !meshes.is_file() {
        assert!(
            std::env::var("BYROREDUX_REQUIRE_GAME_DATA").map_or(true, |v| v == "0"),
            "BYROREDUX_REQUIRE_GAME_DATA is set, but {esm:?} / {meshes:?} are missing"
        );
        return;
    }
    let index = byroredux_plugin::esm::records::parse_esm(&std::fs::read(&esm).unwrap()).unwrap();
    let archive = byroredux_bsa::Ba2Archive::open(&meshes).unwrap();
    // Race → skeleton NIF for the three verified rigs.
    let skeleton_of = |race: &str| match race {
        "HumanRace" | "SynthGen1Race" | "SynthGen2Race" | "GhoulRace" => {
            Some("meshes\\actors\\character\\characterassets\\skeleton.nif")
        }
        "FeralGhoulRace" | "FeralGhoulGlowingRace" => {
            Some("meshes\\actors\\feralghoul\\characterassets\\skeleton.nif")
        }
        "SuperMutantRace" => Some("meshes\\actors\\supermutant\\characterassets\\skeleton.nif"),
        _ => None,
    };
    // Skeleton → (host bone names, bone ids) in template order.
    /// Host bone names and bone ids of a skeleton's ragdoll bodies, in template order.
    type Rig = (Vec<Option<String>>, Vec<Option<u8>>);
    let mut rigs: HashMap<&str, Rig> = HashMap::new();
    let (mut corpses, mut posable, mut mapped) = (0usize, 0usize, 0usize);
    let mut failures: Vec<String> = Vec::new();
    let mut visit = |r: &byroredux_plugin::esm::cell::PlacedRef| {
        if !r.starts_dead || r.ragdoll_pose.is_empty() {
            return;
        }
        let Some(npc) = index.npcs.get(&r.base_form_id) else { return };
        let Some(race) = index.races.get(&npc.race_form_id) else { return };
        let Some(path) = skeleton_of(&race.editor_id) else { return };
        let (names, ids) = rigs.entry(path).or_insert_with(|| {
            let bytes = archive.extract(path).expect("the skeleton NIF");
            let scene = byroredux_nif::parse_nif(&bytes).unwrap();
            let ragdoll = byroredux_nif::import::collision::extract_ragdoll(&scene).expect("a ragdoll");
            (
                ragdoll.bodies.iter().map(|b| Some(b.bone_name.to_string())).collect(),
                ragdoll.bodies.iter().map(|b| b.pose_bone_id).collect(),
            )
        });
        corpses += 1;
        posable += usize::from(is_fo4_posable_ragdoll(names));
        match entries_by_bone_id(&r.ragdoll_pose, ids) {
            Ok(_) => mapped += 1,
            Err(e) => failures.push(format!("{:08x} ({}): {e}", r.form_id, race.editor_id)),
        }
    };
    for cell in index.cells.cells.values() {
        cell.references.iter().for_each(&mut visit);
    }
    for cells in index.cells.exterior_cells.values() {
        for cell in cells.values() {
            cell.references.iter().for_each(&mut visit);
        }
    }
    eprintln!("[fo4] {corpses} human/ghoul/super-mutant-skeleton corpses, {posable} on a verified body set, {mapped} resolve by bone id");
    for f in failures.iter().take(10) {
        eprintln!("  refused: {f}");
    }
    assert!(corpses > 900, "the human, ghoul and super-mutant corpses were found, got {corpses}");
    assert_eq!(posable, corpses, "all three rigs are on the verified body sets");
    // Measured 2026-10-19: 972 of 987 on the human and ghoul rigs (98.5 %); the 7
    // super mutants all resolve. The rest are ghouls with two unmatched trailing
    // entries (20 in all) and a few poses that omit a body.
    assert!(mapped * 100 >= corpses * 95, "at least 95% resolve, got {mapped} of {corpses}");
}
