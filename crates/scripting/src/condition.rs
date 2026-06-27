//! M47.1 Phases 2 + 3 — `ConditionFunction` enum + OR-precedence
//! evaluator.
//!
//! Plugin-side ([`byroredux_plugin::esm::records::condition`]) parses
//! CTDA sub-records into [`Condition`] values; scripting-side (here)
//! interprets the function indices against ECS state and combines
//! the per-condition booleans with the OR-precedence rule.
//!
//! ## What "OR-precedence" means
//!
//! Default operator between conditions is AND. Setting `or_next` on
//! a condition combines it with the next via OR. **Consecutive ORs
//! form a block that binds tighter than the surrounding AND chain.**
//!
//! `A AND B OR C AND D` evaluates as `A AND (B OR C) AND D`, NOT
//! `(A AND B) OR (C AND D)` (the standard boolean reading).
//!
//! This is the opposite of standard boolean precedence. Bethesda
//! designers compose complex expressions by exploiting the distributive
//! law (`(A AND B) OR (C AND D) ⇔ (A OR C) AND (A OR D) AND (B OR C) AND (B OR D)`).
//!
//! ## Function catalog status
//!
//! Bethesda ships ~300 condition functions across the four-game
//! lineage. M47.1 Phase 2 ships **7 representative functions** with
//! their canonical FO3 / FNV / Skyrim indices:
//!
//! | Index | Function       | Reads                     |
//! |-------|----------------|---------------------------|
//! | 9     | GetActorValue  | `ActorStats[param_1.name]`|
//! | 36    | GetDistance    | `GlobalTransform`         |
//! | 58    | GetStage       | `QuestStageState[param_1]`|
//! | 59    | GetStageDone   | `QuestStageState[param_1]`|
//! | 60    | GetFactionRank | `FactionRanks`            |
//! | 71    | GetIsID        | `FormIdComponent`         |
//! | 99    | HasPerk        | `PerkList`                |
//!
//! Unknown function indices evaluate to `0.0` (the Bethesda "unknown
//! function → safe-default" contract) and are logged at debug for
//! future-catalog tracking.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::{Condition, ConditionList, ConditionValue, RunOn};

/// Identifier for a condition function. Wraps the raw u32 function
/// index from `Condition.function_index` with a typed constructor so
/// the dispatcher's `match` is exhaustive against the known catalog.
///
/// New functions land by adding a variant + a match arm in
/// [`evaluate_function`]. Unknown indices fall through to
/// [`Self::Unknown`] which evaluates to `0.0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionFunction {
    /// `GetActorValue(actor_value_name) → f32`. Intended to read the
    /// `param_1`-named stat from the Run-On's `ActorStats` component;
    /// currently returns 0.0 pending the AVIF-FormID→name resolver — the
    /// one genuine stub left in this catalog (#1663).
    /// FO3 / FNV / Skyrim function index 9.
    GetActorValue,
    /// `GetDistance(target_form_id) → f32`. Squared-distance reduces
    /// to a single sqrt at evaluation time. FO3 / FNV / Skyrim index 36.
    GetDistance,
    /// `GetStage(quest_form_id) → f32`. Looks up the current stage
    /// for `param_1` quest in the `QuestStageState` resource.
    /// FO3 / FNV / Skyrim index 58.
    GetStage,
    /// `GetStageDone(quest_form_id, stage) → f32`. Returns 1.0 if
    /// `stage` (param_2) for `quest_form_id` (param_1) has been
    /// reached at any point, 0.0 otherwise. Bethesda's standard
    /// idempotency primitive for "this quest milestone has fired
    /// before." FO3 / FNV / Skyrim index 59.
    GetStageDone,
    /// `GetFactionRank(faction_form_id) → f32`. Reads the Run-On actor's
    /// `FactionRanks` component: the integer rank when the actor is in
    /// the faction, else -1 (Bethesda's "not in faction" sentinel — also
    /// returned when the actor carries no `FactionRanks`).
    /// FO3 / FNV / Skyrim index 60.
    GetFactionRank,
    /// `GetIsID(base_form_id) → f32`. Returns 1.0 when the Run-On's
    /// `FormIdComponent` matches `param_1`, 0.0 otherwise. Common
    /// gate for "is this specific REFR?" checks. FO3 / FNV / Skyrim
    /// index 71.
    GetIsID,
    /// `HasPerk(perk_form_id) → f32`. Reads the Run-On actor's `PerkList`:
    /// 1.0 when it contains `param_1`, 0.0 otherwise (also 0.0 when the
    /// actor carries no `PerkList`). Skyrim+ index 99.
    HasPerk,
    /// Function index outside the M47.1 catalog. Evaluates to 0.0
    /// (the Bethesda "unknown function safe-default" — see file-
    /// header doc-comment).
    Unknown(u32),
}

