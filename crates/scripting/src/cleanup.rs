//! Event cleanup system — removes transient marker components.
//!
//! Runs at the end of each frame to clear event markers, ensuring
//! events are only visible for one frame. This is the ECS equivalent
//! of "clearing the event queue."
//!
//! # Marker-lifetime house rules (#2672)
//!
//! This module doc used to say every transient marker is drained here. It
//! is not, and never was: a sweep of the crate's `impl Component for` types
//! finds **two** sanctioned patterns, both legitimate, neither written down
//! anywhere authoritative. That ambiguity is not hypothetical — a future
//! marker author reading this file concluded registration was mandatory
//! while one reading a self-draining consumer concluded it was optional,
//! and nothing adjudicated. Marker-lifecycle defects have already been
//! filed against this crate on exactly that seam.
//!
//! **Pattern A — register with `event_cleanup_system`** (the list below).
//! For a marker with *no single owning consumer*: anything a re-evaluating
//! system might observe, or that several systems read in the same frame.
//! `event_cleanup_system` is the last system scheduled overall
//! (`byroredux/src/boot.rs`, `Stage::Late`), so a Pattern-A marker is
//! visible to every system in the frame it was raised and to none in the
//! next.
//!
//! **Pattern B — drain at the head of your own consumer.** For a marker
//! with *exactly one* owning system, which snapshots and clears it in the
//! same pass. This is the stronger guarantee where it applies — the marker
//! cannot outlive the system that owns it even by a frame, and it is
//! self-evidently correct at the drain site rather than depending on a
//! registration in another file. The obligation: the drain must be
//! **unconditional** — no early return may sit between the top of the
//! system and the drain, or the marker is stranded and its consumer
//! re-fires forever.
//!
//! Live Pattern-B markers, each verified to drain before any early return:
//!
//! | Marker | Owning consumer |
//! |---|---|
//! | `SceneStartRequest`, `SceneStopRequest`, `SceneActionCompletionBatch` | `scene::playback::scene_playback_system` |
//! | `DialoguePresentationEventBatch`, `DialogueLineCompletionBatch` | `dialogue::scene_dialogue_system` |
//! | `ScenePackageEventBatch`, `ScenePackageCompletionBatch`, `EvaluatePackageRequest` | `package::scene_package_system` |
//! | `TwoStateTransitionBatch` | `vm_state::two_state_activator_system` |
//! | `MotionTypeChangeRequest` | `byroredux::systems::cinematic` (the one tail-drain — it removes exactly the entities it snapshotted, after an empty-set early return that strands nothing) |
//!
//! Everything else is **persistent state**, not a marker, and belongs to
//! neither pattern: playback/plan components (`ScenePlayer`,
//! `DialoguePlayback`, `ScenePackagePlayback`), per-entity script state
//! (`ScriptVariables`, `ScriptTimer`, `TwoStateActivator`), alias/candidate
//! stamps (`SceneAliasCandidate`, `QuestAliasRuntimeOverlays`), and
//! subscriptions (`RecurringUpdate`, removed by the script's own
//! `UnregisterFor*` logic).
//!
//! Adding a marker? Pick a pattern and say which in its docstring. If it has
//! one owning system, prefer B and drain at the top of it. Otherwise add it
//! to `event_cleanup_system` below.

use crate::events::{
    ActivateEvent, AnimationTextKeyEvents, EquipmentEventBatch, HitEvent, OnCellLoadEvent,
    OnInitEvent, OnTriggerEnterEvent, RippleEvent, SplashEvent, TimerExpired,
};
use crate::papyrus_demo::mg07_door::UiMessageCommand;
use crate::papyrus_demo::{CameraShakeCommand, ControllerRumbleCommand};
use crate::quest_stages::QuestStageAdvancedBatch;
use crate::recurring_update::OnUpdateEvent;
use crate::scene::{SceneEventBatch, SceneFragmentInvocationBatch};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;

