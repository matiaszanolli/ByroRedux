//! AI-package behavior tagging (M42.1–M42.8).
//!
//! Split out of `npc_spawn.rs` under #2198 / TD1-NEW-03: that file re-crossed
//! 2000 LOC purely from this dispatcher's arrival, with no single function
//! newly oversized. Extracted as its own module both to get the parent back
//! under threshold and to leave room for the ~10 AI procedures that still
//! have no runtime — each one adds an arm here, not to the spawn path.
//!
//! Mirrors the per-family module layout the runtimes themselves already use
//! (`systems/{sandbox,wander,travel,follow,escort,guard,patrol}.rs`): this is
//! the *selection* half — which Behavior component an NPC gets — and those
//! are the *execution* half that consumes it.
//!
//! Pure code movement; no behavior change. The dispatcher's own tests stay in
//! the parent's test module, calling it through the re-export there.

use super::{EsmIndex, NpcRecord};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;

/// Whether an AI package's CTDA conditions permit it to be selected for this
/// actor (M42.2), evaluated through the M47.1 condition evaluator.
///
/// **Fail-open on unimplemented functions.** The M47.1 catalog covers ~15
/// of Bethesda's ~300 condition functions; an out-of-catalog index
/// evaluates to `0.0` in `evaluate_function`, which would silently turn a
/// `SomeUnimplementedFunc == 1` gate into a *false* result and drop a
/// package the engine can't actually reason about — a regression versus
/// M42.1, where conditions were ignored entirely and every scheduled
/// package stayed eligible. So if *any* condition in the list references a
/// function outside the catalog, the whole list is treated as passing (the
/// OR/AND blocks mix, so one unknown poisons the boolean). Only lists whose
/// every function is implemented are gated for real — honoring the common,
/// evaluable cases (`GetIsID` / `GetActorValue` / `GetFactionRank` /
/// `GetStage`) without regressing on the rest. Empty lists pass (Bethesda's
/// "no conditions = always fires", also short-circuited in `evaluate`).
fn package_conditions_pass(
    conditions: &byroredux_plugin::esm::records::condition::ConditionList,
    world: &byroredux_core::ecs::World,
    ctx: &byroredux_scripting::condition::ConditionContext,
) -> bool {
    use byroredux_scripting::condition::{evaluate, ConditionFunction};
    if conditions.is_empty() {
        return true;
    }
    let all_known = conditions.iter().all(|c| {
        !matches!(
            ConditionFunction::from_index(c.function_index),
            ConditionFunction::Unknown(_)
        )
    });
    if !all_known {
        return true; // can't evaluate soundly → don't gate (fail-open)
    }
    evaluate(conditions, world, ctx)
}