impl ConditionFunction {
    /// Map a raw u32 function index to a typed variant. Indices
    /// outside the catalog fall through to [`Self::Unknown`].
    pub fn from_index(index: u32) -> Self {
        match index {
            9 => Self::GetActorValue,
            36 => Self::GetDistance,
            58 => Self::GetStage,
            59 => Self::GetStageDone,
            60 => Self::GetFactionRank,
            71 => Self::GetIsID,
            99 => Self::HasPerk,
            other => Self::Unknown(other),
        }
    }
}

/// Per-evaluation context — the abstract Run-On targets a CTDA may
/// name (`Subject` / `Target` / `CombatTarget` / …) resolved to
/// concrete entity ids by the caller. Each consumer
/// (perk dispatcher, AI package head, quest_advance system) fills
/// the slots it knows about.
///
/// `None` slots cause the evaluator to short-circuit `false` when a
/// condition's Run-On references that slot — matches Bethesda's
/// "missing reference → condition fails" contract. The exception is
/// [`RunOn::Subject`], which defaults to `subject` (always Some, since
/// every condition list runs in the context of a subject).
#[derive(Debug, Clone, Copy)]
pub struct ConditionContext {
    /// The "self" entity — quest target, dialogue speaker, magic
    /// effect caster. Always populated.
    pub subject: EntityId,
    /// The "other" entity — dialogue listener, package target, effect
    /// target. `None` when the context doesn't carry one (e.g., a
    /// quest stage condition with no specific target).
    pub target: Option<EntityId>,
    /// Subject's current combat target. `None` when not in combat or
    /// out of M47.1's scope.
    pub combat_target: Option<EntityId>,
    /// Subject's linked-reference chain head. `None` until M47.0.x
    /// linked-ref wiring lands.
    pub linked_reference: Option<EntityId>,
}

impl ConditionContext {
    /// Build a minimal context with only the subject populated. Most
    /// consumers (quest stage gates, dialogue branches) start here
    /// and add `target` per call.
    pub fn for_subject(subject: EntityId) -> Self {
        Self {
            subject,
            target: None,
            combat_target: None,
            linked_reference: None,
        }
    }

    /// Resolve a [`RunOn`] choice to a concrete EntityId. Returns
    /// `None` when the slot isn't populated or the choice references
    /// data M47.1 doesn't yet plumb (alias / package / event data).
    fn resolve(&self, run_on: RunOn, condition: &Condition, _world: &World) -> Option<EntityId> {
        match run_on {
            RunOn::Subject => Some(self.subject),
            RunOn::Target => self.target,
            RunOn::CombatTarget => self.combat_target,
            RunOn::LinkedReference => self.linked_reference,
            RunOn::Reference => {
                // FormID→EntityId resolver not yet wired: condition.reference_form_id
                // is a raw u32 ESM form ID; find_by_form_id requires an interned FormId.
                // Returns None until a u32→FormId pool lookup is plumbed here.
                log::trace!(
                    "M47.1: RunOn::Reference for form_id {:08X} — \
                     FormID→EntityId resolver not yet wired",
                    condition.reference_form_id,
                );
                None
            }
            RunOn::QuestAlias | RunOn::PackageData | RunOn::EventData => {
                log::trace!(
                    "M47.1: RunOn::{:?} (extra_data_id={:08X}) — \
                     alias / package / event resolvers deferred",
                    run_on,
                    condition.extra_data_id,
                );
                None
            }
        }
    }
}