/// System: remove all transient event marker components.
///
/// Must be registered as the LAST system in the scheduler so all
/// gameplay systems have a chance to process events before cleanup.
///
/// This is Pattern A of the two documented in the module doc above: a
/// marker with no single owning consumer, visible for exactly one frame.
/// Markers whose lifetime is owned end-to-end by one system use Pattern B
/// instead and are drained there, not here — see the table above for which
/// is which, and add new markers to one list or the other rather than
/// leaving the choice implicit (#2672).
///
/// Subscriptions (e.g. [`crate::RecurringUpdate`]) belong to neither —
/// they outlive individual frames and are removed by the script's own
/// `UnregisterFor*` logic.
pub fn event_cleanup_system(world: &World, _dt: f32) {
    drain_component::<ActivateEvent>(world);
    drain_component::<HitEvent>(world);
    drain_component::<SplashEvent>(world);
    drain_component::<RippleEvent>(world);
    drain_component::<TimerExpired>(world);
    drain_component::<AnimationTextKeyEvents>(world);
    // R5 prototype additions — all transient-by-design markers.
    drain_component::<OnUpdateEvent>(world);
    drain_component::<QuestStageAdvancedBatch>(world);
    drain_component::<CameraShakeCommand>(world);
    drain_component::<ControllerRumbleCommand>(world);
    drain_component::<UiMessageCommand>(world);
    drain_component::<SceneEventBatch>(world);
    drain_component::<SceneFragmentInvocationBatch>(world);
    // Canonical markers — all one-frame transients. Each has an engine emit
    // site: OnInitEvent from provider-program attachment, OnTriggerEnterEvent
    // from `trigger_detection_system` (M47.2), OnCellLoadEvent from the cell
    // loader's `attach_script_for_refr`, EquipmentEventBatch from the equip
    // pipeline. Without draining, a re-evaluating consumer (e.g.
    // `quest_advance_system`) re-fires every frame.
    drain_component::<OnTriggerEnterEvent>(world);
    drain_component::<OnCellLoadEvent>(world);
    drain_component::<OnInitEvent>(world);
    drain_component::<EquipmentEventBatch>(world);
}

/// Regression for #2672. The module doc's two-pattern contract is only
/// worth having if it stays true, and both halves of it are checkable from
/// source: every `drain_component::<T>` below must appear in the Pattern-A
/// prose, and every marker the doc lists as Pattern-B must actually be
/// drained by the consumer it names.
#[cfg(test)]
mod contract_tests {
    /// The module doc and the system body are two lists of the same set.
    /// Pre-#2672 the doc claimed the drain list covered *every* transient
    /// marker, which was never true and could not be checked; the claim it
    /// makes now can be.
    #[test]
    fn every_drained_marker_is_a_documented_pattern_a_marker() {
        let src = include_str!("cleanup.rs");
        let doc: String = src
            .lines()
            .take_while(|line| line.starts_with("//!"))
            .collect::<Vec<_>>()
            .join("\n");
        let drained: Vec<&str> = src
            .match_indices("drain_component::<")
            .map(|(index, needle)| {
                let rest = &src[index + needle.len()..];
                &rest[..rest.find('>').expect("closing angle bracket")]
            })
            .collect();
        assert!(drained.len() >= 16, "found only {} drains", drained.len());
        for marker in &drained {
            assert!(
                !doc.contains(&format!("`{marker}`")),
                "`{marker}` is drained by `event_cleanup_system` (Pattern A) \
                 but the module doc lists it under Pattern B or as \
                 persistent state — the two contradict each other (#2672)"
            );
        }
    }

    /// Each Pattern-B marker must really be drained by the consumer the doc
    /// names. A marker that silently moved to `event_cleanup_system`, or
    /// lost its drain entirely, would leave the table describing a contract
    /// the code no longer implements — the exact rot #2672 filed.
    #[test]
    fn every_documented_pattern_b_marker_drains_in_its_named_consumer() {
        // `drain_site` is the exact call the consumer makes. Most use the
        // shared `drain::<T>` helper; `vm_state` hand-rolls the same removal
        // in a named `drain_transitions`, which is why the needle is per-row
        // rather than derived from the marker name.
        for (marker, consumer_src, drain_site) in [
            (
                "SceneStartRequest",
                include_str!("scene/playback.rs"),
                "drain::<SceneStartRequest>",
            ),
            (
                "SceneStopRequest",
                include_str!("scene/playback.rs"),
                "drain::<SceneStopRequest>",
            ),
            (
                "SceneActionCompletionBatch",
                include_str!("scene/playback.rs"),
                "drain::<SceneActionCompletionBatch>",
            ),
            (
                "DialoguePresentationEventBatch",
                include_str!("dialogue.rs"),
                "drain::<DialoguePresentationEventBatch>",
            ),
            (
                "DialogueLineCompletionBatch",
                include_str!("dialogue.rs"),
                "drain::<DialogueLineCompletionBatch>",
            ),
            (
                "ScenePackageEventBatch",
                include_str!("package.rs"),
                "drain::<ScenePackageEventBatch>",
            ),
            (
                "ScenePackageCompletionBatch",
                include_str!("package.rs"),
                "drain::<ScenePackageCompletionBatch>",
            ),
            (
                "EvaluatePackageRequest",
                include_str!("package.rs"),
                "drain::<EvaluatePackageRequest>",
            ),
            (
                "TwoStateTransitionBatch",
                include_str!("vm_state.rs"),
                "query_mut::<TwoStateTransitionBatch>",
            ),
        ] {
            assert!(
                consumer_src.contains(drain_site),
                "`{marker}` is documented as Pattern B (self-draining in its \
                 own consumer) but that consumer no longer drains it — \
                 either restore the drain or move it to \
                 `event_cleanup_system` and update the module doc (#2672)"
            );
            assert!(
                !include_str!("cleanup.rs").contains(&format!("drain_component::<{marker}>")),
                "`{marker}` is drained BOTH by its consumer and by \
                 `event_cleanup_system` — pick one pattern (#2672)"
            );
        }
    }
}

