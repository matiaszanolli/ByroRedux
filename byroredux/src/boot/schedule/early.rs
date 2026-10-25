//! `Stage::Early` registrations (#3855, split from `boot.rs`).

use byroredux_core::ecs::{
    Access, ActiveCamera, GlobalTransform, Scheduler, Stage, TotalTime, Transform,
};

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
            // #6350 — the VATS suppression gates inside
            // `player_can_act` / `player_accepts_movement_input`.
            .reads_resource::<crate::components::VatsState>()
            // M47.2 — over-encumbrance's movement effect.
            .reads_resource::<crate::encumbrance::PlayerEncumbrance>()
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
            .reads::<byroredux_core::ecs::components::water::WaterSurfaceMesh>()
            .reads::<byroredux_core::ecs::components::water::WaterFlow>()
            // #3974 — `player_water_state` falls back to a placed
            // `WaterCurrentVolume` marker when the plane has no flow of
            // its own (the dynamic path's XWCU+XPRM rapids arm).
            .reads::<byroredux_core::ecs::components::water::WaterCurrentVolume>()
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
            // #5641 — the drowning death raises the KILL story event
            // (#5434, `raise_kill_story_event`): the `StoryEvent` marker
            // write plus the `resolve_current_lctn` reads of the cell
            // index / current cell context. Declared so the access analyzer
            // sees them against `npc_dialogue`'s and the dispatcher's rows.
            .writes::<byroredux_scripting::story_manager::StoryEvent>()
            .writes::<byroredux_scripting::OnDeathEvent>()
            .reads_resource::<crate::cell_loader::LoadedCellIndex>()
            .reads_resource::<crate::cell_loader::CurrentCellContext>()
            .reads_resource::<crate::cell_loader::CurrentExteriorContext>()
            // #6217 — `raise_death_event` → the scripting crate's `note_death`
            // (d1b298921): the victim's kill record and the reads that name it
            // (`entity_global_form_id`'s player check, FormID and pool). Its
            // player-killer branch (misc stats, challenges) cannot run here:
            // drowning has no killer.
            .writes_resource::<byroredux_scripting::obscript_state::KillRecords>()
            .reads::<byroredux_scripting::SceneAliasCandidate>()
            .reads::<byroredux_core::ecs::components::CreatureActor>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads_resource::<byroredux_scripting::papyrus_demo::PapyrusPlayerEntity>()
            .reads::<Transform>()
            .writes::<Transform>()
            // `fly_camera_system` publishes its pose to `GlobalTransform`
            // immediately (see systems/camera.rs) instead of waiting for
            // next frame's propagation pass, since same-frame consumers
            // (interaction, picking, audio, debug camera commands) read
            // the global pose before propagation runs again.
            .writes::<GlobalTransform>(),
    );
    // #6350 — VATS enter/exit on the key edge. EXCLUSIVE: the mode
    // state is the freeze switch the whole frame reads, and
    // `player_controller_system`'s VATS suppression gates read it too —
    // a parallel write would conflict with every reader. Exclusive Early
    // also runs after the parallel batch, i.e. after this frame's
    // `ActionState` edges exist.
    scheduler.add_exclusive_with_access(
        Stage::Early,
        crate::systems::vats::vats_mode_system,
        Access::new()
            .reads_resource::<crate::interaction::ActionState>()
            .writes_resource::<crate::components::VatsState>()
            .reads_resource::<byroredux_core::character::VatsModel>()
            .reads_resource::<crate::systems::PlayerMode>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads_resource::<byroredux_scripting::input_layers::InputLayers>(),
    );
    // #3111 — weather writes WindField while the player controller samples it
    // for water waves. Keep weather in the same stage but serialize it after
    // the parallel batch so the access analyzer and runtime agree: there is no
    // same-stage read/write race, and the controller sees one stable snapshot.
    // #4186 — "stable" means *last frame's*: the exclusive phase runs after
    // Early's parallel batch, so `player_controller_system` (parallel) reads
    // the WindField `weather_system` wrote on the previous frame. Accepted
    // cross-stage lag — wave-scroll/wind-wave scale change over
    // seconds-to-minutes timescales — and structurally invisible to
    // `analyze_pair`, which never compares a parallel entry against an
    // exclusive one (#3653 is the documented-and-pinned precedent). Any new
    // Early-parallel WindField consumer silently inherits the same lag.
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
            .writes_resource::<crate::components::CloudSimState>()
            // #4416 — publishes the exterior's base image space.
            .writes_resource::<byroredux_scripting::ImageSpaceBase>(),
    );
    scheduler.add_to_with_access(
        Stage::Early,
        byroredux_scripting::timer_tick_system,
        Access::new()
            .writes::<byroredux_scripting::ScriptTimer>()
            .writes::<byroredux_scripting::TimerExpired>(),
    );
}
