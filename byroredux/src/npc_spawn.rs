//! NPC spawning — assemble a placed NPC actor entity from its NPC_,
//! RACE, HDPT/EYES/HAIR, and FaceGen content.
//!
//! M41.0 lands the spawn function itself; this Phase 0 file ships the
//! game-variant path helpers that the spawn function will consume.
//! Each helper maps (game, gender) → a vanilla archive path string for
//! the per-game content layout.

use byroredux_core::animation::AnimationClipRegistry;
use byroredux_core::animation::AnimationPlayer;
use byroredux_core::ecs::components::{
    EquipmentSlots, EquippedWeapon, FactionRanks, GlobalTransform, Inventory, InventoryIndex,
    ItemStack, MotionType, Name, Parent, RigidBodyData, Transform,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{EsmIndex, ItemKind, NpcRecord, RaceRecord};
use byroredux_renderer::VulkanContext;

/// AI-package behavior tagging — split out under #2198. See the module's own
/// docs for why it lives beside this file rather than in it.
pub(crate) mod ai_package;
pub(crate) use ai_package::ambient_ai_package_system;
use ai_package::apply_ai_package_behavior;
mod resumable;
pub(crate) use resumable::{NpcSpawnJob, NpcSpawnProgress};

use crate::anim_convert::convert_nif_clip;
use crate::asset_provider::{MaterialProvider, TextureProvider};
use crate::helpers::add_child;
use crate::scene::load_nif_bytes_with_skeleton;

use byroredux_plugin::equip::Gender;

/// Path inside the meshes archive for the default humanoid skeleton.
///
/// Returns `None` for game variants that do not pre-bake a singleton
/// skeleton path at this convention — currently no targeted variant
/// returns `None`, but the optional return is preserved so future
/// per-race skeleton lookup (creatures, bestiary) can route through
/// the same helper without an API break.
///
/// Vanilla path table verified by listing every archive (re-verified
/// 2026-05-26 against FO4/FO76/Starfield BA2s; the prior 2026-04-28
/// pass mis-extended the Skyrim path to FO4+ and missed the folder-name
/// change Bethesda made after Skyrim):
///
/// - **FNV / FO3** ship a single `meshes\characters\_male\skeleton.nif`
///   used by both genders. There is no `_female/skeleton.nif`
///   sibling in vanilla content (BSA scan: 0 hits).
/// - **Skyrim** (LE/SE) ships the unified
///   `meshes\actors\character\character assets\skeleton.nif` — note
///   the **space** in `character assets`. The `skeletonbeast.nif`
///   sibling is the Argonian/Khajiit variant; not handled here yet
///   (creature-race spawning is Phase 3+).
/// - **FO4 / FO76** ship the same shape but renamed the folder to
///   `characterassets` (no space). Pre-fix this function returned the
///   Skyrim-shaped path for FO4/FO76 too, so every NPC in every FO4
///   interior failed the skeleton lookup and silently rendered as
///   floating equipment (`F1` of the 2026-05-26 Fallout symptom
///   sweep). Verified against `Fallout4 - Meshes.ba2`: 0 files match
///   the space form, 1 file matches the no-space form.
/// - **Starfield** moved humanoids out of `\character\` entirely —
///   the skeleton is at `meshes\actors\human\characterassets\skeleton.nif`.
///   Same no-space convention as FO4/FO76.
///
/// Oblivion is not yet a target for NPC spawning (M41.0 closes on
/// FNV first); the path is the same as FNV's by convention.
/// Stamp a [`FactionRanks`] component on the NPC's placement root from its
/// `NPC_` `SNAM` faction list, so the M47.1 `GetFactionRank` condition can
/// read it (#1665). No-op when the NPC declares no factions. Faction ids are
/// carried verbatim from the record (NPC source space) — see `FactionRanks`.
fn stamp_faction_ranks(world: &mut World, placement_root: EntityId, npc: &NpcRecord) {
    if npc.factions.is_empty() {
        return;
    }
    world.insert(
        placement_root,
        FactionRanks::from_pairs(npc.factions.iter().map(|f| (f.faction_form_id, f.rank))),
    );
}

/// Stamp an [`ActorValues`] component on the NPC's placement root, derived
/// from the active game's authored/derived character rules, so condition and
/// combat systems read the same values. No-op when the derivation yields
/// nothing or the index cannot resolve the required records.
fn stamp_actor_values(
    world: &mut World,
    placement_root: EntityId,
    npc: &NpcRecord,
    index: &EsmIndex,
) {
    let pairs = byroredux_plugin::esm::records::derive_npc_actor_values(npc, index);
    if pairs.is_empty() {
        return;
    }
    let health = index
        .health_actor_value_key()
        .filter(|health| pairs.iter().any(|(form_id, _)| form_id == health));
    world.insert(
        placement_root,
        byroredux_core::ecs::components::ActorValues::from_pairs(pairs),
    );
    if let Some(health) = health {
        world.insert(
            placement_root,
            byroredux_core::ecs::components::ActorVitals { health },
        );
    }
}

/// Stamp a [`CreatureAttack`] on the placement root from the `CREA`
/// record's authored `DATA.Damage` (#3762).
///
/// `CreatureStats::damage` reached the parser and stopped there. #3390 gave
/// creatures SPECIAL + Health, which made them melee participants under the
/// shipped `combat_damage_system` — all of them swinging for
/// `combat.rs`'s flat `UNARMED_DAMAGE`, because the one number that defines
/// a creature's attack had no reader. Measured on the vanilla masters: 692
/// FNV and 186 FO3 creatures author a non-zero damage AND carry no
/// inventory `WEAP`, so they resolved through the no-weapon arm — a
/// Deathclaw hitting for 8 instead of 125.
///
/// No-op for `NPC_` (no `creature_stats`) and for a creature whose `DATA`
/// leaves damage at zero or negative: absence means "no authored attack",
/// which keeps the existing `UNARMED_DAMAGE` baseline the answer rather
/// than materialising an actor that attacks for nothing.
fn stamp_creature_attack(world: &mut World, placement_root: EntityId, npc: &NpcRecord) {
    let Some(stats) = npc.creature_stats else {
        return;
    };
    if stats.damage <= 0 {
        return;
    }
    world.insert(
        placement_root,
        byroredux_core::ecs::components::CreatureAttack {
            damage: f32::from(stats.damage),
        },
    );
}

/// The actor's effective level for anything that treats level as a number —
/// re-exported from the plugin crate, where it lives beside the `NPC_` record
/// whose overloaded `level` field it decodes.
///
/// #3171 — this used to be defined here, and both copies made of it drifted to
/// `.max(1)` on the non-multiplier branch (see the definition's docs). It now
/// sits at the ESM-record boundary so the binary imports the rule instead of
/// re-deriving it.
pub(crate) use byroredux_plugin::esm::records::effective_actor_level;

/// Stamp the CHARAL structural components — [`CharacterLevel`],
/// [`Background`], and [`Perks`] — on the NPC's placement root. Level +
/// race/class come from every game's `NPC_`; perks from FO4+ `PRKR`.
/// Complements [`stamp_actor_values`] (the numeric substrate): together they
/// land the full canonical character state an actor carries at spawn.
///
/// #2956 — `Use Stats`/`Use Traits` `TPLT` template inheritance is resolved
/// first, the same way [`resolve_inherited_inventory`] already resolves
/// `Use Inventory`: a `Lvl*` shell NPC's own level/class/race are frequently
/// not what the engine actually uses once its `template_flags` says to
/// inherit them.
///
/// [`resolve_inherited_inventory`]: byroredux_plugin::equip::resolve_inherited_inventory
fn stamp_character_components(
    world: &mut World,
    placement_root: EntityId,
    npc: &NpcRecord,
    index: &EsmIndex,
) {
    use byroredux_core::character::{Background, CharacterLevel, PerkRank, Perks};
    use byroredux_plugin::equip::{resolve_inherited_stats, resolve_inherited_traits};

    // The shell's own level gates which `LVLN` tier a chained template
    // resolves to — same contract `resolve_inherited_inventory` already
    // uses at its own call site below.
    let shell_level = effective_actor_level(npc);
    let stats_npc = resolve_inherited_stats(npc, shell_level, index);
    let traits_npc = resolve_inherited_traits(npc, shell_level, index);

    // Level: the resolved stats source's level (its own when `Use Stats`
    // isn't set or doesn't resolve). NPCs carry no XP.
    //
    // #2955 — routed through `effective_actor_level` so a PC-level-multiplier
    // record contributes its ACBS `calcMin`, not the raw multiplier. Writing
    // the multiplier here put 268 FNV / ~190 FO3 base actors two to three
    // orders of magnitude out, and this field feeds `DerivedInput::LEVEL`,
    // the leveling model and the M45 save snapshot.
    world.insert(
        placement_root,
        CharacterLevel {
            level: effective_actor_level(stats_npc).max(0) as u16,
            xp: 0,
        },
    );
    // Provenance: race (from the Use-Traits source) + class (from the
    // Use-Stats source; 0 = none), reused by runtime leveling.
    world.insert(
        placement_root,
        Background {
            race_form_id: traits_npc.race_form_id,
            class_form_id: stats_npc.class_form_id,
        },
    );
    // Perks (FO4+ `PRKR`) — skip the component entirely when the NPC has none.
    if !npc.perks.is_empty() {
        world.insert(
            placement_root,
            Perks {
                entries: npc
                    .perks
                    .iter()
                    .map(|&(perk_form_id, rank)| PerkRank { perk_form_id, rank })
                    .collect(),
            },
        );
    }
}

/// Build the per-game [`CharacterRuleset`](byroredux_core::character::CharacterRuleset)
/// from the parsed AVIF set. The parser-selected character profile owns the
/// per-game ruleset choice; this consumer only supplies authored FormID
/// resolution.
pub fn build_character_ruleset(
    index: &EsmIndex,
) -> Option<byroredux_core::character::CharacterRuleset> {
    let resolve = |editor_id: &str| index.actor_value_form_id(editor_id);
    let gmst = |editor_id: &str| index.game_setting_float(editor_id);
    index.character_rules.build_ruleset(resolve, gmst)
}

/// Build the per-game [`MeleeDamageConfig`](byroredux_core::character::MeleeDamageConfig)
/// resource — the resolved AVIF id for the AVIF-backed additive Melee Damage
/// row (`STR × 0.5` on FO3/FNV, #3092). `None` means this game authors no
/// `MeleeDamage` AVIF (FO4, TES) — the combat consumer treats that exactly
/// like a missing [`CharacterRuleset`](byroredux_core::character::CharacterRuleset)
/// row: fall back to the flat weapon/unarmed baseline.
pub fn build_melee_damage_config(
    index: &EsmIndex,
) -> Option<byroredux_core::character::MeleeDamageConfig> {
    let resolve = |editor_id: &str| index.actor_value_form_id(editor_id);
    byroredux_core::character::melee_damage_config(resolve)
}

/// #1698 — keyframe a live NPC's ragdoll bones.
///
/// Skyrim (and FO3/FNV/Oblivion) author each skeleton ragdoll bone's bhk body
/// as `MO_SYS_DYNAMIC`, but a *living* actor's ragdoll must be **keyframed to
/// the animation** — driven by the animated skeleton, inert and hittable —
/// and only free-simulate on death. Importing them as free Dynamic bodies lets
/// ~18 bones/NPC collapse and free-fall (nothing drives them, no floor beneath
/// the spawn), and ~480+ such bodies across a dense interior pin
/// `physics_sync_system`'s dynamic solver at ~140 ms/frame for ~28 s
/// (Dragonsreach RT-1).
///
/// Flip each skeleton bone's Dynamic collision body to [`MotionType::Keyframed`]
/// **before** the first `physics_sync_system` registers it with Rapier, so it
/// registers as a kinematic body. `push_kinematic` then drives it from the
/// bone's animation-written `GlobalTransform` each frame (skipping idle bones),
/// keeping it out of the dynamic solver entirely. Death-time ragdoll activation
/// (`RagdollActive` / `build_ragdoll`) is unaffected — it rebuilds the
/// simulated ragdoll separately.
///
/// #2873 — each skeleton bone that carries a collision shape also gets an
/// [`ActorBoneCollider`](byroredux_physics::ActorBoneCollider) marker, so
/// registration files its colliders under
/// [`ACTOR_BONE_GROUP`](byroredux_physics::ACTOR_BONE_GROUP) and downward
/// floor probes skip them. Without it, `locomotion::step_toward`'s ground-snap
/// ray — cast from 256 BU above the actor's own root — meets the actor's
/// upper-body bone before it reaches the floor and re-seats the root at that
/// bone's height; because the bones are then driven from the root's
/// `GlobalTransform`, the next tick casts from higher still. That is a
/// monotonic elevator, not a one-off offset: walking NPCs ascend out of the
/// cell. `exclude_rigid_body` cannot fix it — each bone is a separate body.
fn keyframe_live_ragdoll_bones(
    world: &mut World,
    actor_root: EntityId,
    skel_map: &std::collections::HashMap<std::sync::Arc<str>, EntityId>,
) {
    use byroredux_core::ecs::components::collision::CollisionShape;

    for &bone in skel_map.values() {
        if let Some(body) = world.get_mut::<RigidBodyData>(bone) {
            if body.motion_type == MotionType::Dynamic {
                body.motion_type = MotionType::Keyframed;
            }
        }
        // Tag every bone that actually registers a collider, whatever its
        // authored motion type — a bone shipped Keyframed upstream is just as
        // self-hittable as one flipped above.
        let has_shape = world
            .query::<CollisionShape>()
            .is_some_and(|q| q.contains(bone));
        if has_shape {
            world.insert(bone, byroredux_physics::ActorBoneCollider);
            world.insert(bone, byroredux_physics::ActorColliderOwner(actor_root));
        }
    }
}

pub fn humanoid_skeleton_path(game: GameKind) -> Option<&'static str> {
    match game {
        GameKind::Oblivion | GameKind::Fallout3NV => Some(r"meshes\characters\_male\skeleton.nif"),
        GameKind::Skyrim => Some(r"meshes\actors\character\character assets\skeleton.nif"),
        GameKind::Fallout4 | GameKind::Fallout76 => {
            Some(r"meshes\actors\character\characterassets\skeleton.nif")
        }
        GameKind::Starfield => Some(r"meshes\actors\human\characterassets\skeleton.nif"),
    }
}

