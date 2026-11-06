//! Animation sampling and root motion, and cinematic animation events.
//!
//! Split out of `player_animation.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage, Transform};
use byroredux_core::string::StringPool;
use crate::systems::make_animation_system;

/// Animation sampling and root motion, and cinematic animation events.
pub(super) fn register_animation_systems(scheduler: &mut Scheduler) {
    scheduler.add_to_with_access(
        Stage::Update,
        make_animation_system(),
        // M27 — animation_system writes the full set of animated-
        // channel storages (every channel a clip may target). The
        // declaration is the UNION across all paths; individual
        // frames touch a subset depending on which clips are
        // playing. See `byroredux/src/systems/animation.rs:305`.
        Access::new()
            .reads_resource::<byroredux_core::animation::AnimationClipRegistry>()
            .writes_resource::<byroredux_core::animation::AnimationClipRegistry>()
            .reads_resource::<crate::components::SubtreeCache>()
            .writes_resource::<crate::components::SubtreeCache>()
            .reads_resource::<crate::components::NameIndex>()
            .writes_resource::<crate::components::NameIndex>()
            .writes_resource::<byroredux_core::string::StringPool>()
            .reads::<byroredux_core::ecs::Name>()
            .reads::<byroredux_core::ecs::Children>()
            .writes::<Transform>()
            .writes::<byroredux_core::animation::RootMotionDelta>()
            .writes::<byroredux_core::animation::RootMotionYaw>()
            .writes::<byroredux_core::ecs::AnimatedVisibility>()
            .writes::<byroredux_core::ecs::AnimatedDiffuseColor>()
            .writes::<byroredux_core::ecs::AnimatedAmbientColor>()
            .writes::<byroredux_core::ecs::AnimatedSpecularColor>()
            .writes::<byroredux_core::ecs::AnimatedEmissiveColor>()
            .writes::<byroredux_core::ecs::AnimatedShaderColor>()
            .writes::<byroredux_core::ecs::AnimatedAlpha>()
            .writes::<byroredux_core::ecs::AnimatedUvTransform>()
            .writes::<byroredux_core::ecs::AnimatedShaderFloat>()
            .writes::<byroredux_core::ecs::AnimatedMorphWeights>()
            .writes::<byroredux_core::ecs::AnimatedTextureFlip>()
            .writes::<byroredux_core::ecs::LightSource>()
            .writes::<byroredux_core::animation::AnimationPlayer>()
            .writes::<byroredux_scripting::events::AnimationTextKeyEvents>()
            .writes::<byroredux_core::animation::AnimationStack>(),
    );
    // Translate clip text keys into Skyrim behavior/Papyrus completion
    // notifications in the same frame, before Late drains the transient
    // AnimationTextKeyEvents components.
    scheduler.add_exclusive(Stage::Update, crate::systems::cinematic_root_motion_system);
    // M47.2 — a graph-driven actor is carried by the root motion its clips extract.
    scheduler.add_exclusive(Stage::Update, crate::systems::graph_root_motion_system);
    // #2391 — the second half of #2269's inverted pair. The body reads
    // text-key events and writes `ActorCinematicState`, then hands the
    // player's events to `dispatch_player_cinematic_animation_event`,
    // which takes `CinematicPresentationState` and then `QuestStageState`
    // — the reverse of the order `quest_fragment_dispatch` establishes.
    // That inversion is safe only because both systems are exclusive;
    // naming both resources here is what makes the contract legible in
    // `sys.accesses` rather than only in prose.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::systems::cinematic_animation_event_system,
        Access::new()
            .reads_resource::<StringPool>()
            .reads_resource::<byroredux_scripting::papyrus_demo::PapyrusPlayerEntity>()
            .writes_resource::<byroredux_scripting::CinematicPresentationState>()
            .writes_resource::<byroredux_scripting::quest_stages::QuestStageState>()
            .reads::<byroredux_scripting::AnimationTextKeyEvents>()
            .writes::<byroredux_scripting::ActorCinematicState>(),
    );
}
