//! R5 follow-up — translation of `DLC2TTR4aPlayerScript.psc`, the
//! canonical "register-for-update, poll-actor-value, fire-once,
//! cancel" pattern.
//!
//! Source: [`docs/r5/source/DLC2TTR4aPlayerScript.psc`](../../../../docs/r5/source/DLC2TTR4aPlayerScript.psc).
//! Sibling to [`super::quest_advance`] (SetStage demo) and the
//! parent module's `defaultRumbleOnActivate` translation; together
//! they cover three of the four major Papyrus pattern families
//! identified in [`docs/r5-evaluation.md`](../../../../docs/r5-evaluation.md).
//!
//! ## The source script in full
//!
//! ```papyrus
//! ScriptName DLC2TTR4aPlayerScript Extends ReferenceAlias
//!
//! Quest Property DLC2TTR4a Auto
//!
//! Event OnInit()
//!   Self.RegisterForUpdate(5 as Float)
//! EndEvent
//!
//! Event OnUpdate()
//!   If Game.GetPlayer().GetActorValue("Variable05") > 0 as Float
//!     DLC2TTR4a.SetStage(200)
//!     Self.UnregisterForUpdate()
//!   EndIf
//! EndEvent
//! ```
//!
//! 13 LOC of actual code (excluding the Champollion-emitted
//! compiler-generated stubs). The pattern:
//!
//! 1. **`OnInit`** — first-frame setup. Subscribes the script to
//!    a 5-second recurring update.
//! 2. **`OnUpdate`** — fires every 5s while the subscription is
//!    alive. Polls a cross-entity stat
//!    (`Game.GetPlayer().GetActorValue("Variable05")`) — Papyrus's
//!    "Variable05" is one of ten free-form quest-script bookkeeping
//!    slots on every Actor; mods abuse them as `bool`s, counters,
//!    timestamp markers. Here it's used as a deferred-trigger flag.
//! 3. **Threshold gate** — when the value crosses zero, do the
//!    work (advance the quest) AND **unsubscribe** so the script
//!    stops polling.
//!
//! This is THE most common pattern Papyrus's `RegisterForUpdate`
//! supports: "watch for a condition to become true, fire once,
//! self-terminate." It's how cleanup-after-quest hooks, late-trigger
//! cinematic launchers, and player-state listeners all work in
//! Skyrim.
//!
//! ## ECS translation choices
//!
//! - **`RegisterForUpdate` / `UnregisterForUpdate`** lower to
//!   inserting / removing the [`crate::RecurringUpdate`] component
//!   on the script's entity. No registry, no subscription table —
//!   the ECS storage IS the registration. Lookup is O(1) via the
//!   sparse-set query.
//! - **`OnUpdate` body** runs in a per-script system queried
//!   against `(RecurringUpdate, OnUpdateEvent, MyScript)`. The
//!   `OnUpdateEvent` marker is what gates the body — it's emitted
//!   by `recurring_update_tick_system` only when the interval has
//!   elapsed; the per-script system doesn't have to know about
//!   timing at all.
//! - **`Game.GetPlayer().GetActorValue("Variable05")`** lowers to a
//!   resource read of [`super::PlayerEntity`] (the player-resolver
//!   already established in `defaultRumbleOnActivate`) + a
//!   component read of [`super::actor_stats::ActorStats`] on that
//!   entity (a small stand-in for the eventual full ActorValue
//!   system; see its module docs).
//! - **`UnregisterForUpdate()` inside the handler body**: the
//!   handler removes the script's own `RecurringUpdate` component
//!   mid-frame. The next frame's tick finds no subscription and
//!   stops emitting `OnUpdateEvent`. Standard ECS pattern; matches
//!   Papyrus's contract exactly (RegisterForUpdate / UnregisterForUpdate
//!   are atomic table mutations in the Papyrus runtime too).
//!
//! ## Why this is per-script, not a generic component
//!
//! Unlike `QuestAdvanceOnActivate` (which generalises across
//! dozens of scripts), the DLC2TTR4a pattern is too one-off to
//! benefit from generalisation:
//!
//! - The `"Variable05"` stat name is hardcoded; a generic shape
//!   would have to take the stat key as a string property + the
//!   comparison + the target stage + the target quest. That's six
//!   fields covering a script-content surface that's small and
//!   varied.
//! - Other "poll-and-fire-once" scripts poll different stats
//!   (`"Health"`, faction rank, quest stage of OTHER quests), use
//!   different comparisons (<=, ==, !=), and fire different
//!   side-effects (start a scene, enable/disable a reference, set
//!   an Int property). Generalising covers ~5% additional cases at
//!   ~100% additional surface.
//! - M47.2's transpiler can emit per-script components like this
//!   one trivially: parse the OnUpdate body → emit a tiny system
//!   that matches it. The generic shape only pays off when ≥10
//!   scripts share the same body modulo constants; the DA10 family
//!   is in that regime, this isn't.
//!
//! The take-away for the transpiler design: there are two
//! emission strategies, and both are valid:
//!
//! 1. **Pattern-match the script body** against a small catalogue
//!    of known shapes (`QuestAdvanceOnActivate`,
//!    `RumbleOnActivate`, ...) and populate the generic component
//!    when the script fits. ~70% of vanilla scripts.
//! 2. **Emit a per-script component + system** when the script
//!    doesn't fit any catalogued shape. The remaining ~30%. The
//!    transpiler's job is the AST→Rust translation of the OnUpdate
//!    body; we already have all the primitives.
//!
//! Both produce idiomatic ECS code — the first is more compact at
//! ten cards in the catalogue covering most of the corpus, the
//! second is fall-through for everything else.