/// Split a creature's `CREA` MODL into `(skeleton path, directory prefix)`,
/// both normalised to the archive's `meshes\…` convention.
///
/// #2567 — a creature's whole asset set lives in one directory, keyed off
/// MODL. Verified against `Oblivion.esm` + `Oblivion - Meshes.bsa`:
/// `MODL = "Creatures\Rat\Skeleton.NIF"`, and beside it in the archive sit
/// `rat.nif`, `head.nif`, `whiskers.nif` (the `NIFZ` list) plus `idle.kf`,
/// `forward.kf`, `turnleft.kf` … So the directory *is* the creature's
/// namespace, and every path this module derives for a creature is that
/// prefix plus an authored filename — never a guessed one.
///
/// `None` when the record has no MODL (nothing to anchor to).
pub fn creature_skeleton_and_dir(model_path: &str) -> Option<(String, String)> {
    if model_path.is_empty() {
        return None;
    }
    let skeleton = normalize_mesh_path(model_path).into_owned();
    // Both separators: MODL is authored with backslashes, but a mod tool can
    // emit forward slashes and the archive layer accepts either.
    let dir_end = skeleton.rfind(['\\', '/'])?;
    let dir = skeleton[..=dir_end].to_owned();
    Some((skeleton, dir))
}

/// Resolve a creature's `NIFZ` body-part filenames against its MODL
/// directory (#2567). Entries are authored bare (`Rat.NIF`), so they are
/// joined to the prefix from [`creature_skeleton_and_dir`] rather than
/// normalised independently — a bare name has no `meshes\` to normalise.
/// Case is left as authored; the archive layer lowercases on lookup.
pub fn creature_body_paths(dir: &str, body_part_models: &[String]) -> Vec<String> {
    body_part_models
        .iter()
        .filter(|name| !name.is_empty())
        .map(|name| format!("{dir}{name}"))
        .collect()
}

