//! M47.3 — the app's half of the ObScript commands that hand it a request: `UseWeapon`, `ResetInventory`,
//! `SetPCSleepHours`, `PlayMusic` and `EssentialDeathReload`. (`PushActorAway` and `ForceFlee` have their
//! own systems, [`super::knockdown`] and [`super::flee`].)

use byroredux_core::ecs::components::{ActorVitals, Dead, GlobalTransform, Inventory, ItemInstanceId, Transform};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::Quat;
use byroredux_plugin::esm::records::ItemKind;
use byroredux_scripting::obscript_more::{PendingMusic, PendingSleep, ReloadRequest, ResetInventoryRequest, UseWeaponRequest};

// ── UseWeapon ──────────────────────────────────────────────────────────────────────────────────────

/// A `UseWeapon` volley under way. `NOT_SAVED_BY_DESIGN`: it lasts seconds, a save keeps the actor
/// standing.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UseWeaponRun {
    request: UseWeaponRequest,
    cooldown: f32,
    /// Seconds spent unable to use the weapon (the target out of reach or out of sight).
    waited: f32,
}

/// How long a volley waits for a shot it cannot take before the actor gives up.
const PATIENCE_SECS: f32 = 10.0;

impl Component for UseWeaponRun {
    type Storage = SparseSetStorage<Self>;
}

/// The yaw that points an actor's forward (-Z) along the horizontal direction `(dx, dz)`.
pub(crate) fn facing(dx: f32, dz: f32) -> Quat {
    Quat::from_rotation_y((-dx).atan2(-dz))
}

