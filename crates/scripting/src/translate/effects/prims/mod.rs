//! The effect-primitive table (#5801, split from `effects.rs`).
//!
//! [`EFFECT_PRIMITIVES`] is the ordering contract: first match wins, so a
//! row's position is behavior and the table stays ONE list here. The
//! classifiers themselves live in one file per family, each named after its
//! `fragment/effects/<family>.rs` applier where one exists, so a native's
//! lowering and its application pair by name.

use super::*;

mod actor_state;
mod ai_combat;
mod inventory;
mod lock;
mod placement;
mod player_control;
mod quest_scoped;
mod scene;
mod vehicle_cinematic;
mod world;

use actor_state::*;
use ai_combat::*;
use inventory::*;
use lock::*;
use placement::*;
use player_control::*;
use quest_scoped::*;
use scene::*;
use vehicle_cinematic::*;
use world::*;

/// Classify a single effect statement against the primitive table.
pub(super) fn classify_effect(e: &Expr, scope: &Scope) -> Option<Effect> {
    EFFECT_PRIMITIVES.iter().find_map(|p| p(e, scope))
}

pub(super) fn classify_effect_with_providers(
    expression: &Expr,
    scope: &Scope,
    providers: Option<&PapyrusProviderCatalog>,
) -> Option<Effect> {
    // #3934 — the canonical effect table is consulted FIRST, and a manifest
    // provider alias only ever gets a statement no primitive claimed.
    //
    // The reverse order let one installed extension change how vanilla
    // content lowers, two ways. `PapyrusProviderCatalog::insert` records the
    // *provider identifier* of every strictly-inserted alias, and
    // `lower_provider_call` hard-errors (`UnknownFunction`) on a miss whose
    // identifier is any known provider — so a single alias published under
    // the reserved `Self` receiver (which the SDK prescribes for instance
    // methods), or under `Game`/`Utility`, made every `Self.SetStage(..)`,
    // `Game.*` or `Utility.Wait(..)` in the corpus decline its whole
    // fragment. And an alias spelled exactly like a modeled effect silently
    // *replaced* it with a deferred host barrier — for `Utility.Wait` that
    // also slipped past `has_latent`, which only matches `Effect::Wait`.
    // Primitives-first makes both impossible: canonical semantics win, and
    // an alias can only ever add a spelling the engine does not model.
    if let Some(effect) = classify_effect(expression, scope) {
        return Some(effect);
    }
    let providers = providers?;
    match lower_provider_call(expression, providers) {
        Ok(Some(call)) => Some(Effect::ProviderCall(FragmentProviderCall {
            route: call.route.qualified_name().to_owned(),
            arguments: call.arguments,
            principal: None,
        })),
        // Unclaimed by both tables, or a bound-but-unresolvable extender
        // call: decline the fragment, same as any unmodeled statement.
        Ok(None) | Err(_) => None,
    }
}

/// An effect primitive: matches one effect-call shape and binds its
/// holes (resolving the receiver to a [`QuestRef`] via `scope`), or
/// declines. Internal — the public surface is [`lower_fragment`].
type EffectPrimitive = fn(&Expr, &Scope) -> Option<Effect>;

/// The effect-primitive table. First match wins.
pub(super) const EFFECT_PRIMITIVES: &[EffectPrimitive] = &[
    prim_set_global_value,
    prim_set_stage,
    prim_start_quest,
    prim_stop_quest,
    prim_complete_quest,
    prim_reset_quest,
    prim_set_quest_active,
    prim_set_objective_displayed,
    prim_set_objective_completed,
    prim_set_objective_failed,
    prim_complete_all_objectives,
    prim_fail_all_objectives,
    prim_add_item,
    prim_remove_item,
    prim_change_faction,
    prim_set_actor_value,
    prim_change_perk,
    prim_force_alias,
    prim_block_activation,
    prim_send_story_event,
    prim_add_to_map,
    prim_set_location_cleared,
    prim_set_base_protection,
    prim_kill,
    prim_set_relationship_rank,
    prim_show_message,
    prim_notification,
    prim_show_barter_menu,
    prim_delete_when_able,
    prim_change_keyword,
    prim_reward_player_xp,
    prim_increment_stat,
    prim_add_achievement,
    prim_set_linked_ref,
    prim_set_loc_ref_type,
    prim_damage_actor_value,
    prim_advance_skill,
    prim_obtain_word,
    prim_set_player_teammate,
    prim_set_doing_favor,
    prim_player_caps,
    prim_set_objective_skipped,
    prim_equip_item,
    prim_move_to,
    prim_move_to_editor_location,
    prim_force_location_to,
    prim_set_location_keyword_data,
    prim_disable,
    prim_enable,
    prim_start_scene,
    prim_stop_scene,
    prim_activate,
    prim_set_open,
    prim_lock,
    prim_set_lock_level,
    prim_set_player_restrained,
    prim_disable_player_controls,
    prim_enable_player_controls,
    prim_set_player_ai_driven,
    prim_set_hud_cart_mode,
    prim_set_in_chargen,
    prim_show_race_menu,
    prim_request_save,
    prim_request_auto_save,
    prim_set_enemy,
    prim_set_ally,
    prim_start_combat,
    prim_stop_combat,
    prim_stop_combat_alarm,
    prim_set_crime_faction,
    prim_set_unconscious,
    prim_set_ghost,
    prim_add_spell,
    prim_remove_spell,
    prim_add_race_spells,
    prim_remove_race_spells,
    prim_play_idle,
    prim_set_vehicle,
    prim_tether_to_horse,
    prim_set_motion_type,
    prim_set_sitting_rotation,
    prim_exit_cart,
    prim_player_imod_animation,
    prim_player_furniture_animation,
    prim_evaluate_package,
    prim_wait,
];

pub(super) fn prim_wait(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = static_call(e, "Utility", "Wait")?;
    if args.len() != 1 {
        return None;
    }
    let seconds = as_num(&args[0].value.node)?;
    // #5681 — a wait the session can never satisfy (`3.0e38` is a finite
    // f32) would park an immortal entry in `FragmentExecutionQueue`, saved
    // with the rest of the world. Decline above the shared cap; the VM
    // fallback stops the same run at its own suspend site, so the wait
    // dies on both paths instead of moving between them.
    (seconds.is_finite() && (0.0..=MAX_PROVIDER_WAIT_SECONDS).contains(&seconds))
        .then_some(Effect::Wait { seconds })
}