/// A creature's own idle clip, beside its skeleton (#2567).
///
/// Creatures do not share the humanoid `idle.kf` — a rat's skeleton has
/// none of the humanoid bone names, so the shared per-cell idle pool
/// animates nothing. Every vanilla Oblivion creature directory ships its
/// own `idle.kf` (verified over `Oblivion - Meshes.bsa`); a creature whose
/// directory lacks one simply gets no idle, which is the pre-#2567 status
/// quo rather than a regression.
pub fn creature_idle_kf_path(dir: &str) -> String {
    format!("{dir}idle.kf")
}

/// Hardcoded vanilla body NIF paths for KF-era humanoids.
///
/// The TES4/Fallout RACE `MODL` entries describe head parts, not the body.
/// Body meshes live under `characters\_male` in all three games, but the
/// directory name is historical rather than a gender discriminator: shipped
/// female meshes use `female*` filename prefixes in that same directory.
/// FO3/FNV child races set RACE DATA flag `0x04` and select the corresponding
/// `child*upperbody` mesh; hands remain the gendered adult hand meshes because
/// those are the only hand variants the archives ship (#3037).
///
/// Archive listings re-verified 2026-08-17 against Oblivion, FO3, and FNV.
/// Every returned body variant skins against the same canonical skeleton from
/// [`humanoid_skeleton_path`]. Skyrim+ uses its separate FaceGen body path and
/// therefore returns an empty slice here.
pub fn humanoid_body_paths(
    game: GameKind,
    gender: Gender,
    is_child: bool,
) -> &'static [&'static str] {
    match (game, gender, is_child) {
        (GameKind::Fallout3NV, Gender::Male, true) => &[
            r"meshes\characters\_male\childupperbody.nif",
            r"meshes\characters\_male\lefthand.nif",
            r"meshes\characters\_male\righthand.nif",
        ],
        (GameKind::Fallout3NV, Gender::Female, true) => &[
            r"meshes\characters\_male\childfemaleupperbody.nif",
            r"meshes\characters\_male\femalelefthand.nif",
            r"meshes\characters\_male\femalerighthand.nif",
        ],
        (GameKind::Oblivion | GameKind::Fallout3NV, Gender::Female, _) => &[
            r"meshes\characters\_male\femaleupperbody.nif",
            r"meshes\characters\_male\femalelefthand.nif",
            r"meshes\characters\_male\femalerighthand.nif",
        ],
        (GameKind::Oblivion | GameKind::Fallout3NV, Gender::Male, _) => &[
            r"meshes\characters\_male\upperbody.nif",
            r"meshes\characters\_male\lefthand.nif",
            r"meshes\characters\_male\righthand.nif",
        ],
        (
            GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield,
            _,
            _,
        ) => &[],
    }
}

/// Biped slots represented by one loose KF-era naked-body mesh.
/// Fallout splits left/right hands into distinct slots; Oblivion has one
/// shared Hand bit even though its archive also stores two hand NIFs.
fn humanoid_body_path_biped_mask(game: GameKind, path: &str) -> u32 {
    if path.ends_with("upperbody.nif") {
        return 1 << 2;
    }
    match game {
        GameKind::Fallout3NV if path.ends_with("lefthand.nif") => 1 << 3,
        GameKind::Fallout3NV if path.ends_with("righthand.nif") => 1 << 4,
        GameKind::Oblivion if path.ends_with("lefthand.nif") || path.ends_with("righthand.nif") => {
            1 << 4
        }
        _ => 0,
    }
}

