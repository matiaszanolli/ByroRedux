//! Reference / alias scripts, quest fragment dispatch, the Papyrus update clock and the effects scripts request (traps, hits, magic, knockdown, flee, requests).
//!
//! A contiguous run of `Stage::Update` registrations (#6359): registration order is the
//! contract, so [`super::register_update_systems`] calls the segments in file order.

use super::*;

/// Reference / alias scripts, quest fragment dispatch, the Papyrus update clock and the effects scripts request (traps, hits, magic, knockdown, flee, requests).
pub(super) fn register_script_runtime(scheduler: &mut Scheduler) {
    // SCR-D6-NEW-02 (#1768) — the runtime scripting systems that were
    // registered (component/resource) but never scheduled. Both ride
    // the same exclusive-in-Update lane as the demo dispatchers above:
    // exclusive registration holds the world serially, so the fragment
    // system's resource-lock sequence never composes with a parallel
    // neighbour. `quest_fragment_dispatch_system` is `&World`-only, so
    // it needs the dt-dropping wrapper; `recurring_update_tick_system`
    // already has the `(&World, f32)` shape and is added directly.
    fn quest_fragment_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::quest_fragment_dispatch_system(world)
    }
    // M47.2 — Papyrus update registrations count the host's game clock.
    fn script_update_dispatch(world: &World, dt: f32) {
        let game_hours = world
            .try_resource::<crate::components::GameTimeRes>()
            .map(|time| time.absolute_hours());
        // M47.3 — the `GameHour` global is the game clock (CS *GetCurrentTime*).
        if let Some((day, hour)) = world.try_resource::<crate::components::GameTimeRes>().map(|time| (time.day, time.hour)) {
            byroredux_scripting::globals::sync_game_clock_globals(world, day, hour);
        }
        byroredux_scripting::script_update_system(world, dt, game_hours)
    }
    fn check_alias_scripts_dispatch(world: &World, _dt: f32) {
        crate::asset_provider::check_running_quest_alias_scripts(world)
    }
    // M47.2 — reference scripts the Papyrus VM runs receive this frame's
    // cell-load, activation and trigger events (all produced above). Ahead
    // of quest fragment dispatch, so a stage they set runs its fragment
    // this frame.
    //
    // M47.2 — the alias scripts of quests that run go first: checked when a
    // quest first runs, started (OnInit / OnAliasInit) once its aliases are
    // filled, shut down when it stops. The events of the references the
    // aliases are filled with are delivered by the system after them, with the
    // reference scripts'.
    scheduler.add_exclusive(Stage::Update, check_alias_scripts_dispatch);
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::alias_script_event_system);
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::reference_script_event_system,
    );
    // M47.2 — activate parents: this frame's activations passed on to the children.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::activate_parent_relay_system);
    // M47.2 — the object graphs' raised events, delivered as `OnAnimationEvent`
    // to the scripts that registered for them.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::animation_event_system);
    // M47.2 — `Sound.Play` requests the scripts above recorded.
    scheduler.add_exclusive(Stage::Update, crate::systems::scripted_sound::scripted_sound_system);
    // Dispatch quest fragments right after the advance that emits the
    // `QuestStageAdvanced` markers, before end-of-frame cleanup drains
    // them (populated live from parsed QUST VMAD fragments, #1739 / `8a70b81a`).
    scheduler.add_exclusive(Stage::Update, quest_fragment_dispatch);
    // QSDT Complete/Fail Quest flags and NAM0 successor quests apply after
    // the stage's own fragment has observed the transition.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::quest_terminal_stage_system,
    );
    // Latent Utility.Wait tails resume after their authored delay. Running
    // immediately after fresh fragment dispatch lets one timing path serve
    // both newly-suspended and already-pending continuations.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::fragment_continuation_system,
    );
    // OnUpdate / OnUpdateGameTime for the scripts registered for them,
    // beside the other latent-script clock.
    scheduler.add_exclusive(Stage::Update, script_update_dispatch);
    // M47.2 — the distance events whose pair is now close (or far).
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::distance_event_system);
    // Translate the resulting Skyrim PlayIdle FormID requests into decoded
    // HKX AnimationPlayers. The parallel animation batch observes a request
    // no later than the following frame, preserving deterministic restarts.
    scheduler.add_exclusive(Stage::Update, crate::systems::idle_clip_playback_system);
    // A one-shot `PlayIdle` that has run its length gives the actor its earlier animation back.
    scheduler.add_exclusive(Stage::Update, crate::systems::idle_resume_system);
    // `PickIdle`: the actor starts its generic idle again.
    scheduler.add_exclusive(Stage::Update, crate::systems::pick_idle_system);
    // Apply Papyrus SetMotionType requests after both immediate and resumed
    // fragment effects, before the Physics stage consumes body state.
    scheduler.add_exclusive(Stage::Update, crate::systems::scripted_motion_type_system);
    // ApplyHavokImpulse requests, after the motion-type change that made the body dynamic.
    scheduler.add_exclusive(Stage::Update, crate::systems::scripted_havok_impulse_system);
    // Fallout 4 actors stand in the idle their animation archetype names; a script's
    // `ChangeAnimArchetype` (run above) swaps it.
    scheduler.add_exclusive(Stage::Update, crate::systems::archetype_idle::archetype_idle_system);
    // M47.2 — `Weapon.Fire` projectiles: start the requested ones, then fly them
    // (a hit on a live actor queues the `HitEvent` the damage system consumes).
    scheduler.add_exclusive(Stage::Update, crate::systems::trap_hits::trap_contact_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::projectiles::projectile_launch_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::projectiles::projectile_flight_system);
    // `HitEvent` is a one-frame marker that `event_cleanup_system` drains in Late, so its
    // consumers run after EVERY producer: the melee / NPC strikes above, the VM effect flush
    // (`ProcessTrapHit`) and the projectile flight just before. (A consumer ahead of a
    // producer never sees its hits: the dart trap's darts dealt no damage that way.)
    // #5872 — `ProcessTrapHit` requests (queued by every script flush, including the Late
    // dialogue one, whose same-frame `HitEvent` would be drained unread) land as `HitEvent`s
    // here: after the script systems and projectile flight, before every consumer.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::pending_trap_hit_flush_system);
    // #5991 — the reference / alias scripts' `OnHit` (and its remote relays) read the same
    // markers, so they are delivered here, after the projectile and trap hits have landed;
    // `reference_script_event_system` above delivers every other marker.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::reference_script_hit_system);
    // M47.2 — the hits scripts registered for (Fallout 4 / Starfield
    // `RegisterForHitEvent`), delivered once each while the `HitEvent` markers
    // stand.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::hit_event_system);
    // M47.3 — the same markers for the ObScript `OnHit` / `OnHitWith` blocks of a struck
    // actor's script, and the combat transitions for `OnStartCombat` / `OnCombatEnd`.
    // Here rather than beside `obscript_object_system`: the projectile and trap hits
    // above are produced after that one runs.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_objects::obscript_hit_system);
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_objects::obscript_combat_system);
    // M47.3 — the magic effects running on actors: `ScriptEffectStart` / `Update` / `Finish`, value
    // changes over time, and the `OnMagicEffectHit` they raised when they landed. After the ObScript
    // systems whose `Cast` / `CastImmediateOnSelf` start them.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::magic_effects::magic_effect_system);
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::magic_effects::magic_hit_system);
    // M47.3 — magic shaders: advance their clocks, then glow (or restore) the surfaces they are on.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::magic_shaders::magic_shader_clock_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::magic_shader::magic_shader_tint_system);
    // M47.3 — `OnDestructionStageChange` for the objects a `DamageObject` moved to another stage.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_objects::obscript_destruction_system);
    // M47.3 — `Resurrect`: bring the actors back that scripts named this frame.
    scheduler.add_exclusive(Stage::Update, crate::systems::revive::revive_system);
    // M47.3 — `PushActorAway` (knock the actor down, then stand it up where it came to rest), `ForceFlee`,
    // `UseWeapon`, `ResetInventory`, `SetPCSleepHours`, `PlayMusic` and `EssentialDeathReload`: the app's
    // half of the commands that raise a request.
    scheduler.add_exclusive(Stage::Update, crate::systems::knockdown::push_actor_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::knockdown::get_up_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::flee::flee_system);
    // M42 — the UseWeapon PACK procedure raises its request first, so the volley starts this frame.
    scheduler.add_exclusive(Stage::Update, crate::systems::use_weapon_package_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::use_weapon_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::reset_inventory_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::sleep_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::music_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::reload_system);
    scheduler.add_exclusive(Stage::Update, crate::combat::chargen_catch_up_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::script_ui_request_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::actor_alpha_system);
    // M47.3 — the New Vegas challenges whose stat has risen far enough complete (reward script and all).
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::challenges::challenge_system);
}
