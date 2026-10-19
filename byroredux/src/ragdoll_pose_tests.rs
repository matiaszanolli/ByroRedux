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

/// The order matters: this convention is not the REFR `ZYX` product, and a
/// pose with two non-zero angles tells them apart.
#[test]
fn rotation_is_not_the_refr_zyx_product() {
    let rotation = [0.7, 0.4, 0.0];
    let xrgd = bone_poses(&[entry([0.0; 3], rotation)], 1).unwrap()[0].rotation;
    let refr = euler_zup_to_quat_yup_mode(1, rotation[0], rotation[1], rotation[2]);
    assert!(!same_rotation(xrgd, refr));
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
    for (i, name) in HUMANOID_RAGDOLL_BODIES.iter().enumerate() {
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
    let body = |bone| RagdollTemplateBody {
        bone,
        local_translation: Vec3::ZERO,
        local_rotation: Quat::IDENTITY,
        shape: byroredux_core::ecs::components::CollisionShape::Ball { radius: 1.0 },
        mass: 1.0,
        linear_damping: 0.0,
        angular_damping: 0.0,
        friction: 0.5,
        restitution: 0.0,
    };
    world.insert(
        root,
        RagdollTemplate {
            bodies: bones.iter().map(|&b| body(b)).collect(),
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
    assert!(is_humanoid_ragdoll(&names));
    let lower: Vec<Option<String>> = HUMANOID_RAGDOLL_BODIES.iter().map(|n| Some(n.to_ascii_lowercase())).collect();
    assert!(is_humanoid_ragdoll(&lower), "case-insensitive");
    let mut swapped = names.clone();
    swapped.swap(1, 4);
    assert!(!is_humanoid_ragdoll(&swapped), "order matters");
    assert!(!is_humanoid_ragdoll(&names[..17]));
    let mut unnamed = names;
    unnamed[3] = None;
    assert!(!is_humanoid_ragdoll(&unnamed));
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
/// gate refuses.
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
    let mut bodies_of: std::collections::HashMap<String, Option<usize>> = Default::default();
    // skeleton → (corpses, entry count == body count, distinct entry counts)
    let mut tally: BTreeMap<String, (usize, usize, std::collections::BTreeSet<usize>)> = BTreeMap::new();

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
            byroredux_nif::import::collision::extract_ragdoll(&scene).map(|r| r.bodies.len())
        });
        let entry = tally.entry(path).or_default();
        entry.0 += 1;
        let body_entries = body_entries(&r.ragdoll_pose);
        entry.1 += usize::from(bodies == Some(body_entries.len()));
        entry.2.insert(body_entries.len());
    };
    for cell in index.cells.cells.values() {
        cell.references.iter().for_each(&mut visit);
    }
    for cells in index.cells.exterior_cells.values() {
        for cell in cells.values() {
            cell.references.iter().for_each(&mut visit);
        }
    }
    for (skeleton, (corpses, matching, counts)) in &tally {
        eprintln!(
            "{skeleton}: {corpses} corpses, {matching} entry-count == body-count, entry counts {counts:?}, bodies {:?}",
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
}