/// Parse a `.kf` clip at `kf_path` from the texture provider's mesh
/// archives, convert it through `byroredux_nif::anim::import_kf` →
/// [`AnimationClip`], register the **first** clip with the
/// [`AnimationClipRegistry`], and return its handle.
///
/// Returns `None` when the path isn't archived or the file produces
/// zero clips (malformed `.kf`s do this — defensive). Vanilla
/// `meshes\characters\_male\idle.kf` yields exactly one clip.
///
/// The handle is intended to be **shared across every NPC in a cell
/// load** — Phase 2 calls this once per `load_references` invocation
/// and threads the result through each [`NpcSpawnJob::runtime`] call so
/// the clip lands in the registry at most once per cell.
pub fn load_idle_clip(
    world: &mut World,
    tex_provider: &TextureProvider,
    game: GameKind,
) -> Option<u32> {
    if !game.has_kf_animations() {
        return None;
    }
    let kf_path = humanoid_default_idle_kf_path(game)?;
    load_kf_clip_by_path(world, tex_provider, kf_path)
}

/// Load the sit-**enter** transition clip once per cell so
/// `sandbox_seat_system` — which has no archive provider — can park a seated
/// actor's `AnimationPlayer` on its final (fully-seated) frame via the
/// registry. Returns `(handle, duration)`: the seat system holds the clip at
/// `local_time = duration` with `playing = false`, which yields the seated end
/// pose (see the M42.1 diagnosis in `systems::sandbox`). Path-keyed memoised
/// (#790). `None` for Skyrim+/Havok games or when the clip isn't archived.
pub fn load_sit_clip(
    world: &mut World,
    tex_provider: &TextureProvider,
    game: GameKind,
) -> Option<(u32, f32)> {
    let kf_path = sandbox_sit_enter_kf_path(game)?;
    let handle = load_kf_clip_by_path(world, tex_provider, kf_path)?;
    // The held-frame time is the clip duration; fetch it once now so the seat
    // system doesn't re-query the registry per assignment.
    let duration = world
        .resource::<AnimationClipRegistry>()
        .get(handle)
        .map(|c| c.duration)
        .unwrap_or(0.0);
    Some((handle, duration))
}

/// Archive path of the humanoid **sit-enter** transition the Sandbox seat
/// procedure holds at its final frame. Unlike the `dynamicidle_*` sit *loops*
/// (which fold the limbs but carry no `Bip01`/`NonAccum` channel, so the body
/// never lowers onto the seat — the M42.0 float bug), this enter clip drives
/// the accum root + `Bip01 NonAccum` down onto the seat; its last frame is a
/// complete, grounded seated pose. Verified present in vanilla FNV
/// `Fallout - Meshes.bsa` (BSA scan 2026-07-12). `None` for games whose actors
/// animate through Havok `.hkx` (Skyrim+/FO4+) or whose furniture sit-anim path
/// hasn't been verified (Oblivion — deferred). FO3 shares the FNV path. The
/// enter↔furniture pairing is game-hardcoded; Phase A uses one verified chair
/// enter for all sit markers (per-type mapping is Phase C).
pub fn sandbox_sit_enter_kf_path(game: GameKind) -> Option<&'static str> {
    match game {
        GameKind::Fallout3NV => Some(r"meshes\characters\_male\idleanims\chairskirt_leftenter.kf"),
        GameKind::Oblivion
        | GameKind::Skyrim
        | GameKind::Fallout4
        | GameKind::Fallout76
        | GameKind::Starfield => None,
    }
}

/// Shared KF-clip loader: fast-path the registry by `kf_path`, else
/// extract from the mesh archives, parse, `import_kf`, convert the first
/// clip, and register it path-keyed (#790). Returns the clip handle, or
/// `None` when the KF is absent / unparseable / empty. Backs both
/// [`load_idle_clip`] and [`load_sit_clip`].
fn load_kf_clip_by_path(
    world: &mut World,
    tex_provider: &TextureProvider,
    kf_path: &str,
) -> Option<u32> {
    // Fast path: clip already registered for this path. Skips the BSA
    // extract + NIF parse + channel conversion entirely (#790).
    if let Some(handle) = world
        .resource::<AnimationClipRegistry>()
        .get_by_path(kf_path)
    {
        return Some(handle);
    }

    let kf_bytes = match tex_provider.extract_mesh(kf_path) {
        Some(b) => b,
        None => {
            log::debug!(
                "KF clip '{}' not found in mesh archives — actors in this \
                 cell will not use it",
                kf_path,
            );
            return None;
        }
    };
    let nif_scene = match byroredux_nif::parse_nif(&kf_bytes) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("KF clip '{}' failed to parse: {}", kf_path, e);
            return None;
        }
    };
    let mut clips = byroredux_nif::anim::import_kf(&nif_scene);
    if clips.is_empty() {
        log::warn!("KF clip '{}' produced zero clips — skipping", kf_path);
        return None;
    }
    let nif_clip = clips.remove(0);
    let clip_name = nif_clip.name.clone();
    let duration = nif_clip.duration;
    let channel_count = nif_clip.channels.len();
    let handle = {
        let mut pool = world.resource_mut::<StringPool>();
        let clip = convert_nif_clip(&nif_clip, &mut pool);
        drop(pool);
        let mut registry = world.resource_mut::<AnimationClipRegistry>();
        registry.get_or_insert_by_path(kf_path.to_string(), || clip)
    };
    log::info!(
        "KF clip '{}' registered from '{}' ({:.2}s, {} channels) → handle {}",
        clip_name,
        kf_path,
        duration,
        channel_count,
        handle,
    );
    Some(handle)
}

/// Load the shared per-cell idle-clip **pool** — the set of generic
/// standing idles an NPC can be assigned at spawn (`pick_idle_handle`).
/// Each clip is registered once (path-keyed via `load_idle_clip` →
/// `AnimationClipRegistry::get_or_insert_by_path`, #790) and its handle
/// collected; re-entry across cell loads is a HashMap hit.
///
/// **Pool contents (M41.5 Phase A2).** Today the pool is the single
/// verified generic standing idle, `mtidle.kf` — a BSA scan of vanilla
/// FNV (`Fallout - Meshes.bsa`, 2026-07-11) confirmed it is the *only*
/// unconditional full-body standing idle: every other candidate is
/// either weapon-stance-specific (`1hmidle`/`2hlidle` — wrong on an
/// unarmed civilian) or a `3rdp_specialidle_*` context idle
/// (`barkeeper*`, `blacksmith`, `workbench*` — need a matching furniture
/// / AI context to not mime in empty air). Assigning those blind would
/// be a guess (`feedback_no_guessing`); they slot into the pool only
/// once the IDLE-record condition tree (DATA/ANAM/CTDA) or AI-package
/// context lands. Until then the per-NPC *variety* comes from
/// `idle_desync` (phase + speed), not from clip diversity — the size-1
/// pool is an intentional floor, and this fn is the extension point.
///
/// Returns an empty `Vec` for Havok-animation games (Skyrim+/FO4+) or
/// when the KF isn't archived — those NPCs spawn without an idle.
pub fn load_idle_pool(
    world: &mut World,
    tex_provider: &TextureProvider,
    game: GameKind,
) -> Vec<u32> {
    // One verified entry today; extend here (behind the no-guessing gate
    // above) as safe generic idles are confirmed.
    load_idle_clip(world, tex_provider, game)
        .into_iter()
        .collect()
}