/// Evaluate a single [`Condition`] against world state + the
/// resolution context.
///
/// Returns the boolean result. Used internally by [`evaluate`]; exposed
/// for tests + diagnostic dumps.
pub fn evaluate_condition(condition: &Condition, world: &World, ctx: &ConditionContext) -> bool {
    // Resolve Run-On to a concrete entity. `None` → condition fails
    // (Bethesda: "missing reference fails the predicate").
    let Some(entity) = ctx.resolve(condition.run_on, condition, world) else {
        return false;
    };

    // Run the function — returns the f32 the comparator works against.
    let function = ConditionFunction::from_index(condition.function_index);
    let function_result = evaluate_function(function, condition, entity, world);

    // Resolve comparand — Globals route through the `Globals` resource
    // (mirrored from `EsmIndex.globals`, #1668). `form_id` is already in
    // global load-order space (remapped at CTDA parse time), matching the
    // resource's key space. A missing resource or unknown GLOB resolves to
    // 0.0 — Bethesda's "missing GLOB defaults to 0".
    let comparand = match condition.comparand {
        ConditionValue::Literal(v) => v,
        ConditionValue::Global(form_id) => world
            .try_resource::<crate::globals::Globals>()
            .and_then(|g| g.get(form_id))
            .unwrap_or(0.0),
    };

    condition.comparator.apply(function_result, comparand)
}

/// Resolve a global-load-order FormID to the entity that carries it.
///
/// `World::find_by_form_id` keys by an interned [`FormId`] handle; a remapped
/// CTDA `param_1` is a raw global `u32`, so this resolves through the
/// [`FormIdPool`] instead — an entity matches when its `FormIdComponent`
/// resolves to a pair whose `local` equals `form_id` (the cell loader stores
/// the full global FormID as the `LocalFormId`). O(n) over entities carrying
/// a `FormIdComponent`, which is fine for the rare condition-eval path.
fn resolve_entity_by_global_form_id(world: &World, form_id: u32) -> Option<EntityId> {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::FormIdPool;
    let pool = world.try_resource::<FormIdPool>()?;
    let q = world.query::<FormIdComponent>()?;
    let found = q
        .iter()
        .find(|(_, fid)| pool.resolve(fid.0).is_some_and(|p| p.local.0 == form_id))
        .map(|(eid, _)| eid);
    found
}

