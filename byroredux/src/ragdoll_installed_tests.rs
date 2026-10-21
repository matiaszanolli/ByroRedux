use super::*;
use byroredux_core::string::StringPool;

/// Isolate the restored bind-pose articulation from world collision and saves.
#[test]
#[ignore = "requires installed Fallout 3 mesh archive"]
fn fo3_bind_pose_ragdoll_stays_finite_in_free_fall() {
    check_fo3_bind_pose(Environment::FreeFall);
}

#[test]
#[ignore = "requires installed Fallout 3 mesh archive"]
fn fo3_bind_pose_ragdoll_stays_finite_on_flat_floor() {
    check_fo3_bind_pose(Environment::FlatFloor);
}

#[test]
#[ignore = "requires installed Fallout 3 master and mesh archive"]
fn fo3_bind_pose_ragdoll_stays_finite_in_saloon_geometry() {
    check_fo3_bind_pose(Environment::Saloon);
}

#[test]
#[ignore = "requires installed Fallout 3 master and mesh archive"]
fn fo3_bind_pose_ragdoll_stays_finite_among_saloon_followers() {
    check_fo3_bind_pose(Environment::SaloonFollowers);
}

#[derive(Clone, Copy, PartialEq)]
enum Environment {
    FreeFall,
    FlatFloor,
    Saloon,
    SaloonFollowers,
    ReloadedSaloon,
    RagdollFirstReload,
}

#[test]
#[ignore = "requires installed Fallout 3 master and mesh archive"]
fn fo3_bind_pose_ragdoll_stays_finite_when_created_before_restored_geometry() {
    check_fo3_bind_pose(Environment::RagdollFirstReload);
}

#[test]
#[ignore = "requires installed Fallout 3 master and mesh archive"]
fn fo3_bind_pose_ragdoll_stays_finite_after_physics_world_reuse() {
    check_fo3_bind_pose(Environment::ReloadedSaloon);
}