/// Build a sidecar path next to the given head NIF, swapping the
/// `.nif` extension for the requested `extension` (e.g. `"egm"`,
/// `"egt"`, `"tri"`). FaceGen co-locates all four sidecars in the
/// same archive directory so this is purely a path-string rewrite.
///
/// Returns `None` when the input doesn't end in `.nif` (case-
/// insensitive) — defensive against a head MODL that points at an
/// unexpected file type.
pub fn facegen_sidecar_path(head_nif_path: &str, extension: &str) -> Option<String> {
    let lower = head_nif_path.to_ascii_lowercase();
    let stem = lower.strip_suffix(".nif")?;
    let stem_len = stem.len();
    let mut out = String::with_capacity(stem_len + 1 + extension.len());
    out.push_str(&head_nif_path[..stem_len]);
    out.push('.');
    out.push_str(extension);
    Some(out)
}

use crate::asset_provider::normalize_mesh_path;

/// Path inside the meshes archive for the default idle animation
/// (`.kf` keyframe clip) the NPC plays on loop when no AI package
/// drives a different clip.
///
/// Returns `None` for game variants that do not ship `.kf` clips.
/// **Skyrim and later use Havok Behavior Format `.hkx`** — there is
/// no `.kf` sibling for any humanoid actor in vanilla SSE / FO4 / FO76
/// / Starfield archives (BSA scan: 0 `.kf` hits across Meshes0 +
/// Meshes1 + Animations BSAs in Skyrim SE on 2026-04-28). Animating
/// SSE+ actors lands once a `.hkx` parser stub is wired — folded into
/// M41.1 follow-up.
///
/// FNV / FO3 ship the canonical resting-state idle as
/// `meshes\characters\_male\locomotion\mtidle.kf` (move-type idle —
/// the standing-still loop the engine plays when no AI package
/// drives a different clip). Verified via vanilla BSA scan
/// 2026-04-29; the more obvious `_male\idle.kf` does NOT exist in
/// vanilla (`idleanims/` carries 962 specific clips like `talk_*`,
/// `chair_*`, `dlcanch*`, but no plain `idle.kf` base). Per-NPC
/// overrides from IDLE form records and AI packages slot in on top
/// once M42 / M47 land.
pub fn humanoid_default_idle_kf_path(game: GameKind) -> Option<&'static str> {
    match game {
        GameKind::Oblivion | GameKind::Fallout3NV => {
            Some(r"meshes\characters\_male\locomotion\mtidle.kf")
        }
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield => None,
    }
}

/// Speed jitter half-range for idle desync — NPC playback rate lands in
/// `[1 - IDLE_SPEED_JITTER, 1 + IDLE_SPEED_JITTER]`. Small enough to read
/// as natural variation, large enough that two NPCs on the same clip
/// visibly drift apart within a few seconds instead of breathing in
/// lockstep. `± 8 %` keeps the loop period within ~half a second of the
/// authored `mtidle.kf` cadence.
const IDLE_SPEED_JITTER: f32 = 0.08;

/// Deterministically derive a per-NPC idle start phase and playback speed
/// from the NPC's stable FormId, so every actor sharing one idle clip
/// starts at a different point in the loop and drifts at a slightly
/// different rate. Without this, a cell full of NPCs plays the single
/// `mtidle.kf` in perfect sync — the "mannequins breathing together"
/// tell. See M41.5 Phase A1.
///
/// Deterministic (a FormId hash, **not** an RNG): the same actor produces
/// the same phase every load, so a save/reload or a re-streamed cell
/// re-seeds identically — matching the determinism the ECS + save paths
/// assume.
///
/// Returns `(start_time, speed)`:
/// - `start_time ∈ [0, duration)` — the seed for `AnimationPlayer.local_time`
///   (and `prev_time`, so no spurious text-key events fire on the first
///   tick). `0.0` when `duration` is non-positive (empty / released clip).
/// - `speed ∈ [1 - IDLE_SPEED_JITTER, 1 + IDLE_SPEED_JITTER]`.
pub fn idle_desync(form_id: u32, duration: f32) -> (f32, f32) {
    // SplitMix64-style avalanche on the FormId → two independent 32-bit
    // fractions. Cheap, allocation-free, good bit diffusion so adjacent
    // FormIds (Bethesda hands them out sequentially within a plugin)
    // don't produce near-identical phases.
    let mut z = (form_id as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(0x1234_5678);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;

    let frac_phase = ((z & 0xFFFF_FFFF) as f32) / (u32::MAX as f32 + 1.0);
    let frac_speed = (((z >> 32) & 0xFFFF_FFFF) as f32) / (u32::MAX as f32 + 1.0);

    let start_time = if duration > 0.0 {
        // clamp guards f32 rounding pushing the product to exactly `duration`.
        (frac_phase * duration).min(duration * (1.0 - f32::EPSILON))
    } else {
        0.0
    };
    // Map [0,1) → [1 - jitter, 1 + jitter].
    let speed = 1.0 + (frac_speed * 2.0 - 1.0) * IDLE_SPEED_JITTER;
    (start_time, speed)
}

/// Deterministically pick one idle-clip handle from the shared per-cell
/// pool for an NPC, keyed by its stable FormId. `None` when the pool is
/// empty (Havok-animation game, or the KF wasn't archived). The choice
/// is stable across loads and balanced across a >1 pool; with today's
/// size-1 pool it always resolves the single generic idle. Uses a hash
/// multiplier distinct from [`idle_desync`]'s so clip choice and start
/// phase don't correlate.
fn pick_idle_handle(pool: &[u32], form_id: u32) -> Option<u32> {
    if pool.is_empty() {
        return None;
    }
    let h = (form_id as u64)
        .wrapping_mul(0xD6E8_FEB8_6659_FD93)
        .rotate_left(29);
    pool.get((h % pool.len() as u64) as usize).copied()
}

/// One armor piece resolved against the ESM index, queued for mesh
/// dispatch. The borrow into `EsmIndex` keeps this lifetime-tied to
/// the spawn-function scope.
struct ResolvedArmor<'a> {
    form_id: u32,
    /// Form referenced by OTFT/CNTO before leveled-list expansion. This is
    /// identical to `form_id` for a base ARMO and for the race skin layer.
    source_form_id: u32,
    model_path: &'a str,
    /// Biped slots that another equipped item displaced from this mesh.
    /// Non-zero only for the low-priority `RACE.WNAM` skin layer; the spawn
    /// hook uses it to remove matching dismember partitions while preserving
    /// still-uncovered body regions from the same NIF.
    hidden_biped_mask: u32,
    /// Inventory row this armor mesh resolves from. Cross-checked
    /// against `EquipmentSlots.occupants` after the equip loop
    /// finishes (#2094 / SKY-D3-NEW-02) — an entry whose index no
    /// longer occupies any of the biped bits it was equipped into
    /// was displaced by a later overlapping entry (multi-pick LVLI,
    /// mod CNTO overlapping a default OTFT slot) and must not spawn
    /// a mesh alongside the winner.
    inv_idx: InventoryIndex,
    /// The ARMO's own authored `BOD2`/`BODT` biped mask, as handed to
    /// `EquipmentSlots::equip`. **Zero is meaningful**: such a record
    /// claims no biped region at all, so it never enters `occupants` and
    /// the #2094 occupancy filter has no opinion about it — see the
    /// retain at the end of [`build_npc_equip_state`] and #3408.
    authored_biped_mask: u32,
}