/// `UseWeapon`: the actor turns to its target and uses the weapon on it `shots` times — a ranged weapon
/// launches its projectile at the target's centre, a melee one strikes while the target is within reach.
/// The GECK's options (where to stand, crouching to reload, holding fire, always hit, no damage, the
/// target's place) are not modelled: the actor fires from where it stands.
pub(crate) fn use_weapon_system(world: &World, dt: f32) {
    // Start the volleys the scripts asked for.
    let asked = super::take_requests::<UseWeaponRequest>(world);
    if !asked.is_empty() {
        if let Some(mut runs) = world.query_mut::<UseWeaponRun>() {
            for (actor, request) in asked {
                log::info!("UseWeapon: actor {actor} uses {:08X} on {:08X} x{}", request.weapon, request.target, request.shots);
                runs.insert(actor, UseWeaponRun { request, cooldown: 0.0, waited: 0.0 });
            }
        } else {
            log::warn!("UseWeapon: the volley storage is not registered");
        }
    }
    let runs: Vec<(EntityId, UseWeaponRun)> = match world.query::<UseWeaponRun>() {
        Some(runs) if !runs.is_empty() => runs.iter().map(|(actor, run)| (actor, *run)).collect(),
        _ => return,
    };
    let Some(index) = world.try_resource::<crate::cell_loader::LoadedCellIndex>().map(|index| index.0.clone()) else {
        return;
    };
    for (actor, run) in runs {
        let target = byroredux_scripting::condition::resolve_entity_by_global_form_id(world, run.request.target);
        let actor_form = byroredux_scripting::obscript_state::reference_form(world, actor);
        let alive = |entity: EntityId| world.get::<ActorVitals>(entity).is_none() || !world.has::<Dead>(entity);
        let (Some(target), Some(actor_form)) = (target, actor_form) else {
            log::warn!("UseWeapon: actor {actor} stops: target {:08X} resolved {target:?}, actor reference {actor_form:?}", run.request.target);
            end_run(world, actor);
            continue;
        };
        if !alive(actor) || !alive(target) || run.request.shots == 0 {
            end_run(world, actor);
            continue;
        }
        // Face the target.
        let (Some(from), Some(to)) = (world.get::<GlobalTransform>(actor).map(|t| t.translation), world.get::<GlobalTransform>(target).map(|t| t.translation)) else {
            end_run(world, actor);
            continue;
        };
        if let Some(mut transforms) = world.query_mut::<Transform>() {
            if let Some(transform) = transforms.get_mut(actor) {
                transform.rotation = facing(to.x - from.x, to.z - from.z);
            }
        }
        let cooldown = run.cooldown - dt;
        if cooldown > 0.0 {
            store_run(world, actor, UseWeaponRun { cooldown, ..run });
            continue;
        }
        let weapon = index.items.get(&run.request.weapon).map(|item| &item.kind);
        let fired = match weapon {
            Some(ItemKind::Weapon { ammo_form, .. }) if *ammo_form != 0 => {
                if crate::systems::projectiles::resolve_shot(&index, run.request.weapon, 0).is_ok() {
                    if let Some(mut pending) = world.try_resource_mut::<byroredux_scripting::PendingProjectileLaunches>() {
                        pending.0.push(byroredux_scripting::projectile_requests::ProjectileLaunch { source: actor_form, weapon: run.request.weapon, ammo: 0, aim_at: run.request.target, harmless: run.request.harmless });
                    }
                    true
                } else if let Ok((weapon_damage, ammo_damage, ..)) = crate::systems::projectiles::resolve_ammunition(&index, run.request.weapon, 0) {
                    // A gun whose records name no projectile (Fallout's WEAP carries the PROJ, which is not
                    // decoded yet): the shot lands, for the weapon's and the ammunition's damage, when the
                    // target is in sight.
                    let clear = crate::systems::actor_has_line_of_sight(world, actor, target);
                    if clear {
                        if let Some(mut pending) = world.try_resource_mut::<byroredux_scripting::PendingTrapHits>() {
                            pending.0.push(byroredux_scripting::TrapHit { target, trap: actor, damage: if run.request.harmless { 0.0 } else { weapon_damage + ammo_damage.max(0.0) } });
                        }
                    }
                    clear
                } else {
                    false
                }
            }
            // A melee weapon strikes only what is within its reach.
            Some(ItemKind::Weapon { damage, .. }) => {
                let reach = crate::combat::attack_reach_bu(world, actor);
                let near = from.distance(to) <= reach;
                if near {
                    if let Some(mut pending) = world.try_resource_mut::<byroredux_scripting::PendingTrapHits>() {
                        pending.0.push(byroredux_scripting::TrapHit { target, trap: actor, damage: if run.request.harmless { 0.0 } else { *damage as f32 } });
                    }
                }
                near
            }
            _ => {
                log::warn!("UseWeapon: {:08X} is not a weapon; actor {actor} stops", run.request.weapon);
                end_run(world, actor);
                continue;
            }
        };
        if fired {
            log::debug!("UseWeapon: actor {actor} fires {:08X} at {:08X} ({} left)", run.request.weapon, run.request.target, run.request.shots - 1);
        }
        let waited = if fired { 0.0 } else { run.waited + dt };
        if waited >= PATIENCE_SECS {
            log::info!("UseWeapon: actor {actor} cannot reach {:08X} and gives up", run.request.target);
            end_run(world, actor);
            continue;
        }
        let remaining = if fired { run.request.shots - 1 } else { run.request.shots };
        let interval = crate::combat::attack_cooldown_seconds(world, actor).max(0.1);
        store_run(world, actor, UseWeaponRun { request: UseWeaponRequest { shots: remaining, ..run.request }, cooldown: interval, waited });
        if remaining == 0 {
            end_run(world, actor);
        }
    }
}

fn store_run(world: &World, actor: EntityId, run: UseWeaponRun) {
    if let Some(mut runs) = world.query_mut::<UseWeaponRun>() {
        runs.insert(actor, run);
    }
}

fn end_run(world: &World, actor: EntityId) {
    if let Some(mut runs) = world.query_mut::<UseWeaponRun>() {
        runs.remove(actor);
    }
}

