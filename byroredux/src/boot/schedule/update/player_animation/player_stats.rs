//! Player derived stats and pool regeneration.
//!
//! Split out of `player_animation.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// Player derived stats and pool regeneration.
pub(super) fn register_player_stats_systems(scheduler: &mut Scheduler) {
    // The race chosen in character creation back onto the player's
    // `Background`, where every race reader looks (it is not saved; the
    // identity is).
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::systems::player_identity_sync_system,
        Access::new()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads::<byroredux_core::character::CharacterIdentity>()
            .writes::<byroredux_core::character::Background>(),
    );
    // #5039 — refresh the player's stamped PlayerOnly pools from their
    // current inputs before regen reads them. Exclusive, like its neighbour
    // below: it takes the `CharacterRuleset` → `ActorValues` pair.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::systems::player_derived_stats_system,
        Access::new()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads::<byroredux_core::character::CharacterLevel>()
            .reads_resource::<byroredux_core::character::CharacterRuleset>()
            .writes::<byroredux_core::ecs::components::ActorValues>(),
    );
    // CHARAL pool regen (Fatigue/Magicka) — a fixed 60 Hz tick decoupled
    // from the variable frame rate, mirroring `physics_sync_system`'s
    // accumulator (`crates/core/src/character/regen.rs`).
    //
    // Gated on TWO resources (#2950): `PoolRegenAccumulator`, inserted
    // unconditionally in `build_world`, and `PoolRegenConfig`, which is
    // per-game. FO3/FNV install it with an `ActionPoints` lane (#6350,
    // `build_vats_model`), so the tick restores the player's AP there; every
    // other game never inserts it and the system returns at its second line
    // (Oblivion's Fatigue/Magicka lanes wait on a pre-AVIF resolver, #3768).
    // #2391 / ECS-D5B-03 — declared via `add_exclusive_with_access` (the
    // #1236 channel, previously unused in production). This system is
    // #2153's site: it builds a 3-deep hold stack (`PoolRegenConfig` read
    // held across `PoolRegenAccumulator` write, then `CharacterRuleset`
    // read, then the `ActorValues` write query), and its safety rests
    // entirely on being scheduled exclusive. The analyzer doesn't pair
    // exclusives, so this declaration surfaces no conflict row — its job
    // is to put the disputed types on the `sys.accesses` report instead
    // of leaving a blank row where the dispute actually is.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        byroredux_core::character::pool_regen_tick_system,
        Access::new()
            .reads_resource::<byroredux_core::character::PoolRegenConfig>()
            .writes_resource::<byroredux_core::character::PoolRegenAccumulator>()
            .reads_resource::<byroredux_core::character::CharacterRuleset>()
            .reads::<byroredux_core::character::CharacterLevel>()
            .writes::<byroredux_core::ecs::components::ActorValues>(),
    );
}
