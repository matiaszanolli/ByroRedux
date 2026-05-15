//! Unit tests for the debug-console command dispatcher. Extracted
//! from `commands.rs` to keep the production code under ~1400 lines;
//! pulled in via `#[cfg(test)] #[path = "..."] mod tests;`.

use super::*;
use byroredux_core::ecs::components::{GlobalTransform, Name, SkinnedMesh, Transform};
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};

/// `skin.dump` regression for #841 — the dump must surface the
/// resolved bone entity, its `Name`, the per-bone `bind_inverse`
/// translation, and the composed palette translation in the
/// summary table. Identity-fallback bones (no GT for the bone
/// entity) print `(no GT)` and `palette: identity` so the
/// `SKIN_DROPOUT_DUMPED` slots are obvious in the dump.
#[test]
fn skin_dump_renders_resolved_bone_with_world_and_palette() {
    let mut world = World::new();
    let mut pool = StringPool::new();
    let bip01 = pool.intern("Bip01 Spine");
    world.insert_resource(pool);

    // Bone entity: positioned at world (0, 5, 0), with a Name.
    let bone = world.spawn();
    world.insert(bone, Transform::from_translation(Vec3::new(0.0, 5.0, 0.0)));
    world.insert(
        bone,
        GlobalTransform::new(Vec3::new(0.0, 5.0, 0.0), Quat::IDENTITY, 1.0),
    );
    world.insert(bone, Name(bip01));

    // Skinned mesh: one bone, bind_inverse cancels the bind-pose
    // translation so palette = world * bind_inv = identity (the
    // canonical "bone hasn't moved relative to bind" case).
    let bind_inv = Mat4::from_translation(Vec3::new(0.0, -5.0, 0.0));
    let skin_entity = world.spawn();
    let skin = SkinnedMesh::new_with_global(Some(bone), vec![Some(bone)], vec![bind_inv], Mat4::IDENTITY);
    let lines = format_skin_dump(&world, skin_entity, &skin);
    let dump = lines.join("\n");

    // Header — dump is for the right entity and bone count.
    assert!(
        dump.contains(&format!("dump for entity {} (1 bones)", skin_entity)),
        "header missing or wrong: {}",
        dump
    );
    // Bone slot 0 row: shows resolved bone entity + Name +
    // world_t (0, 5, 0) + bind_inv_t (0, -5, 0) + palette_t (0, 0, 0).
    assert!(dump.contains("bip01 spine"), "Name missing: {}", dump);
    assert!(
        dump.contains("(0.00,5.00,0.00)"),
        "world translation missing: {}",
        dump
    );
    assert!(
        dump.contains("(0.00,-5.00,0.00)"),
        "bind_inv translation missing: {}",
        dump
    );
    // palette = T(0,5,0) * T(0,-5,0) = identity → translation (0,0,0).
    assert!(
        dump.contains("(0.00,0.00,0.00)"),
        "palette translation missing: {}",
        dump
    );
}

#[test]
fn skin_dump_marks_unresolved_bone_slots() {
    // Phase 1b.x DROPOUT scenario — a bone slot that didn't
    // resolve to an entity must show up as `(None)` /
    // `(unresolved)` so the operator can correlate against the
    // SKIN_DROPOUT_DUMPED warn.
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    let skin_entity = world.spawn();
    let skin = SkinnedMesh::new_with_global(None, vec![None], vec![Mat4::IDENTITY], Mat4::IDENTITY);
    let lines = format_skin_dump(&world, skin_entity, &skin);
    let dump = lines.join("\n");
    assert!(
        dump.contains("(None)"),
        "unresolved entity missing: {}",
        dump
    );
    assert!(
        dump.contains("(unresolved)"),
        "unresolved name missing: {}",
        dump
    );
}