// ── ResetInventory ─────────────────────────────────────────────────────────────────────────────────

/// `ResetInventory`: "all previously added, removed, or calculated objects will be lost" — a container
/// goes back to what its `CONT` record authors (leveled lists drawn afresh at the player's level). An
/// actor's carried stacks are tied to its equipment slots, which a reset would orphan, so an actor is
/// left as it is.
pub(crate) fn reset_inventory_system(world: &World, _dt: f32) {
    let asked: Vec<EntityId> = super::take_requests::<ResetInventoryRequest>(world).into_iter().map(|(entity, _)| entity).collect();
    if asked.is_empty() {
        return;
    }
    let Some(index) = world.try_resource::<crate::cell_loader::LoadedCellIndex>().map(|index| index.0.clone()) else {
        return;
    };
    for entity in asked {
        let base = world.get::<byroredux_scripting::SceneAliasCandidate>(entity).map(|candidate| candidate.base_form_id);
        let authored = base.and_then(|base| crate::cell_loader::references::authored_container_inventory(world, base, &index));
        match authored {
            Some(fresh) => {
                let replaced = replace_inventory(world, entity, fresh);
                release_instances(world, &replaced);
                log::info!("ResetInventory: container {entity} is back to its record");
            }
            None => log::warn!("ResetInventory: {entity} is not a container with a record; left as it is"),
        }
    }
}

/// Swap `entity`'s [`Inventory`] for `fresh`, handing back the stacks it held (empty when it had none).
fn replace_inventory(world: &World, entity: EntityId, fresh: Inventory) -> Vec<ItemInstanceId> {
    let Some(mut inventories) = world.query_mut::<Inventory>() else {
        return Vec::new();
    };
    let instances = inventories.get(entity).map(|old| old.items.iter().filter_map(|stack| stack.instance).collect()).unwrap_or_default();
    inventories.insert(entity, fresh);
    instances
}

/// Release the dropped stacks' [`ItemInstancePool`] entries, as consumption does, so a reset does not orphan them.
fn release_instances(world: &World, instances: &[ItemInstanceId]) {
    if instances.is_empty() {
        return;
    }
    if let Some(mut pool) = world.try_resource_mut::<byroredux_core::ecs::resources::ItemInstancePool>() {
        for &instance in instances {
            pool.release(instance);
        }
    }
}

// ── SetPCSleepHours ────────────────────────────────────────────────────────────────────────────────

/// `SetPCSleepHours`: run the game clock forward by the hours a script told the player to sleep (GECK:
/// "manually forces the player to sleep the specified number of hours"). The rest itself — healing, the
/// sleep screen — is not modelled; the clock, and everything that follows it, moves.
pub(crate) fn sleep_system(world: &World, _dt: f32) {
    let hours = match world.try_resource_mut::<PendingSleep>() {
        Some(mut pending) if pending.0 > 0.0 => std::mem::take(&mut pending.0),
        _ => return,
    };
    if let Some(mut time) = world.try_resource_mut::<crate::components::GameTimeRes>() {
        time.advance_hours(hours);
        log::info!("SetPCSleepHours: the clock runs {hours} h to day {} {:.2}", time.day, time.hour);
    }
}

// ── PlayMusic ──────────────────────────────────────────────────────────────────────────────────────

/// The crossfade a scripted track starts and stops with.
const SCRIPT_MUSIC_FADE_SECS: f32 = 1.0;