fn check_fo3_bind_pose(environment: Environment) {
    let data = std::env::var("BYROREDUX_FO3_DATA")
        .unwrap_or_else(|_| "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data".into());
    let archive =
        byroredux_bsa::BsaArchive::open(std::path::Path::new(&data).join("Fallout - Meshes.bsa"))
            .expect("Fallout 3 mesh archive required for this explicit test");
    let bytes = archive
        .extract(r"meshes\characters\_male\skeleton.nif")
        .unwrap();
    let parsed = byroredux_nif::parse_nif(&bytes).unwrap();
    let imported = byroredux_nif::import::import_nif_scene(&parsed, &mut StringPool::new());
    for yaw in [0.0_f32, -std::f32::consts::FRAC_PI_2] {
        let mut world = World::new();
        world.register::<GlobalTransform>();
        world.register::<RagdollTemplate>();
        world.register::<RagdollActive>();
        world.register::<Ragdoll>();
        world.insert_resource(
            if matches!(
                environment,
                Environment::ReloadedSaloon | Environment::RagdollFirstReload
            ) {
                reused_saloon_physics(&archive, &imported, &data)
            } else {
                PhysicsWorld::new()
            },
        );
        if environment != Environment::FreeFall {
            world.register::<Transform>();
            world.register::<CollisionShape>();
            world.register::<RigidBodyData>();
            world.register::<RapierHandles>();
        }
        if matches!(
            environment,
            Environment::Saloon | Environment::SaloonFollowers | Environment::ReloadedSaloon
        ) {
            add_saloon_geometry(&mut world, &archive, &data);
            if matches!(
                environment,
                Environment::SaloonFollowers | Environment::ReloadedSaloon
            ) {
                add_saloon_followers(&mut world, &imported, &data);
            }
        } else if environment == Environment::FlatFloor {
            let floor = world.spawn();
            let position = Vec3::new(1049.3739, -138.0, -333.2);
            world.insert(floor, Transform::new(position, Quat::IDENTITY, 1.0));
            world.insert(
                floor,
                GlobalTransform {
                    translation: position,
                    ..GlobalTransform::IDENTITY
                },
            );
            world.insert(
                floor,
                CollisionShape::Cuboid {
                    half_extents: Vec3::new(1000.0, 10.0, 1000.0),
                },
            );
            world.insert(floor, RigidBodyData::static_body());
            assert_eq!(
                byroredux_physics::register_newcomers_and_refresh_queries(&world),
                1
            );
        }
        let actor = world.spawn();
        let mut placement = GlobalTransform {
            translation: Vec3::new(1049.3739, -131.3, -333.2),
            rotation: Quat::from_rotation_y(yaw),
            scale: 1.0,
        };
        if matches!(
            environment,
            Environment::Saloon
                | Environment::SaloonFollowers
                | Environment::ReloadedSaloon
                | Environment::RagdollFirstReload
        ) {
            let bytes = std::fs::read(std::path::Path::new(&data).join("Fallout3.esm")).unwrap();
            let index = byroredux_plugin::esm::parse_esm(&bytes).unwrap();
            let target = index.cells.cells["megatonmoriartyssaloon"]
                .references
                .iter()
                .find(|r| r.form_id == 0x41709)
                .unwrap();
            placement.translation =
                Vec3::from_array(byroredux_core::math::coord::zup_to_yup_pos(target.position));
            placement.rotation = byroredux_core::math::coord::euler_zup_to_quat_yup_mode(
                1,
                target.rotation[0],
                target.rotation[1],
                target.rotation[2],
            ) * Quat::from_rotation_y(yaw);
            placement.scale = target.scale;
            eprintln!("target placement {placement:?}");
        }
        let mut rest_poses = Vec::<GlobalTransform>::new();
        let mut by_name = HashMap::new();
        let mut rest_by_name = HashMap::new();
        for node in &imported.nodes {
            let local = GlobalTransform {
                translation: Vec3::from_array(node.translation),
                rotation: Quat::from_array(node.rotation),
                scale: node.scale,
            };
            let rest = node
                .parent_node
                .map(|p| {
                    GlobalTransform::compose(
                        &rest_poses[p],
                        local.translation,
                        local.rotation,
                        local.scale,
                    )
                })
                .unwrap_or(local);
            rest_poses.push(rest);
            let entity = world.spawn();
            world.insert(
                entity,
                GlobalTransform::compose(&placement, rest.translation, rest.rotation, rest.scale),
            );
            if environment == Environment::ReloadedSaloon {
                if let Some((shape, body)) = &node.collision {
                    let global = *world.get::<GlobalTransform>(entity).unwrap();
                    world.insert(
                        entity,
                        Transform::new(global.translation, global.rotation, global.scale),
                    );
                    world.insert(entity, shape.clone());
                    let mut body = body.clone();
                    body.motion_type = byroredux_core::ecs::components::MotionType::Keyframed;
                    world.insert(entity, body);
                    world.insert(entity, byroredux_physics::ActorBoneCollider);
                }
            }
            if let Some(name) = &node.name {
                by_name.entry(name.clone()).or_insert(entity);
                rest_by_name.entry(name.clone()).or_insert(rest);
            }
        }
        let template = template_from_imported(
            imported.ragdoll.as_ref().expect("authored ragdoll"),
            &by_name,
            &rest_by_name,
        )
        .unwrap();
        assert_eq!(template.bodies.len(), 18);
        world.insert(actor, template);
        if environment == Environment::ReloadedSaloon {
            assert_eq!(
                byroredux_physics::register_newcomers_and_refresh_queries(&world),
                18
            );
        }
        assert_eq!(activate_ragdoll(&world, actor).unwrap(), 18);
        if environment == Environment::RagdollFirstReload {
            add_saloon_geometry(&mut world, &archive, &data);
            add_saloon_followers(&mut world, &imported, &data);
        }
        let rag = world.get::<Ragdoll>(actor).unwrap().clone();
        let mut physics = world.resource_mut::<PhysicsWorld>();
        for step in 0..600 {
            physics.step(byroredux_physics::world::PHYSICS_DT);
            for (bone, handle, _) in &rag.bodies {
                let body = &physics.bodies[*handle];
                assert!(
                    body.translation().is_finite()
                        && body.rotation().is_finite()
                        && body.linvel().is_finite()
                        && body.angvel().is_finite(),
                    "yaw={yaw} step={step} bone={bone}"
                );
                if environment != Environment::FreeFall {
                    let p = body.translation();
                    let distance = Vec3::new(p.x, p.y, p.z).distance(placement.translation);
                    assert!(
                        distance <= 512.0,
                        "yaw={yaw} step={step} bone={bone} distance={distance}"
                    );
                }
            }
        }
    }
}