/// Equip pipeline state built purely from `&NpcRecord` + `&EsmIndex`
/// — no World, no VulkanContext, no archive I/O. Both spawn paths
/// insert `inventory` + `equipment_slots` on the placement root
/// **before** skeleton / FaceGen load so the equip data lands even
/// when the spawn function early-returns on a missing archive. The
/// `armor_to_spawn` list is consumed after the skeleton resolves;
/// when the skeleton load early-returns, the meshes simply don't
/// spawn but the components are already in place for inspection +
/// the eventual save round-trip (M45).
struct NpcEquipState<'a> {
    inventory: Inventory,
    equipment_slots: EquipmentSlots,
    equipped_weapon: Option<EquippedWeapon>,
    armor_to_spawn: Vec<ResolvedArmor<'a>>,
    /// Biped bits the pre-baked FaceGen head must suppress, in the same
    /// `hide_skin_partitions` format as [`ResolvedArmor::hidden_biped_mask`].
    ///
    /// The FaceGeom NIF is a multi-region mesh source exactly like the race
    /// skin — vanilla Skyrim heads carry dismember partitions 130 (head +
    /// beard), 131 / 141 (hair), 143 (ears) and 132 (neck) — but it is never
    /// enrolled in `EquipmentSlots`, so nothing was ever computing its
    /// displacement. See #3409 and the fold that builds this at the end of
    /// [`build_npc_equip_state`].
    facegen_hidden_mask: u32,
}

impl NpcEquipState<'_> {
    fn covers_biped_mask(&self, mask: u32) -> bool {
        self.equipment_slots
            .occupants
            .iter()
            .enumerate()
            .any(|(bit, occupant)| mask & (1u32 << bit) != 0 && occupant.is_some())
    }

    /// Whether the winning equipment layer occupies this game's canonical
    /// main-body slot. Runtime-FaceGen games use this to suppress their loose
    /// naked upper-body mesh; prebaked-FaceGen games get the equivalent result
    /// by displacing the lower-priority `RACE.WNAM` skin armor.
    fn main_body_covered(&self, game: GameKind) -> bool {
        byroredux_plugin::equip::main_body_bit(game)
            .is_some_and(|bit| self.covers_biped_mask(1u32 << bit))
    }
}