/// `PlayMusic`: play the named music type's track over what the cell is already playing (GECK: "will not
/// stop the currently playing music of the cell"), from the sound archives. #6226 — on the audio world's
/// scripted music layer: streamed (a stinger is tens of MB of PCM decoded whole), non-spatial on the
/// main track like the cell's music, looping when its positive dB marks it so, and replaced by the next
/// `PlayMusic`. A cell change or a load ends it ([`stop_script_music`]).
pub(crate) fn music_system(world: &World, _dt: f32) {
    let tracks: Vec<_> = match world.try_resource_mut::<PendingMusic>() {
        Some(mut pending) if !pending.0.is_empty() => std::mem::take(&mut pending.0),
        _ => return,
    };
    // One layer: of several asked for in a frame, the last is what would be left playing.
    let Some(track) = tracks.into_iter().last() else { return };
    // A headless session has nothing to play it on: no archive read, no decode.
    if !world.try_resource::<byroredux_audio::AudioWorld>().is_some_and(|audio| audio.is_active()) {
        return;
    }
    let key = format!("music\\{}", track.file.replace('/', "\\").to_ascii_lowercase());
    // The music is loose in `Data\Music` on Fallout 3 / New Vegas; an archive is tried first.
    let from_archive = world
        .try_resource::<crate::asset_provider::SoundArchiveProvider>()
        .and_then(|provider| provider.extract(&key))
        .map(byroredux_audio::load_streaming_sound_from_bytes);
    let sound = from_archive.or_else(|| {
        let (_, data) = crate::cli_args::esm_discovery_root(&crate::cli_args::effective_args()).ok()?;
        let path = find_case_insensitive(&data, &format!("Music\\{}", track.file))?;
        Some(byroredux_audio::load_streaming_sound_from_file(path))
    });
    let sound = match sound {
        Some(Ok(sound)) => sound,
        Some(Err(error)) => {
            log::warn!("PlayMusic: '{key}' failed to decode: {error}");
            return;
        }
        None => {
            log::warn!("PlayMusic: '{key}' is in no sound archive and not under the data folder's Music");
            return;
        }
    };
    // Only a negative level attenuates; a positive one is the loop marker, not a gain.
    let volume = if track.decibels < 0.0 { 10f32.powf(track.decibels / 20.0) } else { 1.0 };
    if let Some(mut audio) = world.try_resource_mut::<byroredux_audio::AudioWorld>() {
        audio.play_script_music(sound, volume, SCRIPT_MUSIC_FADE_SECS, track.looping);
        log::info!("PlayMusic: playing '{key}' at {volume:.2}{}", if track.looping { " (looping)" } else { "" });
    }
}

/// #6226 — end the scripted music layer: called where a cell is left (an interior unloaded, an exterior
/// session drained), which every door transition and every load passes through.
pub(crate) fn stop_script_music(world: &World) {
    if let Some(mut audio) = world.try_resource_mut::<byroredux_audio::AudioWorld>() {
        audio.stop_script_music(SCRIPT_MUSIC_FADE_SECS);
    }
}

/// `relative` (`\\`- or `/`-separated) under `root`, matching each component without regard to case
/// (the games' files are Windows-cased; this filesystem is not).
fn find_case_insensitive(root: &std::path::Path, relative: &str) -> Option<std::path::PathBuf> {
    let mut path = root.to_path_buf();
    for component in relative.split(['\\', '/']).filter(|part| !part.is_empty()) {
        let exact = path.join(component);
        path = if exact.exists() {
            exact
        } else {
            std::fs::read_dir(&path).ok()?.filter_map(Result::ok).find(|entry| entry.file_name().to_string_lossy().eq_ignore_ascii_case(component))?.path()
        };
    }
    Some(path)
}

// ── SetActorAlpha ──────────────────────────────────────────────────────────────────────────────────

/// The alpha each actor was last given and the meshes it was written to, so a change is applied once and
/// a return to full opacity takes the override back off. `NOT_SAVED_BY_DESIGN`: rebuilt from the saved
/// ledger value on the first frame.
#[derive(Debug, Default)]
pub(crate) struct ScriptAlphaApplied(std::collections::HashMap<u32, (f32, Vec<EntityId>)>);

impl byroredux_core::ecs::Resource for ScriptAlphaApplied {}