fn reused_saloon_physics(
    archive: &byroredux_bsa::BsaArchive,
    scene: &byroredux_nif::import::ImportedScene,
    data: &str,
) -> PhysicsWorld {
    let mut world = World::new();
    world.register::<Transform>();
    world.register::<GlobalTransform>();
    world.register::<CollisionShape>();
    world.register::<RigidBodyData>();
    world.register::<RapierHandles>();
    world.insert_resource(PhysicsWorld::new());
    add_saloon_geometry(&mut world, archive, data);
    add_saloon_followers(&mut world, scene, data);
    let mut physics = world.remove_resource::<PhysicsWorld>().unwrap();
    physics.wake();
    for _ in 0..60 {
        physics.step(byroredux_physics::world::PHYSICS_DT);
    }
    let handles: Vec<_> = physics.bodies.iter().map(|(handle, _)| handle).collect();
    for handle in handles {
        assert!(physics.remove_body(handle));
    }
    assert_eq!(physics.body_count(), 0);
    physics
}

// Static bind-pose followers isolate overlap with other actors. Animation and
// runtime actor assembly are deliberately not reproduced by this control.
fn add_saloon_followers(
    world: &mut World,
    scene: &byroredux_nif::import::ImportedScene,
    data: &str,
) {
    world.register::<byroredux_physics::ActorBoneCollider>();
    let bytes = std::fs::read(std::path::Path::new(data).join("Fallout3.esm")).unwrap();
    let index = byroredux_plugin::esm::parse_esm(&bytes).unwrap();
    let cell = &index.cells.cells["megatonmoriartyssaloon"];
    let mut actors = 0;
    for placed in &cell.references {
        if placed.form_id == 0x41709 || !index.npcs.contains_key(&placed.base_form_id) {
            continue;
        }
        actors += 1;
        let placement = GlobalTransform {
            translation: Vec3::from_array(byroredux_core::math::coord::zup_to_yup_pos(
                placed.position,
            )),
            rotation: byroredux_core::math::coord::euler_zup_to_quat_yup_mode(
                1,
                placed.rotation[0],
                placed.rotation[1],
                placed.rotation[2],
            ),
            scale: placed.scale,
        };
        let mut globals = Vec::<GlobalTransform>::new();
        for node in &scene.nodes {
            let parent = node.parent_node.map(|p| &globals[p]).unwrap_or(&placement);
            let global = GlobalTransform::compose(
                parent,
                Vec3::from_array(node.translation),
                Quat::from_array(node.rotation),
                node.scale,
            );
            globals.push(global);
            if let Some((shape, body)) = &node.collision {
                let entity = world.spawn();
                world.insert(
                    entity,
                    Transform::new(global.translation, global.rotation, global.scale),
                );
                world.insert(entity, global);
                world.insert(entity, shape.clone());
                let mut body = body.clone();
                body.motion_type = byroredux_core::ecs::components::MotionType::Keyframed;
                world.insert(entity, body);
                world.insert(entity, byroredux_physics::ActorBoneCollider);
            }
        }
    }
    assert_eq!(actors, 5);
    let registered = byroredux_physics::register_newcomers_and_refresh_queries(world);
    assert_eq!(registered, 5 * 18);
}

