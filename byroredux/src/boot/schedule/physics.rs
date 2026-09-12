//! Physics-stage registrations (#3855, split from `boot.rs`).

use byroredux_core::ecs::{Access, Scheduler, Stage, TotalTime, Transform};

/// `Stage::Physics` registrations (#3739 split of `build_scheduler`).
pub(super) fn register_physics_systems(scheduler: &mut Scheduler) {
    scheduler.add_to_with_access(
        Stage::Physics,
        byroredux_physics::physics_sync_system,
        Access::new()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .writes_resource::<byroredux_physics::PhysicsWorld>()
            // WATAL Phase 2 — the buoyancy phase reads the engine water
            // constants resource (declaration completeness; read-only).
            .reads_resource::<byroredux_physics::PhysicsWaterConstants>()
            .writes_resource::<byroredux_physics::WaterContactScratch>()
            .reads_resource::<TotalTime>()
            .reads_resource::<byroredux_core::ecs::components::groundcover::WindField>()
            // #1787 / CONC-D4-01 — `register_newcomers` snapshots
            // `ContactConfig` once per batch (kcc_offset_bu / trimesh
            // flags); read-only, but must be declared so a future
            // parallel system that writes it is caught by the
            // conflict analyzer instead of silently racing.
            .reads_resource::<byroredux_physics::ContactConfig>()
            .reads::<byroredux_core::ecs::components::CollisionShape>()
            .reads::<byroredux_core::ecs::components::RigidBodyData>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .reads::<byroredux_core::ecs::Parent>()
            .reads::<byroredux_physics::ActorBoneCollider>()
            .reads::<byroredux_physics::RapierHandles>()
            .writes::<byroredux_physics::RapierHandles>()
            .writes::<Transform>()
            // #3492 — the buoyancy phase's second target source: both
            // `apply_buoyancy_with_scratch` and `clear_stale_water_contacts`
            // (`crates/physics/src/water.rs`) take a `world.query::<Ragdoll>()`
            // read guard so a ragdolled actor's limbs get buoyancy contacts
            // too, not just its placement root. Declared here (fixed by
            // PHYS-D6-2026-09-06-03, #3964) so the scheduler's conflict
            // analyzer can see it — this is the fourth time a `Ragdoll`/water
            // storage read landed in this system without a matching
            // declaration update (#1787, #2676, PHYS-D3-2026-08-20-05).
            .reads::<byroredux_physics::Ragdoll>()
            // WATAL Phase 2 — the buoyancy phase reads the water plane
            // components and writes per-body `WaterContact`.
            .reads::<byroredux_core::ecs::components::water::WaterPlane>()
            .reads::<byroredux_core::ecs::components::water::WaterVolume>()
            .reads::<byroredux_core::ecs::components::water::WaterFlow>()
            .reads::<byroredux_core::ecs::components::water::WaterCurrentVolume>()
            .writes::<byroredux_core::ecs::components::water::WaterContact>()
            // #1787 / CONC-D4-01 — the #1698 `BYRO_PROFILE_FALLERS`
            // opt-in diagnostic (`dump_awake_fallers`) reads these
            // three, gated behind an env var + one-shot AtomicBool but
            // still part of the system's true read surface — the
            // analyzer can't see the runtime gate.
            .reads::<byroredux_core::ecs::components::RenderLayer>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads::<byroredux_core::ecs::components::PhysicsSourceForm>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>(),
    );
}