#[test]
fn skin_dump_reports_non_identity_global_skin_transform() {
    // A non-identity `global_skin_transform` is informational
    // (not multiplied at runtime) but its presence is exactly
    // the kind of authoring quirk #841 surfaced on Doc Mitchell;
    // the dump must call it out so it isn't missed.
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    let skin_entity = world.spawn();
    let global = Mat4::from_quat(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
    let skin = SkinnedMesh::new_with_global(None, vec![], vec![], global);
    let lines = format_skin_dump(&world, skin_entity, &skin);
    let dump = lines.join("\n");
    assert!(
        dump.contains("global_skin_transform: NON-IDENTITY"),
        "non-identity global must be flagged: {}",
        dump
    );
}

/// `light.dump` smoke test — exercises both the "no resource"
/// branches (the cold start before any cell is loaded) and the
/// populated branches, so the command is callable from `byro-dbg`
/// at any point in the engine's lifetime without panicking.
#[test]
fn light_dump_handles_missing_and_present_resources() {
    use crate::components::{CellLightingRes, GameTimeRes, SkyParamsRes};

    // Cold start — no resources inserted yet.
    let mut world = World::new();
    let cmd = LightDumpCommand;
    let lines = cmd.execute(&world, "").lines;
    let joined = lines.join("\n");
    assert!(
        joined.contains("CellLightingRes: <not present"),
        "cold start should flag CellLightingRes absence: {}",
        joined
    );
    assert!(
        joined.contains("SkyParamsRes: <not present"),
        "cold start should flag SkyParamsRes absence: {}",
        joined
    );
    assert!(
        joined.contains("GameTimeRes: <not present>"),
        "cold start should flag GameTimeRes absence: {}",
        joined
    );

    // Populated — Markarth-procedural-fallback-shaped values, so
    // the test pins the format the Markarth investigation actually
    // reads.
    world.insert_resource(CellLightingRes {
        ambient: [0.15, 0.14, 0.12],
        directional_color: [1.0, 0.95, 0.8],
        directional_dir: [-0.4, 0.8, -0.45],
        is_interior: false,
        fog_color: [0.65, 0.7, 0.8],
        fog_near: 15000.0,
        fog_far: 80000.0,
        directional_fade: None,
        fog_clip: None,
        fog_power: None,
        fog_far_color: None,
        fog_max: None,
        light_fade_begin: None,
        light_fade_end: None,
        directional_ambient: None,
        specular_color: None,
        specular_alpha: None,
        fresnel_power: None,
    });
    world.insert_resource(SkyParamsRes {
        zenith_color: [0.15, 0.3, 0.65],
        horizon_color: [0.55, 0.5, 0.42],
        lower_color: [0.165, 0.15, 0.126],
        sun_direction: [-0.4, 0.8, -0.45],
        sun_color: [1.0, 0.95, 0.8],
        sun_size: 0.9995,
        sun_intensity: 4.0,
        sun_angular_radius: 0.020,
        is_exterior: true,
        cloud_tile_scale: 0.0,
        cloud_texture_index: 0,
        sun_texture_index: 0,
        cloud_tile_scale_1: 0.0,
        cloud_texture_index_1: 0,
        cloud_tile_scale_2: 0.0,
        cloud_texture_index_2: 0,
        cloud_tile_scale_3: 0.0,
        cloud_texture_index_3: 0,
        current_dalc_cube: None,
    });
    world.insert_resource(GameTimeRes {
        hour: 10.5,
        time_scale: 30.0,
    });

    let lines = cmd.execute(&world, "").lines;
    let joined = lines.join("\n");
    // Ambient + sun_intensity are the two key numbers the Markarth
    // probe needs to read — pin both so output drift breaks the test.
    assert!(
        joined.contains("ambient            = [0.150, 0.140, 0.120]"),
        "ambient must print 3-decimal float triple: {}",
        joined
    );
    assert!(
        joined.contains("sun_intensity      = 4.000"),
        "sun_intensity must print 3-decimal float: {}",
        joined
    );
    // GameTime wall-clock conversion — 10.5 should print "10:30 AM".
    assert!(
        joined.contains("10:30 AM"),
        "GameTimeRes hour=10.5 should print '10:30 AM': {}",
        joined
    );
    // is_exterior true + sun_texture_index 0 should annotate
    // "procedural disc fallback" so a missing CLMT FNAM is obvious.
    assert!(
        joined.contains("procedural disc fallback"),
        "sun_texture_index=0 must annotate procedural fallback: {}",
        joined
    );
}

/// `look_at_yaw_pitch` must produce a (yaw, pitch) pair such that
/// composing the fly-camera quaternion (`Q_y(yaw) * Q_x(pitch)`)
/// applied to `-Z` yields the unit direction from `from` to `to`.
/// Tests pick directions on the six cardinal axes — full coverage
/// of the yaw/pitch sign convention.
fn forward_from(yaw: f32, pitch: f32) -> Vec3 {
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
    rot * (-Vec3::Z)
}

fn assert_forward_matches(from: Vec3, to: Vec3) {
    let (yaw, pitch) = look_at_yaw_pitch(from, to);
    let want = (to - from).normalize();
    let got = forward_from(yaw, pitch);
    assert!(
        (got - want).length() < 1e-3,
        "look_at_yaw_pitch({from:?} -> {to:?}) yielded forward {got:?}, want {want:?}",
    );
}

#[test]
fn look_at_minus_z_is_identity_rotation() {
    // Default fly-camera forward is -Z; looking at -Z from origin
    // must produce yaw=0, pitch=0.
    let (yaw, pitch) = look_at_yaw_pitch(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
    assert!(yaw.abs() < 1e-3, "yaw should be 0, got {yaw}");
    assert!(pitch.abs() < 1e-3, "pitch should be 0, got {pitch}");
}

#[test]
fn look_at_cardinal_axes_round_trip_through_quat() {
    assert_forward_matches(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
    assert_forward_matches(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));
    assert_forward_matches(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
    assert_forward_matches(Vec3::ZERO, Vec3::new(-1.0, 0.0, 0.0));
    assert_forward_matches(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.001));
    assert_forward_matches(Vec3::ZERO, Vec3::new(0.0, -1.0, 0.001));
}

#[test]
fn look_at_offset_origin_round_trips() {
    // Camera at (10, 5, 200), target at (0, 0, 0): forward should
    // point toward origin and through-quat must reproduce that
    // direction.
    assert_forward_matches(Vec3::new(10.0, 5.0, 200.0), Vec3::ZERO);
    // The cam.tp default framing: 200 back + 50 up.
    let target = Vec3::new(100.0, 0.0, 0.0);
    let camera = target + Vec3::new(0.0, 50.0, 200.0);
    assert_forward_matches(camera, target);
}

#[test]
fn prid_sets_selected_ref_resource() {
    // Spawn an entity with Transform, run `prid <id>`, verify the
    // SelectedRef resource is updated. Output line should name the
    // entity. No-arg `prid` after the set should report the same.
    let mut world = World::new();
    world.insert_resource(SelectedRef::default());
    world.insert_resource(StringPool::new());

    let target = world.spawn();
    world.insert(target, Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)));

    let cmd = PridCommand;
    let out = cmd.execute(&world, &target.to_string()).lines.join("\n");
    assert!(
        out.contains(&format!("selected: entity {}", target)),
        "expected 'selected: entity {target}' in output: {out}"
    );

    // Resource state should now hold Some(target).
    let sel = world.resource::<SelectedRef>();
    assert_eq!(sel.0, Some(target));
    drop(sel);

    // `prid` with no args prints the current selection.
    let out2 = cmd.execute(&world, "").lines.join("\n");
    assert!(
        out2.contains(&format!("selected: entity {}", target)),
        "no-arg prid should print current selection: {out2}"
    );
}