/// Walk the NPC's default outfit + inventory, expand LVLI refs to
/// base ARMO records, populate `Inventory` + `EquipmentSlots`, and
/// collect the armor mesh paths the spawn-side mesh loader will
/// dispatch. Independent of World / VulkanContext so the caller can
/// insert the components ahead of any archive I/O — that way a
/// missing skeleton.nif (e.g. a modded path the resolver can't
/// match, or a future game whose humanoid-skeleton convention isn't
/// yet in `humanoid_skeleton_path`) still leaves the equip data
/// inspectable on the placement root.
fn build_npc_equip_state<'a>(
    npc: &NpcRecord,
    index: &'a EsmIndex,
    game: GameKind,
    gender: Gender,
) -> NpcEquipState<'a> {
    struct ExpandedEquip {
        form_id: u32,
        source_form_id: u32,
        count: u32,
    }

    let mut inventory = Inventory::new();
    let mut equipment_slots = EquipmentSlots::new();
    let mut equipped_weapon: Option<EquippedWeapon> = None;
    let mut armor_to_spawn: Vec<ResolvedArmor<'a>> = Vec::new();
    let mut race_skin_slots: Option<(InventoryIndex, u32)> = None;
    // #2955 — same gate as `stamp_character_components`: a PC-level-multiplier
    // record's `level` is not a level, and `expand_leveled_form_id` filters
    // `entry.level <= actor_level` then takes the highest eligible tier, so the
    // raw multiplier made every entry eligible and always drew the top one.
    let actor_level = effective_actor_level(npc);
    let mut expanded: Vec<ExpandedEquip> = Vec::new();
    let mut resolved_buf = Vec::new();

    // #2093 / SKY-D3-NEW-01 — race default skin (`RACE.WNAM`), equipped
    // FIRST so it's the lowest-priority layer: any OTFT/CNTO armor
    // resolved below that claims an overlapping biped bit displaces it
    // in `equipment_slots` (the #2094 post-loop filter then drops the
    // skin's mesh for exactly the bits it lost, keeping it for any bit
    // no other gear covers). Without this, an NPC whose OTFT/CNTO
    // doesn't cover a biped region has zero mesh source there — the
    // prebaked path's FaceGeom NIF is head-only (Bethesda FaceGen
    // convention), not "head and body in one mesh."
    if let Some(race) = index.races.get(&npc.race_form_id) {
        if let Some(skin_fid) = race.default_skin {
            let stack = ItemStack::new(skin_fid, 1);
            let inv_idx = inventory.push(stack);
            if let Some(item) = index.items.get(&skin_fid) {
                if let ItemKind::Armor { biped_flags, .. } = item.kind {
                    equipment_slots.equip(biped_flags, inv_idx);
                    race_skin_slots = Some((inv_idx, biped_flags));
                    // #3357 — the race skin is the multi-ARMA case: its
                    // BOD2 covers Head|Body|Hands|Feet and three separate
                    // addons (torso / hands / feet) serve any given race.
                    // One `ResolvedArmor` per mesh, all sharing `inv_idx`
                    // so the displacement mask and the #2094 retain treat
                    // them as one equipped item.
                    for model_path in byroredux_plugin::equip::resolve_armor_meshes(
                        item,
                        gender,
                        npc.race_form_id,
                        index,
                        game,
                    ) {
                        armor_to_spawn.push(ResolvedArmor {
                            form_id: skin_fid,
                            source_form_id: skin_fid,
                            model_path,
                            hidden_biped_mask: 0,
                            inv_idx,
                            authored_biped_mask: biped_flags,
                        });
                    }
                }
            }
        }
    }

    // Default outfit (OTFT.items) → expand each entry through the
    // LVLI dispatcher. Skyrim+ NPCs typically reference leveled
    // lists for outfit variety; the pre-fix loop skipped LVLI refs
    // silently. See M41 Phase 2 close-out / #896.
    if let Some(otft_fid) = npc.default_outfit {
        if let Some(otft) = index.outfits.get(&otft_fid) {
            for &fid in &otft.items {
                resolved_buf.clear();
                byroredux_plugin::equip::expand_leveled_form_id(
                    fid,
                    actor_level,
                    index,
                    &mut resolved_buf,
                );
                expanded.extend(resolved_buf.iter().copied().map(|form_id| ExpandedEquip {
                    form_id,
                    source_form_id: fid,
                    count: 1,
                }));
            }
        }
    }

    // CNTO inventory entries, resolved through the TPLT chain. #1658 —
    // route through the same game-agnostic `resolve_inherited_inventory`
    // helper the kf-era path uses (`:498`): it returns the NPC's own
    // inventory when no template applies, or walks `template_form_id`
    // (NPC_ or LVLN) when `template_flags & TEMPLATE_FLAG_USE_INVENTORY`
    // is set. Without it, templated Skyrim NPCs with an empty own CNTO
    // (leveled actors that inherit gear via TPLT) spawned naked. Negative
    // counts are remove-from-inventory deltas; clamp at runtime.
    for entry in byroredux_plugin::equip::resolve_inherited_inventory(npc, actor_level, index) {
        let count = entry.count.max(0) as u32;
        if count == 0 {
            continue;
        }
        resolved_buf.clear();
        byroredux_plugin::equip::expand_leveled_form_id(
            entry.item_form_id,
            actor_level,
            index,
            &mut resolved_buf,
        );
        expanded.extend(resolved_buf.iter().copied().map(|form_id| ExpandedEquip {
            form_id,
            source_form_id: entry.item_form_id,
            count,
        }));
    }

    for expanded in expanded {
        let form_id = expanded.form_id;
        let stack = ItemStack::new(form_id, expanded.count);
        let inv_idx = inventory.push(stack);

        let Some(item) = index.items.get(&form_id) else {
            // LVLI dispatcher already flattened to base records;
            // anything still unresolved here is a master / DLC
            // master-list miss. Silent — the inventory row stays.
            continue;
        };
        let biped_flags = match &item.kind {
            ItemKind::Weapon {
                damage,
                reach,
                speed,
                ..
            } => {
                let candidate = EquippedWeapon {
                    inventory_index: inv_idx,
                    base_form_id: form_id,
                    damage: *damage as f32,
                    reach: *reach,
                    speed: *speed,
                };
                let replace = equipped_weapon.is_none_or(|current| {
                    candidate.damage > current.damage
                        || (candidate.damage == current.damage
                            && candidate.base_form_id < current.base_form_id)
                });
                if replace {
                    equipped_weapon = Some(candidate);
                }
                continue;
            }
            ItemKind::Armor { biped_flags, .. } => *biped_flags,
            // Food, ammo, MISC, and other non-equipment inventory keeps its
            // row but has no live equip state in this slice.
            _ => continue,
        };

        equipment_slots.equip(biped_flags, inv_idx);

        // #3357 — 166 of 2,762 Skyrim ARMOs serve one race with more than
        // one ARMA; each contributes its own mesh.
        for model_path in byroredux_plugin::equip::resolve_armor_meshes(
            item,
            gender,
            npc.race_form_id,
            index,
            game,
        ) {
            armor_to_spawn.push(ResolvedArmor {
                form_id,
                source_form_id: expanded.source_form_id,
                model_path,
                hidden_biped_mask: 0,
                inv_idx,
                authored_biped_mask: biped_flags,
            });
        }
    }

    // A race skin can cover several regions in one mesh. If later armor wins
    // only some of those bits, keep the skin mesh but tell the importer which
    // dismember partitions to suppress. Bits outside the skin ARMO's authored
    // mask are irrelevant even if some other item occupies them.
    if let Some((skin_inv_idx, skin_biped_flags)) = race_skin_slots {
        let displaced_mask =
            equipment_slots
                .occupants
                .iter()
                .enumerate()
                .fold(0u32, |mask, (bit, occupant)| {
                    let bit_mask = 1u32 << bit;
                    if skin_biped_flags & bit_mask != 0
                        && occupant.is_some()
                        && *occupant != Some(skin_inv_idx)
                    {
                        mask | bit_mask
                    } else {
                        mask
                    }
                });
        // #3357 — `filter`, not `find`: the skin now contributes one
        // `ResolvedArmor` per ARMA mesh (torso / hands / feet), and every
        // one of them needs the displacement mask. With `find`, only the
        // first got it and the rest rendered through gear that should
        // have hidden them.
        for skin in armor_to_spawn
            .iter_mut()
            .filter(|armor| armor.inv_idx == skin_inv_idx)
        {
            skin.hidden_biped_mask = displaced_mask;
        }
    }

    // #2094 / SKY-D3-NEW-02 — drop any queued mesh whose inventory
    // index no longer occupies a biped bit. `equip()` above already
    // resolved slot-overlap precedence (later entry in the expanded
    // list wins any bit it shares with an earlier one, e.g. two
    // multi-pick LVLI entries or a mod CNTO overlapping a default
    // OTFT slot) — this pass makes the mesh set agree with that
    // resolution instead of spawning every candidate regardless of
    // whether it was displaced.
    //
    // #3408 / SKY-2026-08-27b-D3-01 — a record whose authored mask is ZERO is
    // exempt. `equip()` iterates the set bits of the mask, so a zero mask
    // sets none, so such an item can never appear in `occupants` and can
    // never satisfy the retain — its mesh was discarded unconditionally. That
    // is not a displacement; a skin that claims no biped region cannot be
    // displaced out of one, so this filter has no opinion about it.
    //
    // Measured on real `Skyrim.esm`: 10 of 2,762 ARMOs author `BOD2 == 0` and
    // every one of them names ARMAs — `SkinDraugr`, `SkinSabrecat`,
    // `SkinSkeever`, `SkinFrostbiteSpider(Cold)`, `SkinSlaughterfish`, plus
    // the Draugr hair/beard parts. 7 of 99 races point `WNAM` at one, and
    // 351 of 5,118 NPC_ records sit on those races (314 of them Draugr).
    // Every one lost its body mesh here; 170 ended with no mesh source at all.
    armor_to_spawn.retain(|armor| {
        armor.authored_biped_mask == 0 || equipment_slots.occupants.contains(&Some(armor.inv_idx))
    });

    // #3409 / SKY-2026-08-27b-D3-02 — the pre-baked FaceGen head's own
    // displacement mask. The head is a multi-region mesh source (partitions
    // 130 head+beard, 131/141 hair, 143 ears, 132 neck) but is never enrolled
    // in `EquipmentSlots`, so every bit an *equipped item* holds is a bit
    // something else covers — the same question the `displaced_mask` fold
    // above answers for the race skin, with the whole biped range in scope
    // instead of the skin's authored mask.
    //
    // Excluding the race skin's own index is load-bearing, not tidiness:
    // `SkinNaked` authors bit 0 (Head), and 47 of Skyrim's 99 races point
    // `WNAM` at a skin that does. Folding those in would hide partition 130
    // on every one of them — i.e. delete the face of most humanoid NPCs.
    // With the exclusion, bit 0 stays with the skin until an armour actually
    // displaces it, which is exactly what a closed helm does:
    //
    //   Dwarven / Daedric / Nord Plate / Guard "FullReach"  bits 0,1,12,13
    //     → hides 130 (face + beard), 131 (hair), 143 (ears); the helm ships
    //       its own partition-30 geometry (Dwarven: 1514 triangles) to
    //       replace it. 175 of Skyrim's 2,762 ARMOs are authored this way.
    //   Iron / Hide / Studded / Steel light helms          bits 1,12
    //     → hides 131 only; the face survives, which is the visible
    //       difference between an open and a closed helm.
    //   Circlets                                            bit 12
    //     → hides nothing on the head; a circlet displaces other circlets.
    //
    // Known residual: partition 141 (long hair) maps to bit 11 (slot 41),
    // which exactly ONE of Skyrim's 2,762 ARMOs claims, so long hair is never
    // displaced by this rule. Coupling 141 to bit 1 would be inventing a
    // mapping the data doesn't author — and it would be wrong for the
    // deliberate `HairLine*` sub-meshes, which Bethesda authors to show
    // *because* a helmet is worn. Left for a HDPT-type-aware follow-up.
    let skin_inv_idx = race_skin_slots.map(|(idx, _)| idx);
    let facegen_hidden_mask =
        equipment_slots
            .occupants
            .iter()
            .enumerate()
            .fold(0u32, |mask, (bit, occupant)| match occupant {
                Some(idx) if Some(*idx) != skin_inv_idx => mask | (1u32 << bit),
                _ => mask,
            });

    NpcEquipState {
        inventory,
        equipment_slots,
        equipped_weapon,
        armor_to_spawn,
        facegen_hidden_mask,
    }
}

