//! #5161 isolation probe — the BleakFallsBarrow01 skeever-corpse ragdoll
//! (`meshes\Actors\Skeever\Character Assets\skeleton.nif`, 21 bodies)
//! explodes its multibody solve to |t|=2.7e13 within one substep at
//! load-settle, poisoning the multi-SAP broad phase; a later proxy
//! insertion then panics `sap_axis.rs`. This probe replays the exact
//! live spec: seed poses captured from the failing engine run, shapes,
//! masses and joints straight from the authored NIF.
//!
//! Needs the extracted skeleton on disk:
//!   cargo run --release -p byroredux-bsa --example bsa_extract_one -- \
//!     "<Skyrim Data>/Skyrim - Meshes0.bsa" \
//!     "meshes\actors\skeever\character assets\skeleton.nif" /tmp/skeever_skeleton.nif
//!   BYRO_SKEEVER_NIF=/tmp/skeever_skeleton.nfa cargo test -p byroredux \
//!     --bin byroredux skeever -- --ignored --nocapture

use super::*;

/// Load the authored spec: shapes/masses/joints from the NIF, seed poses
/// verbatim from the failing run (bone order = template body order:
/// COG, LLeg1, LLeg3, LLegAnkle, RLeg1, RLeg3, RLegAnkle, SpineLower,
/// SpineUpper, LArm_Upper, LArm_Forearm, LArm_Palm, Neck, HEAD, RArm_Upper,
/// RArm_Forearm, RArm_Palm, Tail1..4).
fn skeever_spec(nif_bytes: &[u8]) -> RagdollSpec {
    let scene = byroredux_nif::parse_nif(nif_bytes).expect("parse skeever skeleton");
    let imported = byroredux_nif::import::collision::extract_ragdoll(&scene)
        .expect("skeever skeleton must carry its ragdoll");
    assert_eq!(imported.bodies.len(), 21);
    let mut bodies = vec![
        RagdollBodySpec { entity: 0, translation: Vec3::new(877.95874, -223.1185, -982.9688), rotation: Quat::from_xyzw(-0.5709435, -2.4956702e-8, 2.6645353e-15, 0.8209894), scale: 1.0, shape: imported.bodies[0].shape.clone(), mass: imported.bodies[0].mass, linear_damping: imported.bodies[0].linear_damping, angular_damping: imported.bodies[0].angular_damping, friction: imported.bodies[0].friction, restitution: imported.bodies[0].restitution },
        RagdollBodySpec { entity: 1, translation: Vec3::new(870.65076, -223.1185, -982.9688), rotation: Quat::from_xyzw(1.7787971e-6, -0.08429253, 0.99644107, -1.2319967e-5), scale: 1.0, shape: imported.bodies[1].shape.clone(), mass: imported.bodies[1].mass, linear_damping: imported.bodies[1].linear_damping, angular_damping: imported.bodies[1].angular_damping, friction: imported.bodies[1].friction, restitution: imported.bodies[1].restitution },
        RagdollBodySpec { entity: 2, translation: Vec3::new(870.65204, -246.57822, -979.4851), rotation: Quat::from_xyzw(3.0258088e-6, 0.14810261, 0.98897207, 5.257017e-5), scale: 1.0, shape: imported.bodies[2].shape.clone(), mass: imported.bodies[2].mass, linear_damping: imported.bodies[2].linear_damping, angular_damping: imported.bodies[2].angular_damping, friction: imported.bodies[2].friction, restitution: imported.bodies[2].restitution },
        RagdollBodySpec { entity: 3, translation: Vec3::new(870.65076, -258.30035, -975.8937), rotation: Quat::from_xyzw(-2.2710028e-6, -0.05001013, 0.99874884, 0.00015736476), scale: 1.0, shape: imported.bodies[3].shape.clone(), mass: imported.bodies[3].mass, linear_damping: imported.bodies[3].linear_damping, angular_damping: imported.bodies[3].angular_damping, friction: imported.bodies[3].friction, restitution: imported.bodies[3].restitution },
        RagdollBodySpec { entity: 4, translation: Vec3::new(885.2667, -223.1185, -982.9688), rotation: Quat::from_xyzw(-1.7940105e-6, -0.08429247, 0.996441, 1.2196118e-5), scale: 1.0, shape: imported.bodies[4].shape.clone(), mass: imported.bodies[4].mass, linear_damping: imported.bodies[4].linear_damping, angular_damping: imported.bodies[4].angular_damping, friction: imported.bodies[4].friction, restitution: imported.bodies[4].restitution },
        RagdollBodySpec { entity: 5, translation: Vec3::new(885.26544, -246.57822, -979.4851), rotation: Quat::from_xyzw(-2.0641637e-6, 0.14827311, 0.98894644, -5.959044e-5), scale: 1.0, shape: imported.bodies[5].shape.clone(), mass: imported.bodies[5].mass, linear_damping: imported.bodies[5].linear_damping, angular_damping: imported.bodies[5].angular_damping, friction: imported.bodies[5].friction, restitution: imported.bodies[5].restitution },
        RagdollBodySpec { entity: 6, translation: Vec3::new(885.2669, -258.2991, -975.88965), rotation: Quat::from_xyzw(8.548798e-6, -0.05022347, 0.998738, -3.0461651e-5), scale: 1.0, shape: imported.bodies[6].shape.clone(), mass: imported.bodies[6].mass, linear_damping: imported.bodies[6].linear_damping, angular_damping: imported.bodies[6].angular_damping, friction: imported.bodies[6].friction, restitution: imported.bodies[6].restitution },
        RagdollBodySpec { entity: 7, translation: Vec3::new(877.95874, -223.1185, -982.9688), rotation: Quat::from_xyzw(-0.6690868, -1.45933035e-8, 1.1501109e-8, 0.74318427), scale: 1.0, shape: imported.bodies[7].shape.clone(), mass: imported.bodies[7].mass, linear_damping: imported.bodies[7].linear_damping, angular_damping: imported.bodies[7].angular_damping, friction: imported.bodies[7].friction, restitution: imported.bodies[7].restitution },
        RagdollBodySpec { entity: 8, translation: Vec3::new(877.95874, -221.16072, -1001.57477), rotation: Quat::from_xyzw(-0.78394115, -1.4101124e-8, 1.6360787e-8, 0.6208353), scale: 1.0, shape: imported.bodies[8].shape.clone(), mass: imported.bodies[8].mass, linear_damping: imported.bodies[8].linear_damping, angular_damping: imported.bodies[8].angular_damping, friction: imported.bodies[8].friction, restitution: imported.bodies[8].restitution },
        RagdollBodySpec { entity: 9, translation: Vec3::new(868.88403, -221.16354, -1022.1943), rotation: Quat::from_xyzw(-0.004916489, 0.07325463, 0.99730134, 0.00011597574), scale: 1.0, shape: imported.bodies[9].shape.clone(), mass: imported.bodies[9].mass, linear_damping: imported.bodies[9].linear_damping, angular_damping: imported.bodies[9].angular_damping, friction: imported.bodies[9].friction, restitution: imported.bodies[9].restitution },
        RagdollBodySpec { entity: 10, translation: Vec3::new(868.86334, -242.64618, -1019.02136), rotation: Quat::from_xyzw(-0.0047494685, -0.15011288, 0.98865753, -0.00048709929), scale: 1.0, shape: imported.bodies[10].shape.clone(), mass: imported.bodies[10].mass, linear_damping: imported.bodies[10].linear_damping, angular_damping: imported.bodies[10].angular_damping, friction: imported.bodies[10].friction, restitution: imported.bodies[10].restitution },
        RagdollBodySpec { entity: 11, translation: Vec3::new(868.90076, -257.59818, -1023.6688), rotation: Quat::from_xyzw(-0.004700742, -0.24595328, 0.9692694, -0.001445059), scale: 1.0, shape: imported.bodies[11].shape.clone(), mass: imported.bodies[11].mass, linear_damping: imported.bodies[11].linear_damping, angular_damping: imported.bodies[11].angular_damping, friction: imported.bodies[11].friction, restitution: imported.bodies[11].restitution },
        RagdollBodySpec { entity: 12, translation: Vec3::new(877.95874, -221.65556, -1028.9419), rotation: Quat::from_xyzw(-0.5556017, -3.0389817e-8, 5.0541904e-9, 0.8314488), scale: 1.0, shape: imported.bodies[12].shape.clone(), mass: imported.bodies[12].mass, linear_damping: imported.bodies[12].linear_damping, angular_damping: imported.bodies[12].angular_damping, friction: imported.bodies[12].friction, restitution: imported.bodies[12].restitution },
        RagdollBodySpec { entity: 13, translation: Vec3::new(877.95874, -219.09085, -1035.135), rotation: Quat::from_xyzw(-0.70243776, -3.0797665e-8, -7.678427e-10, 0.71174544), scale: 1.0, shape: imported.bodies[13].shape.clone(), mass: imported.bodies[13].mass, linear_damping: imported.bodies[13].linear_damping, angular_damping: imported.bodies[13].angular_damping, friction: imported.bodies[13].friction, restitution: imported.bodies[13].restitution },
        RagdollBodySpec { entity: 14, translation: Vec3::new(887.03345, -221.16354, -1022.1943), rotation: Quat::from_xyzw(-0.0049166083, -0.07325485, -0.99730134, 0.000115945935), scale: 1.0, shape: imported.bodies[14].shape.clone(), mass: imported.bodies[14].mass, linear_damping: imported.bodies[14].linear_damping, angular_damping: imported.bodies[14].angular_damping, friction: imported.bodies[14].friction, restitution: imported.bodies[14].restitution },
        RagdollBodySpec { entity: 15, translation: Vec3::new(887.05414, -242.64618, -1019.0213), rotation: Quat::from_xyzw(-0.004749586, 0.15011267, -0.9886576, -0.00048715645), scale: 1.0, shape: imported.bodies[15].shape.clone(), mass: imported.bodies[15].mass, linear_damping: imported.bodies[15].linear_damping, angular_damping: imported.bodies[15].angular_damping, friction: imported.bodies[15].friction, restitution: imported.bodies[15].restitution },
        RagdollBodySpec { entity: 16, translation: Vec3::new(887.0167, -257.59818, -1023.6687), rotation: Quat::from_xyzw(-0.004700856, 0.24595307, -0.96926945, -0.0014451558), scale: 1.0, shape: imported.bodies[16].shape.clone(), mass: imported.bodies[16].mass, linear_damping: imported.bodies[16].linear_damping, angular_damping: imported.bodies[16].angular_damping, friction: imported.bodies[16].friction, restitution: imported.bodies[16].restitution },
        RagdollBodySpec { entity: 17, translation: Vec3::new(877.95874, -222.29276, -970.89075), rotation: Quat::from_xyzw(0.822417, 3.265173e-8, -4.2062585e-9, 0.5688851), scale: 1.0, shape: imported.bodies[17].shape.clone(), mass: imported.bodies[17].mass, linear_damping: imported.bodies[17].linear_damping, angular_damping: imported.bodies[17].angular_damping, friction: imported.bodies[17].friction, restitution: imported.bodies[17].restitution },
        RagdollBodySpec { entity: 18, translation: Vec3::new(877.95874, -227.98477, -955.7914), rotation: Quat::from_xyzw(0.8328696, 3.6110634e-8, 3.08209e-10, 0.55346936), scale: 1.0, shape: imported.bodies[18].shape.clone(), mass: imported.bodies[18].mass, linear_damping: imported.bodies[18].linear_damping, angular_damping: imported.bodies[18].angular_damping, friction: imported.bodies[18].friction, restitution: imported.bodies[18].restitution },
        RagdollBodySpec { entity: 19, translation: Vec3::new(877.95874, -233.67592, -942.2456), rotation: Quat::from_xyzw(0.7720883, 3.589096e-8, 3.9889536e-9, 0.63551533), scale: 1.0, shape: imported.bodies[19].shape.clone(), mass: imported.bodies[19].mass, linear_damping: imported.bodies[19].linear_damping, angular_damping: imported.bodies[19].angular_damping, friction: imported.bodies[19].friction, restitution: imported.bodies[19].restitution },
        RagdollBodySpec { entity: 20, translation: Vec3::new(877.95874, -236.56416, -927.5018), rotation: Quat::from_xyzw(0.68852943, 3.1734345e-8, 4.221069e-9, 0.7252084), scale: 1.0, shape: imported.bodies[20].shape.clone(), mass: imported.bodies[20].mass, linear_damping: imported.bodies[20].linear_damping, angular_damping: imported.bodies[20].angular_damping, friction: imported.bodies[20].friction, restitution: imported.bodies[20].restitution },
    ];
    let zero_rest = std::env::var("BYRO_PROBE_ZERO_REST").is_ok();
    let no_joints = std::env::var("BYRO_PROBE_NOJOINTS").is_ok();
    let constraints = if no_joints {
        println!("probe: joints DROPPED (free-body isolation)");
        Vec::new()
    } else {
        imported
        .constraints
        .iter()
        .map(|c| RagdollConstraintSpec {
            body_a: c.body_a,
            body_b: c.body_b,
            joint: joint_from_imported(&c.kind),
        })
        .collect()
    };
    if zero_rest {
        println!("probe: restitution zeroed");
        for b in &mut bodies {
            b.restitution = 0.0;
        }
    }
    RagdollSpec { bodies, constraints }
}

