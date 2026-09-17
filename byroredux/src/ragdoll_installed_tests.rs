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
            world.insert(floor, RigidBodyData::STATIC);
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
                    body.translation()
                        .iter()
                        .chain(body.rotation().coords.iter())
                        .chain(body.linvel().iter())
                        .chain(body.angvel().iter())
                        .all(|v| v.is_finite()),
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