/// Geometry-only control, not the runtime cell loader: deliberately excludes
/// NPC followers, player capsule, save overlays, and scripts.
fn add_saloon_geometry(world: &mut World, archive: &byroredux_bsa::BsaArchive, data: &str) {
    let bytes = std::fs::read(std::path::Path::new(data).join("Fallout3.esm")).unwrap();
    let index = byroredux_plugin::esm::parse_esm(&bytes).unwrap();
    let cell = &index.cells.cells["megatonmoriartyssaloon"];
    let mut collision_nodes = 0;
    for placed in &cell.references {
        let Some(base) = index.cells.statics.get(&placed.base_form_id) else {
            continue;
        };
        if base.model_path.is_empty() {
            continue;
        }
        let path = format!("meshes\\{}", base.model_path);
        let bytes = archive
            .extract(&path)
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        let nif = byroredux_nif::parse_nif(&bytes).unwrap();
        let scene = byroredux_nif::import::import_nif_scene(&nif, &mut StringPool::new());
        let placement = GlobalTransform {
            translation: Vec3::from_array(byroredux_core::math::coord::zup_to_yup_pos(
                placed.position,
            )),
            rotation: byroredux_core::math::coord::euler_zup_to_quat_yup_mode(
                1,
                placed.rotation[0],
                placed.rotation[1],
                placed.rotation[2],
            ),
            scale: placed.scale,
        };
        let mut globals = Vec::<GlobalTransform>::new();
        for node in &scene.nodes {
            let parent = node.parent_node.map(|p| &globals[p]).unwrap_or(&placement);
            let global = GlobalTransform::compose(
                parent,
                Vec3::from_array(node.translation),
                Quat::from_array(node.rotation),
                node.scale,
            );
            globals.push(global);
            if let Some((shape, body)) = &node.collision {
                let entity = world.spawn();
                world.insert(
                    entity,
                    Transform::new(global.translation, global.rotation, global.scale),
                );
                world.insert(entity, global);
                world.insert(entity, shape.clone());
                world.insert(entity, body.clone());
                collision_nodes += 1;
            }
        }
    }
    assert!(collision_nodes > 0);
    let registered = byroredux_physics::register_newcomers_and_refresh_queries(world);
    assert!(registered > 0);
    eprintln!("saloon geometry: {collision_nodes} authored collision nodes, {registered} registered bodies");
}

/// #6070 — FO4's decoded `bhkRagdollSystem` articulation, bind pose, in free
/// fall over the real skeleton NIFs: every body stays finite, no solver
/// explosion is clamped, and every authored joint stays live.
#[test]
#[ignore = "requires installed Fallout 4 mesh archive"]
fn fo4_bind_pose_ragdoll_holds_together_in_free_fall() {
    for path in [
        r"meshes\actors\character\characterassets\skeleton.nif",
        r"meshes\actors\feralghoul\characterassets\skeleton.nif",
        r"meshes\actors\deathclaw\characterassets\skeleton.nif",
        r"meshes\actors\mirelurkqueen\characterassets\skeleton.nif",
        r"meshes\actors\molerat\characterassets\skeleton.nif",
        r"meshes\actors\supermutant\characterassets\skeleton.nif",
    ] {
        check_fo4_bind_pose(path);
    }
}