/// `SetActorAlpha <0..1>` (SAA): the meshes of the actor are drawn at that opacity — `AnimatedAlpha`, the
/// sink the renderer reads over the material's own alpha. 1 gives the material back.
pub(crate) fn actor_alpha_system(world: &World, _dt: f32) {
    use byroredux_core::ecs::components::AnimatedAlpha;
    let wanted = byroredux_scripting::obscript_more::actor_alphas(world);
    let applied_forms: Vec<u32> = world.try_resource::<ScriptAlphaApplied>().map(|applied| applied.0.keys().copied().collect()).unwrap_or_default();
    if wanted.is_empty() && applied_forms.is_empty() {
        return;
    }
    for &(form, alpha) in &wanted {
        let Some(entity) = byroredux_scripting::condition::resolve_entity_by_global_form_id(world, form) else { continue };
        let up_to_date = world.try_resource::<ScriptAlphaApplied>().is_some_and(|applied| applied.0.get(&form).is_some_and(|(current, _)| *current == alpha));
        if up_to_date {
            continue;
        }
        let meshes = crate::npc_spawn::loot_appearance::mesh_entities_under(world, entity);
        if let Some(mut alphas) = world.query_mut::<AnimatedAlpha>() {
            for &mesh in &meshes {
                if alpha < 1.0 {
                    alphas.insert(mesh, AnimatedAlpha(alpha));
                } else {
                    alphas.remove(mesh);
                }
            }
        }
        if let Some(mut applied) = world.try_resource_mut::<ScriptAlphaApplied>() {
            if alpha < 1.0 {
                applied.0.insert(form, (alpha, meshes));
            } else {
                applied.0.remove(&form);
            }
        }
    }
}

// ── menus, videos, ExitGame ────────────────────────────────────────────────────────────────────────

/// The screens and videos scripts asked for that the engine has no UI to show, most recent last: a
/// bounded log (`script.requests` prints it) so a request is neither dropped unseen nor queued for ever.
/// `NOT_SAVED_BY_DESIGN`: presentation.
#[derive(Debug, Default)]
pub(crate) struct ScriptRequestLog(pub(crate) std::collections::VecDeque<String>);

impl byroredux_core::ecs::Resource for ScriptRequestLog {}

const REQUEST_LOG_CAP: usize = 64;

/// Move the menus (`ShowMap`, the chargen screens, the casino games ...) and videos (`PlayBink`) scripts
/// asked for into the log, and carry out `ExitGame` — "plays credits and returns the game to the main
/// menu"; there is no main menu, so the game ends through the orderly shutdown.
pub(crate) fn script_ui_request_system(world: &World, _dt: f32) {
    use byroredux_scripting::obscript_more::{ExitGameRequest, PendingMenus, PendingVideos};
    let mut lines = Vec::new();
    if let Some(mut menus) = world.try_resource_mut::<PendingMenus>() {
        lines.extend(menus.0.drain(..).map(|menu| format!("menu {} form {:08X}", menu.menu, menu.form)));
    }
    if let Some(mut videos) = world.try_resource_mut::<PendingVideos>() {
        lines.extend(videos.0.drain(..).map(|file| format!("video {file} (no Bink decoder)")));
    }
    if !lines.is_empty() {
        if let Some(mut log) = world.try_resource_mut::<ScriptRequestLog>() {
            for line in lines {
                log::info!("script request: {line}");
                if log.0.len() >= REQUEST_LOG_CAP {
                    log.0.pop_front();
                }
                log.0.push_back(line);
            }
        }
    }
    let exit = world.try_resource_mut::<ExitGameRequest>().is_some_and(|mut request| std::mem::take(&mut request.0));
    if exit {
        log::info!("ExitGame: ending the game through the orderly shutdown");
        if let Some(mut quit) = world.try_resource_mut::<crate::app_events::GracefulExitRequested>() {
            quit.0 = true;
        }
    }
}

// ── EssentialDeathReload ───────────────────────────────────────────────────────────────────────────

