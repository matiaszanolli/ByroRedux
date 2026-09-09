//! `Stage::Early` registrations (#3855, split from `boot.rs`).

use byroredux_core::ecs::{Access, ActiveCamera, Scheduler, Stage, TotalTime, Transform};

use crate::components::InputState;
use crate::interaction::{ActionBindings, ActionState, InjectedKeyHold, InjectedKeyPulse};
use crate::systems::weather_system;

/// `Stage::Early` registrations (#3739 split of `build_scheduler`).
pub(super) fn register_early_systems(scheduler: &mut Scheduler) {
    // M27 Phase 3 — `fly_camera_system` and `character_controller_system`
    // are runtime-mutually-exclusive (each early-returns on the
    // wrong `PlayerMode`), so the scheduler's access analyzer
    // paired them up and surfaced a Transform + PhysicsWorld
    // WriteWrite conflict that's structurally impossible at
    // runtime. `player_controller_system` dispatches to one of
    // the two inner systems per frame; declared accesses are the
    // union of both inner systems' accesses.
    scheduler.add_to_with_access(
        Stage::Early,
        crate::systems::player_controller_system,
        Access::new()
            .reads_resource::<crate::systems::PlayerMode>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<InputState>()
            .reads_resource::<ActionBindings>()
            .reads_resource::<ActionState>()
            .writes_resource::<ActionState>()
            .writes_resource::<crate::combat::PendingDeathReconciliations>()
            .writes_resource::<InjectedKeyPulse>()
            .writes_resource::<InjectedKeyHold>()
            .reads_resource::<byroredux_scripting::PlayerControlState>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .writes_resource::<byroredux_physics::PhysicsWorld>()
            // #1787 / CONC-D4-01 — the character controller snapshots
            // `ContactConfig::kcc_offset_bu` once per tick
            // (systems/character.rs); read-only, declared for the same
            // reason as the `physics_sync_system` sibling gap.
            .reads_resource::<byroredux_physics::ContactConfig>()
            .reads_resource::<TotalTime>()
            .reads_resource::<byroredux_core::ecs::components::groundcover::WindField>()
            .reads::<byroredux_physics::CharacterController>()
            .writes::<byroredux_physics::CharacterController>()
            .reads::<byroredux_physics::RapierHandles>()
            .reads::<byroredux_scripting::ActorControlState>()
            .reads::<byroredux_core::ecs::components::ActorVitals>()
            .reads::<byroredux_core::ecs::components::water::WaterPlane>()
            .reads::<byroredux_core::ecs::components::water::WaterVolume>()
            .reads::<byroredux_core::ecs::components::water::WaterFlow>()
            // WATAL W1 — the character system owns the player's own
            // `WaterContact` row (`sync_player_water_contact`): the dynamic
            // buoyancy pass selects `MotionType::Dynamic` plus ragdoll bones
            // and structurally cannot see a kinematic capsule. It reads last
            // frame's row back to recover the swim-exit transition, so both
            // halves are declared.
            .reads::<byroredux_core::ecs::components::water::WaterContact>()
            .writes::<byroredux_core::ecs::components::water::WaterContact>()
            .writes::<byroredux_core::ecs::components::ActorValues>()
            .writes::<byroredux_core::ecs::components::Dead>()
            .reads::<Transform>()
            .writes::<Transform>(),
    );
    // #3111 — weather writes WindField while the player controller samples it
    // for water waves. Keep weather in the same stage but serialize it after
    // the parallel batch so the access analyzer and runtime agree: there is no
    // same-stage read/write race, and the controller sees one stable snapshot.
    scheduler.add_exclusive_with_access(
        Stage::Early,
        weather_system,
        Access::new()
            .reads_resource::<crate::components::WeatherDataRes>()
            .writes_resource::<crate::components::WeatherDataRes>()
            .reads_resource::<crate::components::WeatherTransitionRes>()
            .writes_resource::<crate::components::WeatherTransitionRes>()
            .writes_resource::<crate::components::GameTimeRes>()
            .reads_resource::<crate::components::CellLightingRes>()
            .writes_resource::<crate::components::CellLightingRes>()
            .reads_resource::<byroredux_core::ecs::components::groundcover::WindField>()
            .writes_resource::<byroredux_core::ecs::components::groundcover::WindField>()
            .writes_resource::<crate::components::SkyParamsRes>()
            .writes_resource::<crate::components::CloudSimState>(),
    );
    scheduler.add_to_with_access(
        Stage::Early,
        byroredux_scripting::timer_tick_system,
        Access::new()
            .writes::<byroredux_scripting::ScriptTimer>()
            .writes::<byroredux_scripting::TimerExpired>(),
    );
}