fn check_fo4_bind_pose(skeleton: &str) {
    let data = std::env::var("BYROREDUX_FO4_DATA")
        .unwrap_or_else(|_| "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data".into());
    let archive = byroredux_bsa::Ba2Archive::open(
        std::path::Path::new(&data).join("Fallout4 - Meshes.ba2"),
    )
    .expect("Fallout 4 mesh archive required for this explicit test");
    let bytes = archive.extract(skeleton).unwrap();
    let parsed = byroredux_nif::parse_nif(&bytes).unwrap();
    let imported = byroredux_nif::import::import_nif_scene(&parsed, &mut StringPool::new());
    let mut world = World::new();
    world.register::<GlobalTransform>();
    world.register::<RagdollTemplate>();
    world.register::<RagdollActive>();
    world.register::<Ragdoll>();
    world.insert_resource(PhysicsWorld::new());
    let actor = world.spawn();
    let placement = GlobalTransform {
        translation: Vec3::new(0.0, 500.0, 0.0),
        rotation: Quat::IDENTITY,
        scale: 1.0,
    };
    let mut rest_poses = Vec::<GlobalTransform>::new();
    let mut by_name = HashMap::new();
    let mut rest_by_name = HashMap::new();
    for node in &imported.nodes {
        let local = GlobalTransform {
            translation: Vec3::from_array(node.translation),
            rotation: Quat::from_array(node.rotation),
            scale: node.scale,
        };
        let rest = node
            .parent_node
            .map(|p| {
                GlobalTransform::compose(&rest_poses[p], local.translation, local.rotation, local.scale)
            })
            .unwrap_or(local);
        rest_poses.push(rest);
        let entity = world.spawn();
        world.insert(
            entity,
            GlobalTransform::compose(&placement, rest.translation, rest.rotation, rest.scale),
        );
        if let Some(name) = &node.name {
            by_name.entry(name.clone()).or_insert(entity);
            rest_by_name.entry(name.clone()).or_insert(rest);
        }
    }
    let template = template_from_imported(
        imported.ragdoll.as_ref().expect("authored ragdoll"),
        &by_name,
        &rest_by_name,
    )
    .unwrap();
    let (bodies, joints) = (template.bodies.len(), template.constraints.len());
    world.insert(actor, template);
    assert_eq!(activate_ragdoll(&world, actor).unwrap(), bodies);
    let rag = world.get::<Ragdoll>(actor).unwrap().clone();
    let mut physics = world.resource_mut::<PhysicsWorld>();
    let mut peak_speed = 0.0_f32;
    for step in 0..120 {
        physics.step(byroredux_physics::world::PHYSICS_DT);
        for (bone, handle, _) in &rag.bodies {
            let body = &physics.bodies[*handle];
            assert!(
                body.translation().is_finite() && body.linvel().is_finite(),
                "{skeleton} step={step} bone={bone:?}"
            );
            peak_speed = peak_speed.max(body.linvel().length());
        }
    }
    let live_joints = rag
        .joints
        .iter()
        .filter(|jh| physics.multibody_joints.get(**jh).is_some())
        .count();
    let (recoveries, _, parked) = physics.recovery_counts();
    eprintln!(
        "{skeleton}: {bodies} bodies, {joints} joints, {live_joints} live, peak speed {peak_speed:.1}, recoveries {recoveries}, parked {parked}"
    );
    assert_eq!(live_joints, joints, "{skeleton}: every authored joint stays live");
    assert_eq!(recoveries, 0, "{skeleton}: no solver explosion");
}


/// The MedTekResearch01 feral-ghoul corpse (REFR 00106A72)'s authored XRGD
/// entries: `[bone_id, px, py, pz, rx, ry, rz]`.
const MEDTEK_GHOUL_XRGD: [[f32; 7]; 18] = [
    [2.0, 0.0, 0.0, 0.0, 0.982891, -0.83291495, 0.3806384],
    [8.0, 1.0881348, 0.5427246, 6.904175, 3.0855272, 0.02029229, -2.8147612],
    [9.0, 32.564575, 0.011962891, 0.011352539, 0.0005713551, -0.002183033, 8.3994346e-05],
    [10.0, 33.193115, -0.0546875, 0.04272461, 0.038358875, 0.09961901, -1.2273268],
    [14.0, 1.0600586, 0.5116577, -6.9746094, -2.8647003, -0.21455453, 2.4875896],
    [15.0, 32.561768, 0.008972168, -0.008850098, -0.0053630644, -0.0026632878, 1.1164975],
    [16.0, 33.155396, 0.019165039, -0.052612305, -0.041616186, -0.09236335, -0.91072154],
    [2.0, 4.73291, -0.02532959, -0.02734375, -0.11025668, 0.13798705, -0.20583436],
    [3.0, 8.811035, 0.03894043, 0.010498047, -0.06941078, 0.1794942, 0.11297216],
    [4.0, 10.286133, 0.057006836, 0.02355957, -0.07960307, 0.07318326, 0.2433533],
    [0.0, 0.0107421875, -0.08169556, -0.030700684, 0.7636364, -0.05287812, -0.11463284],
    [1.0, 11.40332, -0.03656006, -0.06985474, 0.089743, -0.01680755, 0.1836224],
    [5.0, 11.576416, -0.06703949, -0.01586914, -0.57054806, 0.65146863, -0.40236393],
    [6.0, 21.709473, -0.034423828, 0.0037841797, 0.17952132, 0.19459383, 0.009497889],
    [7.0, 10.176636, 0.016479492, 0.010253906, 1.5710905, 0.0042326427, 0.4348193],
    [11.0, 11.601746, -0.039367676, 0.0068359375, 0.113780975, -0.110522434, -0.63460135],
    [12.0, 21.722412, -0.023468018, 0.0056152344, -0.38448408, -0.014733579, -0.42364147],
    [13.0, 10.195801, -0.052490234, 0.024902344, -1.5646036, 0.0044793785, -0.2856283],
];