/// Dispatch one of the M47.1-catalog functions against the supplied
/// entity. Returns `0.0` for unknown indices (Bethesda safe-default)
/// or when the function's backing ECS storage isn't registered.
pub fn evaluate_function(
    function: ConditionFunction,
    condition: &Condition,
    entity: EntityId,
    world: &World,
) -> f32 {
    match function {
        ConditionFunction::GetActorValue => {
            // param_1 is an AVIF FormID; ActorStats is keyed by string name.
            // AVIF→name resolver deferred to M47.1 follow-up.
            log::trace!(
                "M47.1: GetActorValue(param_1={:08X}) — \
                 AVIF→ActorStats key resolver deferred",
                condition.param_1,
            );
            let _ = entity;
            0.0
        }
        ConditionFunction::GetDistance => {
            // GetDistance(target_form_id) → ‖subject − target‖ in world units.
            // `param_1` is the target FormID, already remapped to global
            // load-order space at parse time (#1666) — the same space entity
            // `FormIdComponent`s resolve to — so the resolve is exact.
            use byroredux_core::ecs::components::GlobalTransform;
            let Some(target) = resolve_entity_by_global_form_id(world, condition.param_1) else {
                // Target not currently spawned (e.g. in an unloaded cell).
                // Model an absent target as infinitely far so a proximity
                // gate (`GetDistance < N`) correctly fails rather than
                // reading a missing ref as "right here" (a 0.0 would).
                return f32::MAX;
            };
            let Some(subject_pos) = world.get::<GlobalTransform>(entity).map(|t| t.translation)
            else {
                return f32::MAX;
            };
            // First `get` borrow has been dropped (translation is Copy) before
            // the second — avoids holding two read guards on one storage lock.
            let Some(target_pos) = world.get::<GlobalTransform>(target).map(|t| t.translation)
            else {
                return f32::MAX;
            };
            (subject_pos - target_pos).length()
        }
        ConditionFunction::GetStage => {
            // GetStage(quest_form_id). The current quest stage lives
            // in the [`crate::quest_stages::QuestStageState`] resource.
            // `param_1` is the quest FormID.
            use crate::quest_stages::{QuestFormId, QuestStageState};
            let Some(state) = world.try_resource::<QuestStageState>() else {
                return 0.0;
            };
            state.get_stage(QuestFormId(condition.param_1)) as f32
        }
        ConditionFunction::GetStageDone => {
            // GetStageDone(quest_form_id, stage). param_1 = quest
            // FormID, param_2 = stage. Returns 1.0 when the stage
            // has been reached, 0.0 otherwise. The Bethesda
            // idempotency primitive — "this milestone fired before."
            use crate::quest_stages::{QuestFormId, QuestStageState};
            let Some(state) = world.try_resource::<QuestStageState>() else {
                return 0.0;
            };
            let quest = QuestFormId(condition.param_1);
            let stage = condition.param_2 as u16;
            if state.get_stage_done(quest, stage) {
                1.0
            } else {
                0.0
            }
        }
        ConditionFunction::GetFactionRank => {
            // GetFactionRank(faction_form_id) → the Run-On actor's rank in
            // `param_1`'s faction, or -1.0 (Bethesda's "not in faction"
            // sentinel) when the actor has no `FactionRanks` or isn't a
            // member. Faction ids are compared in the NPC's source space —
            // identity-equal to a remapped `param_1` in single-plugin loads
            // (see `FactionRanks` docs).
            use byroredux_core::ecs::components::FactionRanks;
            world
                .get::<FactionRanks>(entity)
                .and_then(|f| f.rank(condition.param_1))
                .map_or(-1.0, |rank| rank as f32)
        }
        ConditionFunction::GetIsID => {
            // Test the Run-On entity's identity against `param_1`. The CTDA
            // form-id remap (#1666, applied at parse time in the plugin crate)
            // has already promoted `param_1` into global load-order space — the
            // same space the entity's `FormIdComponent` resolves to via
            // `FormIdPool` — so this is a direct, false-positive-free compare
            // across multi-plugin loads (no lower-24-bits shortcut).
            use byroredux_core::ecs::components::FormIdComponent;
            use byroredux_core::form_id::FormIdPool;
            let Some(fid_comp) = world.get::<FormIdComponent>(entity) else {
                return 0.0;
            };
            let Some(pool) = world.try_resource::<FormIdPool>() else {
                return 0.0;
            };
            // `local` carries the full global FormID — the cell loader stores
            // the remapped placement/base id as the LocalFormId
            // (references.rs), so `pair.local.0` is directly comparable.
            match pool.resolve(fid_comp.0) {
                Some(pair) if pair.local.0 == condition.param_1 => 1.0,
                _ => 0.0,
            }
        }
        ConditionFunction::HasPerk => {
            // 1.0 iff the Run-On actor's `PerkList` holds `param_1`. Same
            // remap contract as GetIsID: `param_1` is global, and each perk
            // FormId resolves through `FormIdPool` to its global FormIdPair.
            use byroredux_core::ecs::components::PerkList;
            use byroredux_core::form_id::FormIdPool;
            let Some(perks) = world.get::<PerkList>(entity) else {
                return 0.0;
            };
            let Some(pool) = world.try_resource::<FormIdPool>() else {
                return 0.0;
            };
            let held = perks.0.iter().any(|&perk| {
                pool.resolve(perk)
                    .is_some_and(|pair| pair.local.0 == condition.param_1)
            });
            if held {
                1.0
            } else {
                0.0
            }
        }
        ConditionFunction::Unknown(index) => {
            log::trace!(
                "M47.1: condition function index {index} not in M47.1 catalog — \
                 evaluates to 0.0 (Bethesda safe-default)",
            );
            0.0
        }
    }
}