/// Remove all instances of a component type from every entity.
fn drain_component<T: byroredux_core::ecs::storage::Component>(world: &World) {
    let Some(mut query) = world.query_mut::<T>() else {
        return;
    };
    let entities: Vec<EntityId> = query.iter().map(|(id, _)| id).collect();
    for entity in entities {
        query.remove(entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{
        ActivateEvent, EquipmentChange, EquipmentEventBatch, HitEvent, OnCellLoadEvent,
        OnInitEvent, OnTriggerEnterEvent, RippleEvent, SplashEvent, TimerExpired,
    };
    use crate::scene::{
        SceneEvent, SceneEventBatch, SceneFragmentInvocation, SceneFragmentInvocationBatch,
    };
    use byroredux_core::ecs::world::World;
    use byroredux_plugin::esm::records::script_instance::SceneFragmentEvent;

    fn setup_world() -> World {
        let mut world = World::new();
        crate::register(&mut world);
        world
    }

    #[test]
    fn cleanup_removes_all_event_types() {
        let mut world = setup_world();
        let a = world.spawn();
        let b = world.spawn();
        let c = world.spawn();

        world.insert(a, ActivateEvent { activator: 99 });
        world.insert(
            b,
            HitEvent {
                aggressor: 1,
                source: 2,
                projectile: 3,
                damage: 4.0,
                power_attack: false,
                sneak_attack: false,
                bash_attack: false,
                blocked: false,
            },
        );
        world.insert(c, TimerExpired { timer_id: 5 });

        // M47.0 Phase 5 canonical markers — must drain in lockstep with
        // the legacy trio, else a re-evaluating consumer re-fires forever.
        let d = world.spawn();
        let e = world.spawn();
        let f = world.spawn();
        let init = world.spawn();
        world.insert(
            d,
            OnTriggerEnterEvent {
                triggerers: vec![a],
            },
        );
        world.insert(e, OnCellLoadEvent);
        world.insert(init, OnInitEvent);
        world.insert(
            f,
            EquipmentEventBatch(vec![EquipmentChange {
                item_form_id: 0x1234,
                equipped: true,
            }]),
        );
        let g = world.spawn();
        world.insert(
            g,
            SplashEvent {
                actor: a,
                intensity: 1.0,
                position: [0.0; 3],
            },
        );
        world.insert(
            g,
            RippleEvent {
                actor: a,
                intensity: 0.5,
                position: [0.0; 3],
            },
        );
        world.insert(g, SceneEventBatch(vec![SceneEvent::SceneStarted]));
        world.insert(
            g,
            SceneFragmentInvocationBatch(vec![SceneFragmentInvocation {
                scene_form_id: 1,
                event: SceneFragmentEvent::Begin,
                script_name: "SF_Test".to_owned(),
                fragment_name: "Fragment_0".to_owned(),
            }]),
        );

        event_cleanup_system(&world, 0.0);

        assert!(!world.has::<ActivateEvent>(a));
        assert!(!world.has::<HitEvent>(b));
        assert!(!world.has::<TimerExpired>(c));
        assert!(!world.has::<OnTriggerEnterEvent>(d));
        assert!(!world.has::<OnCellLoadEvent>(e));
        assert!(!world.has::<OnInitEvent>(init));
        assert!(!world.has::<EquipmentEventBatch>(f));
        assert!(!world.has::<SplashEvent>(g));
        assert!(!world.has::<RippleEvent>(g));
        assert!(!world.has::<SceneEventBatch>(g));
        assert!(!world.has::<SceneFragmentInvocationBatch>(g));
    }

    #[test]
    fn cleanup_preserves_non_event_components() {
        use byroredux_core::ecs::components::Transform;

        let mut world = setup_world();
        let e = world.spawn();
        world.insert(e, Transform::IDENTITY);
        world.insert(e, ActivateEvent { activator: 1 });

        event_cleanup_system(&world, 0.0);

        assert!(!world.has::<ActivateEvent>(e));
        assert!(world.has::<Transform>(e));
    }
}