/// Behavior tagging (M42.1-M42.8). Tags `placement_root` with the ECS
/// Behavior component matching the NPC's *active* package at the current
/// game hour — e.g. `sandbox_seat_system` only seats NPCs whose active
/// package is Sandbox-type, not day-shift workers whose Sandbox package
/// is a low-priority evening fallback (stops the saloon bartender's
/// 08:00-20:00 `AtBar` package from being overridden and dragging them
/// to a table at 10:00). `ai_packages` (PKID refs, priority order)
/// resolve through `index.packages`; `active_package` walks them in
/// order and returns the first package scheduled-active at `hour` whose
/// CTDA conditions pass (M42.2).
///
/// Shared by [`spawn_npc_entity`] and [`spawn_prebaked_npc_entity`]
/// (#2052 / TD1-003 — the pre-baked path previously had no AI-package
/// gating at all, a divergence from the kf-era path with no reason to
/// exist since both paths spawn the same NPC record onto the same
/// placement root).
///
/// #2031 / PERF-D7-01 — resolves the active package ONCE (was 14
/// separate `active_package_is_*`/`active_*_location`/`active_*_target`
/// calls, one pair per procedure, each independently re-running this
/// walk — which re-evaluates every rejected package's CTDA conditions
/// via the M47.1 scripting evaluator). An NPC's active package is a
/// single winning `PackRecord` by construction (confirmed by
/// `AUDIT_ECS_2026-07-16.md`), so every one of those 14 calls against
/// the same `(packages, hour, condition_met)` converged on the same
/// package anyway; matching its `procedure_type` once builds the one
/// Behavior component the old code would have built directly, with no
/// behavior change.
pub(super) fn apply_ai_package_behavior(
    world: &mut World,
    placement_root: EntityId,
    npc: &NpcRecord,
    index: &EsmIndex,
) {
    let game_hour = world
        .try_resource::<crate::components::GameTimeRes>()
        .map(|r| r.hour)
        .unwrap_or(10.0);
    // M42.2: CTDA gating. A package is eligible only if its author-set
    // conditions pass, evaluated against this actor via the M47.1 evaluator.
    // The predicate closure is what threads scripting's evaluator into
    // plugin's schedule-only selector (plugin can't call scripting — the
    // dependency runs the other way). `subject` is the spawned NPC; the
    // package `target` isn't resolved (v0, matching the search-center
    // approximation below), so conditions using `RunOn::Target` fall to the
    // evaluator's missing-ref-fails default.
    let cond_ctx = byroredux_scripting::condition::ConditionContext {
        subject: placement_root,
        target: None,
        combat_target: None,
        linked_reference: None,
        quest: None,
    };
    let condition_met = |pk: &byroredux_plugin::esm::records::PackRecord| {
        package_conditions_pass(&pk.conditions, world, &cond_ctx)
    };
    // Single resolve (borrows `world` via `condition_met`) — must happen
    // *before* any `world.insert` below; interleaving a read after an
    // insert here is a borrow-checker conflict, since `condition_met`
    // closes over `world: &World`.
    let active_pkg = byroredux_plugin::esm::records::active_package(
        npc.ai_packages
            .iter()
            .filter_map(|pk| index.packages.get(pk)),
        game_hour,
        condition_met,
    );

    if let Some(pk) = active_pkg {
        if pk.is_sandbox() {
            // PLDT's authored radius (when decoded and > 0) replaces
            // `sandbox_seat_system`'s blanket search-radius guess with the
            // per-package value the CK author actually set. `location` can
            // be `None` (no PLDT) or carry radius 0 (some vanilla packages,
            // e.g. FormID-anchored ones with no meaningful area) — both
            // fall back to the system's own default.
            //
            // Deliberately NOT resolving `PackLocationTarget::NearReference`
            // to a live entity's position for the search *center* —
            // investigated 2026-07-14 and found low-value. Empirically
            // (real FalloutNV.esm, 1822 NearReference-type Sandbox
            // packages): 62% target a FormID that isn't a placed ref
            // anywhere in the parsed cell data; of the rest, 69% resolve to
            // the hardcoded XMarker/XMarkerHeading base records (0x34 /
            // 0x3b), which `cell_loader` filters out and never spawns as
            // ECS entities regardless. That leaves ~12% of packages where
            // resolution could even succeed — before accounting for
            // same-cell scoping (a target in a different, unloaded cell
            // can never resolve) or spawn-ordering (references are placed
            // in one interleaved pass, not markers-then-actors, so a
            // same-cell target later in the REFR list isn't live yet). The
            // actor's own position remains the v0 center approximation.
            // M42.4's `travel_system` now uses
            // `resolve_entity_by_global_form_id` (`crates/scripting/src
            // /condition.rs`) from a later vantage point — its own first
            // tick, after the whole cell has finished loading — which
            // sidesteps the spawn-ordering half of this problem; Sandbox
            // could adopt the same approach here if a future session wants
            // the search *center* to do the same.
            let search_radius = pk
                .location
                .map(|loc| loc.radius as f32)
                .filter(|r| *r > 0.0);
            world.insert(
                placement_root,
                byroredux_core::ecs::components::SandboxBehavior { search_radius },
            );
        } else if pk.is_wander() {
            // M42.3: same PLDT-radius-or-default pattern as Sandbox above,
            // same v0 "actor's own position is the center approximation"
            // rationale.
            let wander_radius = pk
                .location
                .map(|loc| loc.radius as f32)
                .filter(|r| *r > 0.0);
            world.insert(
                placement_root,
                byroredux_core::ecs::components::WanderBehavior {
                    wander_radius,
                    form_id: npc.form_id,
                },
            );
        } else if pk.is_travel() {
            // M42.4: unlike Sandbox/Wander, Travel's *destination* matters
            // (it walks there once and stops, rather than just needing a
            // search center) — so it captures the PLDT target's raw
            // FormID here, and `travel_system` attempts to resolve it to a
            // live entity's position on its own first tick via
            // `resolve_entity_by_global_form_id` (see that fn's doc +
            // `travel.rs`'s module docs for why this is a strictly better
            // vantage point than a spawn-time attempt, and why it still
            // won't resolve most targets). Only `NearReference` carries a
            // FormID that fn can resolve; other location types leave
            // `travel_target_form_id` `None` and fall straight to
            // `travel_system`'s hash-picked fallback.
            let travel_radius = pk
                .location
                .map(|loc| loc.radius as f32)
                .filter(|r| *r > 0.0);
            let travel_target_form_id = pk.location.and_then(|loc| match loc.target {
                byroredux_plugin::esm::records::PackLocationTarget::NearReference(fid) => Some(fid),
                _ => None,
            });
            world.insert(
                placement_root,
                byroredux_core::ecs::components::TravelBehavior {
                    radius: travel_radius,
                    target_form_id: travel_target_form_id,
                    form_id: npc.form_id,
                },
            );
        } else if pk.is_follow() {
            // M42.5: Follow needs `PTDT` (target data) rather than `PLDT`
            // (location) — decoded for the first time in this codebase for
            // this procedure. Only `SpecificReference`/`ObjectId` PTDT
            // target types carry a FormID `resolve_entity_by_global_form_id`
            // can resolve; `Other` leaves `follow_target_form_id` `None`,
            // and (unlike Travel) there's no hash-picked fallback — a
            // Follow package with no resolvable target simply never moves
            // (see `follow.rs`'s module docs).
            let follow_distance = pk
                .target
                .map(|t| t.count_or_distance as f32)
                .filter(|d| *d > 0.0);
            let follow_target_form_id = pk.target.and_then(|t| match t.target {
                byroredux_plugin::esm::records::PackTargetKind::SpecificReference(fid)
                | byroredux_plugin::esm::records::PackTargetKind::ObjectId(fid) => Some(fid),
                byroredux_plugin::esm::records::PackTargetKind::Other(_) => None,
            });
            world.insert(
                placement_root,
                byroredux_core::ecs::components::FollowBehavior {
                    target_form_id: follow_target_form_id,
                    follow_distance,
                },
            );
        } else if pk.is_escort() {
            // M42.6: Escort needs both `PTDT` (who to collect, read the
            // same way Follow reads it) and `PLDT` (where to lead them,
            // read the same way Travel reads it) — no new sub-record
            // decode work, just the two existing reads combined onto one
            // component.
            let escort_target_form_id = pk.target.and_then(|t| match t.target {
                byroredux_plugin::esm::records::PackTargetKind::SpecificReference(fid)
                | byroredux_plugin::esm::records::PackTargetKind::ObjectId(fid) => Some(fid),
                byroredux_plugin::esm::records::PackTargetKind::Other(_) => None,
            });
            let escort_destination_radius = pk
                .location
                .map(|loc| loc.radius as f32)
                .filter(|r| *r > 0.0);
            let escort_destination_form_id = pk.location.and_then(|loc| match loc.target {
                byroredux_plugin::esm::records::PackLocationTarget::NearReference(fid) => Some(fid),
                _ => None,
            });
            world.insert(
                placement_root,
                byroredux_core::ecs::components::EscortBehavior {
                    target_form_id: escort_target_form_id,
                    destination_form_id: escort_destination_form_id,
                    destination_radius: escort_destination_radius,
                    form_id: npc.form_id,
                },
            );
        } else if pk.is_guard() {
            // M42.7: same PLDT-only reads as Travel (no PTDT — Guard has
            // nothing to collect, just a post to hold).
            let guard_radius = pk
                .location
                .map(|loc| loc.radius as f32)
                .filter(|r| *r > 0.0);
            let guard_anchor_form_id = pk.location.and_then(|loc| match loc.target {
                byroredux_plugin::esm::records::PackLocationTarget::NearReference(fid) => Some(fid),
                _ => None,
            });
            world.insert(
                placement_root,
                byroredux_core::ecs::components::GuardBehavior {
                    anchor_form_id: guard_anchor_form_id,
                    radius: guard_radius,
                    form_id: npc.form_id,
                },
            );
        } else if pk.is_patrol() {
            // M42.8: same PLDT-only reads as Wander (no target resolution
            // — v0 Patrol is Wander's algorithm under a different tag, see
            // `systems::patrol` module docs).
            let patrol_radius = pk
                .location
                .map(|loc| loc.radius as f32)
                .filter(|r| *r > 0.0);
            world.insert(
                placement_root,
                byroredux_core::ecs::components::PatrolBehavior {
                    patrol_radius,
                    form_id: npc.form_id,
                },
            );
        }
    }
}