/// #6070 — the MedTek feral-ghoul corpse's XRGD pose over the real ghoul
/// skeleton: every joint's two pivots still name one world point (the pose is
/// forward kinematics over the bone tree), the ragdoll holds together in free
/// fall, and no joint is more than 0.15 rad past its limits (the CK settles a corpse against soft limits).
#[test]
#[ignore = "requires installed Fallout 4 mesh archive"]
fn fo4_xrgd_posed_ghoul_corpse_holds_together_in_free_fall() {
    use byroredux_core::ecs::{Name, Parent};
    use byroredux_plugin::esm::cell::RagdollPoseBone;
    let data = std::env::var("BYROREDUX_FO4_DATA")
        .unwrap_or_else(|_| "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data".into());
    let archive = byroredux_bsa::Ba2Archive::open(
        std::path::Path::new(&data).join("Fallout4 - Meshes.ba2"),
    )
    .unwrap();
    let bytes = archive
        .extract(r"meshes\actors\feralghoul\characterassets\skeleton.nif")
        .unwrap();
    let parsed = byroredux_nif::parse_nif(&bytes).unwrap();
    let mut pool = StringPool::new();
    let imported = byroredux_nif::import::import_nif_scene(&parsed, &mut pool);
    let mut world = World::new();
    world.register::<GlobalTransform>();
    world.register::<Transform>();
    world.register::<Parent>();
    world.register::<Name>();
    world.register::<byroredux_core::ecs::Children>();
    world.register::<RagdollTemplate>();
    world.register::<RagdollActive>();
    world.register::<Ragdoll>();
    let actor = world.spawn();
    let placement = GlobalTransform {
        translation: Vec3::new(0.0, 500.0, 0.0),
        rotation: Quat::IDENTITY,
        scale: 1.0,
    };
    let mut entities: Vec<EntityId> = Vec::new();
    let mut rest_poses = Vec::<GlobalTransform>::new();
    let mut by_name = HashMap::new();
    let mut rest_by_name = HashMap::new();
    for node in &imported.nodes {
        let local = Transform::new(
            Vec3::from_array(node.translation),
            Quat::from_array(node.rotation),
            node.scale,
        );
        let rest = node
            .parent_node
            .map(|p| GlobalTransform::compose(&rest_poses[p], local.translation, local.rotation, local.scale))
            .unwrap_or(GlobalTransform { translation: local.translation, rotation: local.rotation, scale: local.scale });
        rest_poses.push(rest);
        let entity = world.spawn();
        world.insert(entity, local);
        world.insert(
            entity,
            GlobalTransform::compose(&placement, rest.translation, rest.rotation, rest.scale),
        );
        let parent = node.parent_node.map_or(actor, |p| entities[p]);
        world.insert(entity, Parent(parent));
        crate::helpers::add_child(&mut world, parent, entity);
        if let Some(name) = &node.name {
            world.insert(entity, Name(pool.intern(name)));
            by_name.entry(name.clone()).or_insert(entity);
            rest_by_name.entry(name.clone()).or_insert(rest);
        }
        entities.push(entity);
    }
    world.insert_resource(pool);
    world.insert(actor, placement);
    world.insert_resource(PhysicsWorld::new());
    let template = template_from_imported(
        imported.ragdoll.as_ref().expect("authored ragdoll"),
        &by_name,
        &rest_by_name,
    )
    .unwrap();
    world.insert(actor, template);
    let entries: Vec<RagdollPoseBone> = MEDTEK_GHOUL_XRGD
        .iter()
        .map(|e| RagdollPoseBone {
            bone_id: e[0] as u8,
            position: [e[1], e[2], e[3]],
            rotation: [e[4], e[5], e[6]],
        })
        .collect();
    let posed = crate::ragdoll_pose::pose_skeleton(&world, actor, &entries);
    eprintln!("pose_skeleton: {posed:?}");
    posed.unwrap();

    // Joint residuals at the posed seed.
    let template = world.get::<RagdollTemplate>(actor).unwrap().clone();
    let seed = |i: usize| {
        let b = &template.bodies[i];
        let gt = *world.get::<GlobalTransform>(b.bone).unwrap();
        (gt.translation + gt.rotation * (b.local_translation * gt.scale), gt.rotation * b.local_rotation)
    };
    for (k, c) in template.constraints.iter().enumerate() {
        let (pa, ra) = seed(c.body_a);
        let (pb, rb) = seed(c.body_b);
        if let RagdollJointSpec::Ragdoll { twist_a, plane_a, pivot_a, twist_b, plane_b, pivot_b, cone_max, twist_min, twist_max } = &c.joint {
            let wa = pa + ra * *pivot_a;
            let wb = pb + rb * *pivot_b;
            let ta = ra * *twist_a;
            let tb = rb * *twist_b;
            let cone = ta.dot(tb).clamp(-1.0, 1.0).acos();
            // twist: angle between plane axes after removing the swing.
            let swing = Quat::from_rotation_arc(tb, ta);
            let plane_b_aligned = swing * (rb * *plane_b);
            let pl_a = ra * *plane_a;
            let twist = {
                let x = pl_a.dot(plane_b_aligned).clamp(-1.0, 1.0).acos();
                let sign = ta.dot(plane_b_aligned.cross(pl_a));
                if sign < 0.0 { -x } else { x }
            };
            eprintln!(
                "joint {k:2} {:>10}<->{:<10} pivot_gap={:7.3} cone={:6.3}/{:5.3} twist={:6.3} in [{:6.3},{:6.3}]",
                crate::commands::shared::resolve_entity_name(&world, template.bodies[c.body_a].bone).unwrap_or_default(),
                crate::commands::shared::resolve_entity_name(&world, template.bodies[c.body_b].bone).unwrap_or_default(),
                wa.distance(wb), cone, cone_max, twist, twist_min, twist_max
            );
            assert!(wa.distance(wb) < 0.01, "joint {k}: pivots must name one world point");
            assert!(cone <= cone_max + 0.15 && twist >= twist_min - 0.15 && twist <= twist_max + 0.15, "joint {k} is far past its limits");
        } else if let RagdollJointSpec::LimitedHinge { axis_a, pivot_a, axis_b, pivot_b, min_angle, max_angle, .. } = &c.joint {
            let wa = pa + ra * *pivot_a;
            let wb = pb + rb * *pivot_b;
            let aa = ra * *axis_a;
            let ab = rb * *axis_b;
            eprintln!(
                "hinge {k:2} {:>10}<->{:<10} pivot_gap={:7.3} axis_angle={:6.3} limits [{:6.3},{:6.3}]",
                crate::commands::shared::resolve_entity_name(&world, template.bodies[c.body_a].bone).unwrap_or_default(),
                crate::commands::shared::resolve_entity_name(&world, template.bodies[c.body_b].bone).unwrap_or_default(),
                wa.distance(wb), aa.dot(ab).clamp(-1.0, 1.0).acos(), min_angle, max_angle
            );
            assert!(wa.distance(wb) < 0.01, "hinge {k}: pivots must name one world point");
        }
    }
    assert_eq!(activate_ragdoll(&world, actor).unwrap(), 18);
    let rag = world.get::<Ragdoll>(actor).unwrap().clone();
    let mut physics = world.resource_mut::<PhysicsWorld>();
    for _ in 0..120 {
        physics.step(byroredux_physics::world::PHYSICS_DT);
    }
    let live = rag.joints.iter().filter(|jh| physics.multibody_joints.get(**jh).is_some()).count();
    let (recoveries, _, parked) = physics.recovery_counts();
    eprintln!("after 120 steps: live joints {live}/{}, recoveries {recoveries}, parked {parked}", rag.joints.len());
    assert_eq!(live, rag.joints.len(), "every authored joint stays live");
    assert_eq!((recoveries, parked), (0, 0), "no solver explosion");
}