use super::actor_stats::ActorStats;
use super::PlayerEntity;
use crate::quest_stages::{QuestFormId, QuestStageState};
use crate::recurring_update::{OnUpdateEvent, RecurringUpdate};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::world::World;

/// Translation of `DLC2TTR4aPlayerScript`'s `Quest Property
/// DLC2TTR4a Auto` + the implicit state held by the
/// register/unregister/OnUpdate lifecycle.
///
/// The instance carries the quest FormID the script was bound to
/// at edit time (Papyrus's `Quest Property` auto-resolves at
/// script-attach via the ESM's `VMAD` subrecord). The
/// `RegisterForUpdate` subscription state is NOT held in this
/// component — it lives on the same entity as a separate
/// [`RecurringUpdate`] component, mirroring the Papyrus separation
/// (one table for subscriptions, separate per-script state).
#[derive(Debug, Clone, Copy)]
pub struct Dlc2Ttr4aPlayerScript {
    /// Resolved at script-attach time from
    /// `Quest Property DLC2TTR4a Auto`. Papyrus's runtime walks the
    /// VMAD property block once when the alias becomes active; we
    /// take the resolved FormID as a constructor argument and
    /// stash it here.
    pub dlc2_ttr4a_quest: QuestFormId,
    /// Papyrus stat name the OnUpdate body polls. Hardcoded in the
    /// source (`"Variable05"`); kept as a field rather than a
    /// const so test fixtures can drive different stat keys without
    /// editing the source.
    pub poll_actor_value: &'static str,
    /// Threshold the polled value must cross (strictly greater
    /// than). Hardcoded `0.0` in the source. Kept as a field for
    /// the same reason as `poll_actor_value`.
    pub threshold: f32,
    /// Stage to advance the owning quest to when the threshold
    /// crosses. Hardcoded `200` in the source.
    pub on_satisfied_stage: u16,
}

impl Component for Dlc2Ttr4aPlayerScript {
    type Storage = SparseSetStorage<Self>;
}

/// Construct the DLC2TTR4a script with constants exactly matching
/// the .psc source. The transpiler emits an equivalent factory for
/// every per-script component it generates.
pub fn dlc2_ttr4a_player_script(quest: QuestFormId) -> Dlc2Ttr4aPlayerScript {
    Dlc2Ttr4aPlayerScript {
        dlc2_ttr4a_quest: quest,
        poll_actor_value: "Variable05",
        threshold: 0.0,
        on_satisfied_stage: 200,
    }
}