/// Path inside the meshes archive for an NPC's pre-baked FaceGen
/// NIF on Skyrim / FO4 / FO76 / Starfield. Returns `None` for
/// kf-era games (those use the runtime-FaceGen recipe path).
///
/// Vanilla SSE convention (verified by BSA scan 2026-04-28 — 3 158
/// pre-baked NIFs in `Skyrim - Meshes0.bsa`, 1:1 match with face-
/// tint DDS in `Skyrim - Textures0.bsa`):
///
/// ```text
/// meshes\actors\character\facegendata\facegeom\<plugin>\<formid:08x>.nif
/// ```
///
/// The `<plugin>` segment is the lowercase basename including the
/// `.esm` / `.esp` extension. The `<formid:08x>` is the NPC's
/// load-order-global FormID rendered as 8 lowercase hex digits.
pub fn prebaked_facegen_nif_path(plugin_name: &str, form_id: u32) -> Option<String> {
    if plugin_name.is_empty() {
        return None;
    }
    Some(format!(
        r"meshes\actors\character\facegendata\facegeom\{}\{:08x}.nif",
        plugin_name.to_ascii_lowercase(),
        form_id,
    ))
}

/// Companion path to [`prebaked_facegen_nif_path`] for the per-NPC
/// face-tint DDS. Same plugin / FormID structure under
/// `textures\actors\character\facegendata\facetint\` instead of
/// `meshes\...\facegeom\`. Returns `None` on empty plugin.
pub fn prebaked_facegen_tint_path(plugin_name: &str, form_id: u32) -> Option<String> {
    if plugin_name.is_empty() {
        return None;
    }
    Some(format!(
        r"textures\actors\character\facegendata\facetint\{}\{:08x}.dds",
        plugin_name.to_ascii_lowercase(),
        form_id,
    ))
}

/// Walk the subtree rooted at `root` and tag every descendant entity
/// carrying a [`MeshHandle`] with [`RenderLayer::Actor`]. Loose-NIF
/// spawns at `scene::load_nif_bytes` default each mesh entity to
/// `RenderLayer::Architecture` (no REFR base record available), so
/// every NPC body / head / armor / FaceGen mesh comes out of that path
/// with the wrong layer for depth-bias purposes — without this
/// override every standing NPC z-fights the floor at the foot-plant
/// patch. Called from each success path of [`NpcSpawnJob::advance`]
/// (`npc_spawn/resumable.rs`) before yielding a completed placement
/// root, for both the kf-era runtime recipe and the pre-baked-FaceGen
/// recipe. BFS over `Children`, mirrors
/// [`crate::anim_convert::build_subtree_name_map`]'s walk shape.
pub(crate) fn tag_descendants_as_actor(world: &mut World, root: EntityId) {
    use byroredux_core::ecs::components::RenderLayer;
    use byroredux_core::ecs::{Children, MeshHandle};

    // Collect first (read locks), mutate after (write locks). The
    // ECS API forbids holding read + write guards simultaneously.
    let mut to_tag: Vec<EntityId> = Vec::new();
    {
        let children_q = world.query::<Children>();
        let mesh_q = world.query::<MeshHandle>();
        let mut queue = vec![root];
        while let Some(e) = queue.pop() {
            if let Some(ref mq) = mesh_q {
                if mq.get(e).is_some() {
                    to_tag.push(e);
                }
            }
            if let Some(ref cq) = children_q {
                if let Some(children) = cq.get(e) {
                    for &c in &children.0 {
                        queue.push(c);
                    }
                }
            }
        }
    }
    for e in to_tag {
        world.insert(e, RenderLayer::Actor);
    }
}

#[cfg(test)]
mod tests;