/// Evaluate a full [`ConditionList`] with OR-precedence semantics.
///
/// Empty lists return `true` (Bethesda contract: "no conditions =
/// always fires"). Otherwise, walks the list grouping consecutive
/// ORs into a single "OR block" — the block evaluates `true` if ANY
/// of its members evaluate `true`. The list then AND-combines the
/// per-block results.
///
/// Walked formally: `[c0, c1, c2, c3, …]` with `or_next` flags
/// `[true, true, false, …]` groups as `[c0 OR c1 OR c2] AND [c3 OR …]`.
/// A block continues as long as the PREVIOUS condition's `or_next`
/// is set; the LAST condition's `or_next` is meaningless (no `next`).
///
/// Short-circuits: any AND-block returning `false` terminates the
/// scan early. Any OR-block returning `true` finishes the block and
/// moves on without evaluating the remaining OR members.
pub fn evaluate(conditions: &ConditionList, world: &World, ctx: &ConditionContext) -> bool {
    if conditions.is_empty() {
        return true; // "no conditions = always fires" contract
    }

    let mut i = 0usize;
    while i < conditions.len() {
        // Discover the end of the current OR block. A block extends
        // while the CURRENT condition's `or_next` flag is set.
        let block_start = i;
        while i < conditions.len() && conditions[i].or_next {
            i += 1;
        }
        // `i` now points at the LAST condition of the block (its
        // `or_next` is false). Block = [block_start ..= i].
        let block_end_inclusive = i;
        i += 1; // step past the block for next iteration

        // Evaluate the block. Single-condition blocks (no preceding
        // OR flag) reduce to one evaluation; multi-condition blocks
        // are OR-combined with short-circuit.
        let block_result = (block_start..=block_end_inclusive)
            .any(|j| evaluate_condition(&conditions[j], world, ctx));
        if !block_result {
            // AND-combine with the surrounding chain — false block
            // fails the whole list.
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::world::World;
    use byroredux_plugin::esm::records::condition::{ComparisonOp, RunOn};

    fn ctx(subject: EntityId) -> ConditionContext {
        ConditionContext::for_subject(subject)
    }

    fn cond(
        function_index: u32,
        comparator: ComparisonOp,
        comparand: f32,
        or_next: bool,
    ) -> Condition {
        Condition {
            function_index,
            comparator,
            comparand: ConditionValue::Literal(comparand),
            run_on: RunOn::Subject,
            or_next,
            ..Default::default()
        }
    }

    #[test]
    fn empty_condition_list_returns_true() {
        let world = World::new();
        let result = evaluate(&Vec::new(), &world, &ctx(0));
        assert!(result, "no conditions = always fires");
    }

    #[test]
    fn single_true_condition_returns_true() {
        use crate::quest_stages::{QuestFormId, QuestStageState};
        let mut world = World::new();
        crate::register(&mut world);
        let mut state = QuestStageState::default();
        state.set_stage(QuestFormId(0xAA), 42);
        world.insert_resource(state);

        // GetStage(0xAA) == 42
        let list = vec![cond(58, ComparisonOp::Eq, 42.0, false).with_param_1(0xAA)];
        assert!(evaluate(&list, &world, &ctx(0)));
    }

    #[test]
    fn single_false_condition_returns_false() {
        use crate::quest_stages::{QuestFormId, QuestStageState};
        let mut world = World::new();
        crate::register(&mut world);
        let mut state = QuestStageState::default();
        state.set_stage(QuestFormId(0xAA), 42);
        world.insert_resource(state);

        // GetStage(0xAA) == 99 (actual is 42)
        let list = vec![cond(58, ComparisonOp::Eq, 99.0, false).with_param_1(0xAA)];
        assert!(!evaluate(&list, &world, &ctx(0)));
    }

    #[test]
    fn unknown_function_index_returns_zero_and_satisfies_eq_zero() {
        // Bethesda's "unknown function safe-default" — function returns
        // 0.0, so a `== 0` comparator matches.
        let world = World::new();
        let list = vec![cond(99999, ComparisonOp::Eq, 0.0, false)];
        assert!(evaluate(&list, &world, &ctx(0)));
    }

    #[test]
    fn and_chain_short_circuits_on_first_false() {
        // [false, true, true] = AND chain — first failure ends scan.
        let world = World::new();
        let list = vec![
            cond(99999, ComparisonOp::Ne, 0.0, false), // false (Unknown→0, !=0 fails)
            cond(99999, ComparisonOp::Eq, 0.0, false), // would be true
        ];
        assert!(!evaluate(&list, &world, &ctx(0)));
    }

    #[test]
    fn or_block_returns_true_when_any_member_true() {
        // [c0 OR c1 OR c2] — c0 false, c1 true short-circuits the block.
        let world = World::new();
        let list = vec![
            cond(99999, ComparisonOp::Ne, 0.0, true),  // false, or_next
            cond(99999, ComparisonOp::Eq, 0.0, true),  // true (block hit)
            cond(99999, ComparisonOp::Ne, 0.0, false), // would be false; block already true
        ];
        assert!(evaluate(&list, &world, &ctx(0)));
    }

    #[test]
    fn or_precedence_quirk_a_and_b_or_c_and_d_groups_b_or_c() {
        // The load-bearing test: `A AND B OR C AND D` evaluates as
        // `A AND (B OR C) AND D`, NOT `(A AND B) OR (C AND D)`.
        //
        // We construct:
        //   A: false (failing condition)
        //   B: true, or_next=true   ┐
        //   C: true, or_next=false  ┘  → (B OR C) = true
        //   D: true
        //
        // Standard interpretation `(A AND B) OR (C AND D)` would be
        // `(false AND true) OR (true AND true)` = false OR true = true.
        // OR-precedence interpretation `A AND (B OR C) AND D` is
        // `false AND true AND true` = false.
        //
        // Expecting the OR-precedence interpretation → false.
        let world = World::new();
        let list = vec![
            cond(99999, ComparisonOp::Ne, 0.0, false), // A: false
            cond(99999, ComparisonOp::Eq, 0.0, true),  // B: true, or_next
            cond(99999, ComparisonOp::Eq, 0.0, false), // C: true (no or_next: end of OR block)
            cond(99999, ComparisonOp::Eq, 0.0, false), // D: true
        ];
        assert!(
            !evaluate(&list, &world, &ctx(0)),
            "OR-precedence: A AND (B OR C) AND D = false AND (true OR true) AND true = false"
        );
    }

    #[test]
    fn or_precedence_quirk_swap_test_a_true() {
        // Flip A to true so the AND chain succeeds — proves the
        // OR-precedence grouping isn't just always-false.
        //
        //   A: true
        //   B: false, or_next=true   ┐
        //   C: true,  or_next=false  ┘  → (B OR C) = true
        //   D: true
        //
        // → true AND (false OR true) AND true = true
        let world = World::new();
        let list = vec![
            cond(99999, ComparisonOp::Eq, 0.0, false), // A: true (Unknown=0, ==0)
            cond(99999, ComparisonOp::Ne, 0.0, true),  // B: false, or_next
            cond(99999, ComparisonOp::Eq, 0.0, false), // C: true
            cond(99999, ComparisonOp::Eq, 0.0, false), // D: true
        ];
        assert!(evaluate(&list, &world, &ctx(0)));
    }

    #[test]
    fn get_stage_returns_quest_state_stage() {
        use crate::quest_stages::{QuestFormId, QuestStageState};
        let mut world = World::new();
        crate::register(&mut world);
        let mut state = QuestStageState::default();
        state.set_stage(QuestFormId(0xAA), 42);
        world.insert_resource(state);

        // GetStage(0xAA) == 42
        let list = vec![cond(58, ComparisonOp::Eq, 42.0, false).with_param_1(0xAA)];
        assert!(evaluate(&list, &world, &ctx(0)));
        // GetStage(0xAA) < 100
        let list = vec![cond(58, ComparisonOp::Lt, 100.0, false).with_param_1(0xAA)];
        assert!(evaluate(&list, &world, &ctx(0)));
        // GetStage(0xAA) > 50 (false — actually 42)
        let list = vec![cond(58, ComparisonOp::Gt, 50.0, false).with_param_1(0xAA)];
        assert!(!evaluate(&list, &world, &ctx(0)));
    }

    // ── GetIsID (#1666) ─────────────────────────────────────────────────

    #[test]
    fn get_is_id_matches_entity_global_form_id() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

        let mut world = World::new();
        let mut pool = FormIdPool::new();
        // The cell loader stores the full global FormID as the LocalFormId,
        // so `param_1` (also global, post-remap) compares directly.
        let pair = FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0x0001_4D8A),
        };
        let fid = pool.intern(pair);
        world.insert_resource(pool);
        let actor = world.spawn();
        world.insert(actor, FormIdComponent(fid));

        // GetIsID(0x00014D8A) == 1 — matches the entity's id.
        let list = vec![cond(71, ComparisonOp::Eq, 1.0, false).with_param_1(0x0001_4D8A)];
        assert!(evaluate(&list, &world, &ctx(actor)));

        // A different id → 0.
        let list = vec![cond(71, ComparisonOp::Eq, 0.0, false).with_param_1(0x0001_9999)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    #[test]
    fn get_is_id_zero_without_form_id_component() {
        // No FormIdComponent (and no FormIdPool) → GetIsID returns 0.0.
        let world = World::new();
        let actor: EntityId = 7;
        let list = vec![cond(71, ComparisonOp::Eq, 0.0, false).with_param_1(0x0001_4D8A)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    // ── HasPerk (#1667) ─────────────────────────────────────────────────

    #[test]
    fn has_perk_checks_actor_perk_list() {
        use byroredux_core::ecs::components::PerkList;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

        let mut world = World::new();
        let mut pool = FormIdPool::new();
        let held = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(0x0005_8F80),
        });
        world.insert_resource(pool);
        let actor = world.spawn();
        world.insert(actor, PerkList::from_perks([held]));

        // HasPerk(0x00058F80) == 1 — the actor holds it.
        let list = vec![cond(99, ComparisonOp::Eq, 1.0, false).with_param_1(0x0005_8F80)];
        assert!(evaluate(&list, &world, &ctx(actor)));

        // HasPerk(0x00058F81) == 0 — not held.
        let list = vec![cond(99, ComparisonOp::Eq, 0.0, false).with_param_1(0x0005_8F81)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    #[test]
    fn has_perk_zero_without_perk_list() {
        // No PerkList component → HasPerk returns 0.0.
        let world = World::new();
        let actor: EntityId = 3;
        let list = vec![cond(99, ComparisonOp::Eq, 0.0, false).with_param_1(0x0005_8F80)];
        assert!(evaluate(&list, &world, &ctx(actor)));
    }

    // ── GetDistance (#1664) ─────────────────────────────────────────────

    #[test]
    fn get_distance_measures_subject_to_target() {
        use byroredux_core::ecs::components::{FormIdComponent, GlobalTransform};
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
        use byroredux_core::math::{Quat, Vec3};

        let mut world = World::new();
        let mut pool = FormIdPool::new();
        // Target carries global FormID 0x000159E2 (cell loader stores the
        // full global id as the LocalFormId).
        let target_fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0x0001_59E2),
        });
        world.insert_resource(pool);

        let subject = world.spawn();
        world.insert(
            subject,
            GlobalTransform::new(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY, 1.0),
        );
        let target = world.spawn();
        world.insert(target, FormIdComponent(target_fid));
        world.insert(
            target,
            GlobalTransform::new(Vec3::new(3.0, 4.0, 0.0), Quat::IDENTITY, 1.0),
        );

        // GetDistance(0x000159E2) < 10 → 5.0 < 10 → true.
        let mut lt = cond(36, ComparisonOp::Lt, 10.0, false).with_param_1(0x0001_59E2);
        lt.comparand = ConditionValue::Literal(10.0);
        assert!(evaluate(&vec![lt], &world, &ctx(subject)));

        // GetDistance(0x000159E2) < 4 → 5.0 < 4 → false.
        let lt = cond(36, ComparisonOp::Lt, 4.0, false).with_param_1(0x0001_59E2);
        assert!(!evaluate(&vec![lt], &world, &ctx(subject)));
    }

    #[test]
    fn get_distance_unresolved_target_is_far() {
        use byroredux_core::ecs::components::GlobalTransform;
        use byroredux_core::form_id::FormIdPool;
        use byroredux_core::math::{Quat, Vec3};

        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let subject = world.spawn();
        world.insert(
            subject,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );

        // No entity carries 0x000159E2 → distance is f32::MAX, so a
        // proximity gate `GetDistance < 100` fails.
        let lt = cond(36, ComparisonOp::Lt, 100.0, false).with_param_1(0x0001_59E2);
        assert!(!evaluate(&vec![lt], &world, &ctx(subject)));
    }

    // ── GetFactionRank (#1665) ──────────────────────────────────────────

    #[test]
    fn get_faction_rank_reads_membership() {
        use byroredux_core::ecs::components::FactionRanks;

        let mut world = World::new();
        let actor = world.spawn();
        world.insert(
            actor,
            FactionRanks::from_pairs([(0x0001_38B8, 2), (0x0001_5FFB, 0)]),
        );

        // GetFactionRank(0x000138B8) == 2.
        let eq = cond(60, ComparisonOp::Eq, 2.0, false).with_param_1(0x0001_38B8);
        assert!(evaluate(&vec![eq], &world, &ctx(actor)));

        // GetFactionRank(non-member) == -1.
        let eq = cond(60, ComparisonOp::Eq, -1.0, false).with_param_1(0x0009_9999);
        assert!(evaluate(&vec![eq], &world, &ctx(actor)));
    }

    #[test]
    fn get_faction_rank_minus_one_without_component() {
        // No FactionRanks component → -1.0 (not-in-faction sentinel).
        let world = World::new();
        let actor: EntityId = 4;
        let eq = cond(60, ComparisonOp::Eq, -1.0, false).with_param_1(0x0001_38B8);
        assert!(evaluate(&vec![eq], &world, &ctx(actor)));
    }

    // ── Global comparand (#1668) ────────────────────────────────────────

    #[test]
    fn global_comparand_resolves_from_globals_resource() {
        use crate::globals::Globals;
        use crate::quest_stages::{QuestFormId, QuestStageState};

        let mut world = World::new();
        crate::register(&mut world);
        let mut state = QuestStageState::default();
        state.set_stage(QuestFormId(0xAA), 5);
        world.insert_resource(state);

        let mut globals = Globals::new();
        globals.set(0x0100_0042, 5.0); // a GLOB whose value is 5
        world.insert_resource(globals);

        // GetStage(0xAA) == Global(0x01000042) → 5 == 5 → true.
        let mut eq = cond(58, ComparisonOp::Eq, 0.0, false).with_param_1(0xAA);
        eq.comparand = ConditionValue::Global(0x0100_0042);
        assert!(evaluate(&vec![eq], &world, &ctx(0)));

        // GetStage(0xAA) > Global(0x01000042) → 5 > 5 → false.
        let mut gt = cond(58, ComparisonOp::Gt, 0.0, false).with_param_1(0xAA);
        gt.comparand = ConditionValue::Global(0x0100_0042);
        assert!(!evaluate(&vec![gt], &world, &ctx(0)));
    }

    #[test]
    fn global_comparand_defaults_to_zero_when_unresolved() {
        use crate::quest_stages::{QuestFormId, QuestStageState};

        let mut world = World::new();
        crate::register(&mut world);
        let mut state = QuestStageState::default();
        state.set_stage(QuestFormId(0xAA), 0);
        world.insert_resource(state);

        // No Globals resource at all → comparand defaults to 0.0.
        // GetStage(0xAA) == Global(missing) → 0 == 0 → true.
        let mut eq = cond(58, ComparisonOp::Eq, 0.0, false).with_param_1(0xAA);
        eq.comparand = ConditionValue::Global(0x0100_0099);
        assert!(evaluate(&vec![eq], &world, &ctx(0)));
    }

    // ── Helper: chainable param_1 setter for compact test construction ──

    trait CondBuilder {
        fn with_param_1(self, p: u32) -> Self;
    }
    impl CondBuilder for Condition {
        fn with_param_1(mut self, p: u32) -> Self {
            self.param_1 = p;
            self
        }
    }
}