/// `EssentialDeathReload`: the death-reload — the message was shown when the command ran; the game goes
/// back to the last save, as it does when the player dies.
pub(crate) fn reload_system(world: &World, _dt: f32) {
    let message = match world.try_resource_mut::<ReloadRequest>() {
        Some(mut request) => request.0.take(),
        None => return,
    };
    if let Some(message) = message {
        match crate::save_io::queue_player_save_action(world, crate::save_io::PlayerSaveAction::Quickload) {
            Ok(()) => log::info!("EssentialDeathReload ({message:?}): loading the last save"),
            Err(why) => log::warn!("EssentialDeathReload ({message:?}): {why}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::math::Vec3;

    #[test]
    fn facing_points_the_forward_axis_at_the_target() {
        for (dx, dz) in [(1.0f32, 0.0f32), (0.0, 1.0), (-3.0, 4.0), (0.0, -2.0)] {
            let forward = facing(dx, dz) * -Vec3::Z;
            let want = Vec3::new(dx, 0.0, dz).normalize();
            assert!((forward - want).length() < 1e-5, "({dx},{dz}) -> {forward:?}");
        }
    }

    /// #6269 — a reset drops the old stacks, so their pool entries go back to the pool with them.
    #[test]
    fn resetting_an_inventory_releases_the_dropped_stacks_instances() {
        use byroredux_core::ecs::components::ItemStack;
        use byroredux_core::ecs::resources::{ItemInstance, ItemInstancePool};
        let mut world = World::new();
        let mut pool = ItemInstancePool::new();
        let kept = pool.allocate(ItemInstance::default());
        world.insert_resource(pool);
        let container = world.spawn();
        let mut old = Inventory::new();
        let mut instanced = ItemStack::new(0x10, 1);
        instanced.instance = Some(kept);
        old.push(instanced);
        old.push(ItemStack::new(0x11, 3));
        world.insert(container, old);
        let mut fresh = Inventory::new();
        fresh.push(ItemStack::new(0x12, 1));

        let dropped = replace_inventory(&world, container, fresh);
        release_instances(&world, &dropped);

        assert_eq!(dropped, vec![kept]);
        assert_eq!(world.resource::<ItemInstancePool>().live_count(), 0);
        let now = world.get::<Inventory>(container).unwrap();
        assert_eq!(now.items.len(), 1);
        assert_eq!(now.items[0].base_form_id, 0x12);
    }

    /// #6226 — `PlayMusic` goes nowhere near the one-shot pool (no spatial sub-track, no
    /// `oneshots_requested` count), and a headless session drains the request without touching the
    /// archives or decoding anything.
    #[test]
    fn play_music_is_no_one_shot_and_a_headless_session_only_drains_it() {
        use byroredux_scripting::obscript_more::MusicTrack;
        let mut world = World::new();
        world.insert_resource(byroredux_audio::AudioWorld::headless());
        let track = |file: &str| MusicTrack { form_id: 1, file: file.into(), looping: false, decibels: -6.0 };
        world.insert_resource(PendingMusic(vec![track("a.mp3"), track("b.mp3")]));
        music_system(&world, 0.0);
        assert!(world.resource::<PendingMusic>().0.is_empty());
        let audio = world.resource::<byroredux_audio::AudioWorld>();
        assert_eq!(audio.oneshots_requested(), 0);
        assert!(!audio.is_script_music_active());
    }

    /// #6226 — the scripted layer ends where a cell is left: both teardown choke points stop it.
    #[test]
    fn leaving_a_cell_stops_the_scripted_music() {
        let source = include_str!("../cell_loader/transition.rs");
        for teardown in ["pub fn unload_current_interior(", "pub(crate) fn clear_current_exterior_identity("] {
            let body = source.split_once(teardown).and_then(|(_, rest)| rest.split_once("\n}\n")).map(|(body, _)| body).unwrap();
            assert!(body.contains("stop_script_music(world)"), "{teardown} must stop the scripted music");
        }
    }
}