/// Translation of the `OnInit()` event handler — the one-frame
/// initialisation that subscribes the script to a 5-second
/// recurring update.
///
/// Single-shot: runs once when the script's entity gains a
/// `Dlc2Ttr4aPlayerScript` component without a `RecurringUpdate`
/// already attached. The "subscribed-once" condition substitutes
/// for Papyrus's `OnInit` lifecycle event firing exactly once per
/// alias instance.
///
/// Concretely: idempotent — calling it on every frame is safe
/// because subsequent invocations short-circuit on `RecurringUpdate`
/// already present.
pub fn dlc2_ttr4a_on_init_system(world: &World) {
    let Some(scripts) = world.query::<Dlc2Ttr4aPlayerScript>() else {
        return;
    };
    let Some(updates) = world.query::<RecurringUpdate>() else {
        return;
    };
    let mut to_subscribe: Vec<byroredux_core::ecs::storage::EntityId> = Vec::new();
    for (entity, _script) in scripts.iter() {
        if updates.get(entity).is_none() {
            to_subscribe.push(entity);
        }
    }
    drop(updates);
    drop(scripts);
    if to_subscribe.is_empty() {
        return;
    }
    let Some(mut updates_mut) = world.query_mut::<RecurringUpdate>() else {
        return;
    };
    for entity in to_subscribe {
        // Papyrus: `Self.RegisterForUpdate(5 as Float)`. The
        // 5-second cadence is hardcoded in the source.
        updates_mut.insert(entity, RecurringUpdate::every(5.0));
    }
}

/// Translation of the `OnUpdate()` event handler body. Runs on
/// every frame an `OnUpdateEvent` marker is present on a
/// `Dlc2Ttr4aPlayerScript`-bearing entity.
///
/// Implements (lowered):
///
/// ```text
/// If Game.GetPlayer().GetActorValue("Variable05") > 0.0
///   DLC2TTR4a.SetStage(200)
///   Self.UnregisterForUpdate()
/// EndIf
/// ```
pub fn dlc2_ttr4a_on_update_system(world: &World) {
    let player = world.resource::<PlayerEntity>().0;

    // Collect the entities whose threshold predicate holds — we
    // can't write to QuestStageState or remove RecurringUpdate
    // while the read borrows are live, so this is the standard
    // collect-then-apply two-phase.
    let mut to_advance: Vec<(byroredux_core::ecs::storage::EntityId, QuestFormId, u16)> = Vec::new();
    {
        let Some(events) = world.query::<OnUpdateEvent>() else {
            return;
        };
        let Some(scripts) = world.query::<Dlc2Ttr4aPlayerScript>() else {
            return;
        };
        let player_stats = world.query::<ActorStats>();
        for (entity, _ev) in events.iter() {
            let Some(script) = scripts.get(entity) else {
                continue;
            };
            // Game.GetPlayer().GetActorValue("Variable05").
            let stat_value = player_stats
                .as_ref()
                .and_then(|q| q.get(player))
                .map(|s| s.get(script.poll_actor_value))
                .unwrap_or(0.0);
            // `> 0.0` predicate from the source.
            if stat_value > script.threshold {
                to_advance.push((entity, script.dlc2_ttr4a_quest, script.on_satisfied_stage));
            }
        }
    }

    if to_advance.is_empty() {
        return;
    }

    // Write phase 1 — quest state advance.
    {
        let mut stage_state = world.resource_mut::<QuestStageState>();
        for (_entity, quest, stage) in &to_advance {
            stage_state.set_stage(*quest, *stage);
        }
    }

    // Write phase 2 — UnregisterForUpdate. Remove the
    // RecurringUpdate component from every script that advanced.
    // The next tick won't find a subscription to count down and
    // the script effectively self-terminates.
    {
        let Some(mut updates) = world.query_mut::<RecurringUpdate>() else {
            return;
        };
        for (entity, _, _) in to_advance {
            updates.remove(entity);
        }
    }
}

pub fn register(world: &mut World) {
    world.register::<Dlc2Ttr4aPlayerScript>();
}

#[cfg(test)]
mod tests;