/// Step the exact live spec on a flat floor and report the worst body
/// translation magnitude every 60 ticks. The live failure reaches 2.7e13
/// within one substep of the first contact solve.
#[test]
#[ignore = "real-data isolation probe (needs BYRO_SKEEVER_NIF)"]
fn skeever_ragdoll_survives_six_hundred_floor_ticks() {
    let path = std::env::var("BYRO_SKEEVER_NIF")
        .unwrap_or_else(|_| "/tmp/byro-5161-probe3/skeever_skeleton.nif".to_owned());
    let Ok(nif_bytes) = std::fs::read(&path) else {
        panic!("set BYRO_SKEEVER_NIF to the extracted skeever skeleton (missing: {path})");
    };
    let spec = skeever_spec(&nif_bytes);

    let mut world = World::new();
    world.register::<Transform>();
    world.register::<GlobalTransform>();
    world.register::<CollisionShape>();
    world.register::<RigidBodyData>();
    world.register::<RapierHandles>();
    world.insert_resource(PhysicsWorld::new());

    // Flat floor under the corpse pose (seeds span y≈[-262, -219]).
    let floor = world.spawn();
    let no_floor = std::env::var("BYRO_PROBE_NOFLOOR").is_ok();
    if no_floor {
        println!("probe: floor DISABLED (joint-only isolation)");
    }
    // Transform must carry the pose: register_newcomers re-runs transform
    // propagation, which recomposes GlobalTransform from Transform and would
    // otherwise park the floor at the origin. GlobalTransform is seeded to
    // match so the composition below is a no-op.
    // BYRO_PROBE_PENETRATION=N raises the floor top N BU INTO the corpse's
    // lowest capsules (seed bottoms ≈ y −262), simulating activation inside
    // real corridor geometry.
    let penetration: f32 = std::env::var("BYRO_PROBE_PENETRATION")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let floor_top = -262.0 + penetration;
    let floor_pose = Transform {
        translation: Vec3::new(877.0, floor_top - 5.0, -985.0),
        rotation: Quat::IDENTITY,
        scale: 1.0,
    };
    println!("probe: floor top at y={floor_top} (penetration {penetration} BU)");
    world.insert(floor, floor_pose);
    world.insert(
        floor,
        GlobalTransform {
            translation: floor_pose.translation,
            rotation: Quat::IDENTITY,
            scale: 1.0,
        },
    );
    world.insert(floor, CollisionShape::Cuboid {
        half_extents: Vec3::new(5000.0, 5.0, 5000.0),
    });
    world.insert(floor, RigidBodyData::STATIC);
    if no_floor {
        world.remove::<CollisionShape>(floor);
    }
    let registered = byroredux_physics::register_newcomers_and_refresh_queries(&world);
    assert_eq!(registered, usize::from(!no_floor), "floor must register");

    let rag = {
        let mut pw = world.resource_mut::<PhysicsWorld>();
        build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane seed")
    };

    for tick in 0..600 {
        let (worst, worst_body) = {
            let mut pw = world.resource_mut::<PhysicsWorld>();
            pw.step(byroredux_physics::world::PHYSICS_DT);
            rag.bodies
                .iter()
                .filter_map(|&(e, h, _)| {
                    byroredux_physics::ragdoll::body_translation(&pw, h).map(|t| (t.length(), e))
                })
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .unwrap_or((0.0, u32::MAX))
        };
        if tick % 30 == 0 || tick > 590 {
            println!("tick {tick}: worst |t| = {worst:.3e} (entity {worst_body})");
        }
        if tick == 0 || tick == 15 || tick == 30 || tick == 60 || tick == 120 {
            let pw = world.resource::<PhysicsWorld>();
            let ys: Vec<String> = rag
                .bodies
                .iter()
                .map(|&(e, h, _)| {
                    let y = byroredux_physics::ragdoll::body_translation(&pw, h)
                        .map(|t| format!("{:.1}", t.y))
                        .unwrap_or("?".into());
                    format!("{e}:{y}")
                })
                .collect();
            println!("tick {tick} y: {}", ys.join(" "));
        }
        assert!(
            worst.is_finite() && worst <= 5.0e5,
            "articulation diverged at tick {tick}: worst |t| = {worst:.3e}"
        );
    }
}