#[test]
fn prid_rejects_entity_without_transform_or_global_transform() {
    // Entities that exist in the slot table but have no Transform
    // AND no GlobalTransform are conservatively rejected — usually
    // a sign of a typo or a hierarchy-orphan that wouldn't show
    // up in `entities`. The error should mention the id.
    let mut world = World::new();
    world.insert_resource(SelectedRef::default());
    world.insert_resource(StringPool::new());

    let orphan = world.spawn();
    // No Transform / GlobalTransform inserted.

    let cmd = PridCommand;
    let out = cmd.execute(&world, &orphan.to_string()).lines.join("\n");
    assert!(
        out.contains("no Transform/GlobalTransform"),
        "expected rejection message naming missing components: {out}"
    );

    // Resource must be untouched.
    let sel = world.resource::<SelectedRef>();
    assert!(sel.0.is_none(), "SelectedRef should remain None on rejected prid");
}

#[test]
fn cam_tp_no_args_uses_selected_ref() {
    // Set up: ActiveCamera with a Transform, a target with a
    // GlobalTransform, SelectedRef pointing at the target. `cam.tp`
    // with no args should treat the SelectedRef as if it were the
    // explicit argument and move the camera.
    let mut world = World::new();
    world.insert_resource(StringPool::new());

    let camera = world.spawn();
    world.insert(camera, Transform::from_translation(Vec3::ZERO));
    world.insert_resource(byroredux_core::ecs::ActiveCamera(camera));

    let target = world.spawn();
    let target_pos = Vec3::new(100.0, 0.0, 0.0);
    world.insert(
        target,
        GlobalTransform::new(target_pos, Quat::IDENTITY, 1.0),
    );

    world.insert_resource(SelectedRef(Some(target)));
    // InputState must exist because cam.tp tries to update it.
    world.insert_resource(InputState::default());

    let cmd = CamTpCommand;
    let out = cmd.execute(&world, "").lines.join("\n");
    assert!(
        out.contains(&format!("entity {target}")),
        "cam.tp w/o args should target SelectedRef ({target}): {out}"
    );

    // The camera transform should have moved away from origin.
    let tq = world.query::<Transform>().unwrap();
    let cam_t = tq.get(camera).unwrap();
    assert_ne!(cam_t.translation, Vec3::ZERO);
}

#[test]
fn cam_tp_no_args_no_selection_reports_usage() {
    // If SelectedRef is empty AND no arg is given, point the user
    // at both forms — direct (`cam.tp <id>`) and prid-first.
    let mut world = World::new();
    let camera = world.spawn();
    world.insert(camera, Transform::from_translation(Vec3::ZERO));
    world.insert_resource(byroredux_core::ecs::ActiveCamera(camera));
    world.insert_resource(SelectedRef::default());
    world.insert_resource(InputState::default());

    let cmd = CamTpCommand;
    let out = cmd.execute(&world, "").lines.join("\n");
    assert!(
        out.contains("usage:") && out.contains("prid"),
        "no-selection / no-arg should hint at prid workflow: {out}"
    );
}

#[test]
fn look_at_degenerate_zero_distance_returns_zero() {
    // Target equals source — no meaningful direction; return zero
    // instead of producing NaN or an arbitrary unit vector.
    let (yaw, pitch) = look_at_yaw_pitch(Vec3::new(5.0, 5.0, 5.0), Vec3::new(5.0, 5.0, 5.0));
    assert_eq!(yaw, 0.0);
    assert_eq!(pitch, 0.0);
}
