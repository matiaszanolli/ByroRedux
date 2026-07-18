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
    EquipmentSlots, FactionRanks, GlobalTransform, Inventory, InventoryIndex, ItemStack,
    MotionType, Name, Parent, RigidBodyData, Transform,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_plugin::equip::armor_covers_main_body;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{EsmIndex, ItemKind, NpcRecord, RaceRecord};
use byroredux_renderer::VulkanContext;

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
/// from its class's base SPECIAL via the documented FNV/FO3 auto-calc model,
/// so the M47.1 `GetActorValue` condition reads real values (#1663). No-op
/// when the derivation yields nothing — a non-FNV/FO3 game, an NPC with no
/// (parsed) class, or an index whose `AVIF` records don't resolve.
fn stamp_actor_values(
    world: &mut World,
    placement_root: EntityId,
    npc: &NpcRecord,
    index: &EsmIndex,
    game: GameKind,
) {
    let pairs = byroredux_plugin::esm::records::derive_npc_actor_values(npc, index, game);
    if pairs.is_empty() {
        return;
    }
    world.insert(
        placement_root,
        byroredux_core::ecs::components::ActorValues::from_pairs(pairs),
    );
}

/// Stamp the CHARAL structural components — [`CharacterLevel`],
/// [`Background`], and [`Perks`] — on the NPC's placement root. Level +
/// race/class come from every game's `NPC_`; perks from FO4+ `PRKR`.
/// Complements [`stamp_actor_values`] (the numeric substrate): together they
/// land the full canonical character state an actor carries at spawn.
fn stamp_character_components(world: &mut World, placement_root: EntityId, npc: &NpcRecord) {
    use byroredux_core::character::{Background, CharacterLevel, PerkRank, Perks};
    // Level: the NPC's base level (clamped non-negative). NPCs carry no XP.
    world.insert(
        placement_root,
        CharacterLevel {
            level: npc.level.max(0) as u16,
            xp: 0,
        },
    );
    // Provenance: race + class (0 = none), reused by runtime leveling.
    world.insert(
        placement_root,
        Background {
            race_form_id: npc.race_form_id,
            class_form_id: npc.class_form_id,
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
/// from the parsed AVIF set — resolving each derived formula's input/output
/// EditorIDs through `index`. `None` for games CHARAL doesn't model yet
/// (Oblivion / Skyrim / FO76 / Starfield).
///
/// `GameKind::Fallout3NV` covers **both** FO3 and FNV; the *actor-general*
/// derived stats (Carry Weight / Melee Damage / Crit Chance / Unarmed Damage —
/// the only ones a non-player consumer computes) are identical between them,
/// so it resolves to the FNV ruleset. The FO3↔FNV-divergent *player* Health/AP
/// would need master-name disambiguation — deferred with the player actor.
pub fn build_character_ruleset(
    game: GameKind,
    index: &EsmIndex,
) -> Option<byroredux_core::character::CharacterRuleset> {
    let resolve = |editor_id: &str| index.actor_value_form_id(editor_id);
    Some(match game {
        GameKind::Fallout4 => byroredux_core::character::fallout4_ruleset(resolve),
        GameKind::Fallout3NV => byroredux_core::character::falloutnv_ruleset(resolve),
        _ => return None,
    })
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
fn keyframe_live_ragdoll_bones(
    world: &mut World,
    skel_map: &std::collections::HashMap<std::sync::Arc<str>, EntityId>,
) {
    for &bone in skel_map.values() {
        if let Some(body) = world.get_mut::<RigidBodyData>(bone) {
            if body.motion_type == MotionType::Dynamic {
                body.motion_type = MotionType::Keyframed;
            }
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

/// Hardcoded vanilla body NIF paths.
///
/// On FNV / FO3 the RACE record's `MODL` fields carry **head** mesh
/// paths (e.g. `Characters\Head\HeadHuman.NIF`), not body — the body
/// ships at canonical paths per gender that every humanoid race shares.
/// FNV's `Fallout - Meshes.bsa` ships hands as separate NIFs alongside
/// the upperbody, so a single path is not enough to fully cover a kf-
/// era humanoid (#793 — pre-fix every NPC rendered without hands).
///
/// Returns `&[]` for game variants on the pre-baked-FaceGen track —
/// SSE / FO4 / FO76 / Starfield don't ship a separate skinned body
/// NIF; the per-NPC `facegendata\facegeom\<plugin>\<formid:08x>.nif`
/// carries head + body in one mesh. That spawn path lands in Phase 4.
///
/// Female humanoids on FNV vanilla re-use the male body (verified
/// 2026-04-28 — `_female\` directory not present in vanilla
/// Fallout - Meshes.bsa). Mods may add a separate female set; the
/// gender split can be re-introduced on the signature at that point.
/// See TD8-018 / #1117 for the placeholder-arg removal rationale.
pub fn humanoid_body_paths(game: GameKind) -> &'static [&'static str] {
    match game {
        // Oblivion's mesh layout uses the same `_male\` directory shape
        // as FO3 / FNV; if Oblivion ships hands at different paths the
        // load will silently miss (debug-logged) like any other modded
        // path. Verification deferred per #793 issue body.
        GameKind::Oblivion | GameKind::Fallout3NV => &[
            r"meshes\characters\_male\upperbody.nif",
            r"meshes\characters\_male\lefthand.nif",
            r"meshes\characters\_male\righthand.nif",
        ],
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield => &[],
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
/// and threads the result through each [`spawn_npc_entity`] call so
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
        GameKind::Fallout3NV => {
            Some(r"meshes\characters\_male\idleanims\chairskirt_leftenter.kf")
        }
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
    let mut z = (form_id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0x1234_5678);
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
    model_path: &'a str,
    /// Inventory row this armor mesh resolves from. Cross-checked
    /// against `EquipmentSlots.occupants` after the equip loop
    /// finishes (#2094 / SKY-D3-NEW-02) — an entry whose index no
    /// longer occupies any of the biped bits it was equipped into
    /// was displaced by a later overlapping entry (multi-pick LVLI,
    /// mod CNTO overlapping a default OTFT slot) and must not spawn
    /// a mesh alongside the winner.
    inv_idx: InventoryIndex,
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
    armor_to_spawn: Vec<ResolvedArmor<'a>>,
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
    let mut inventory = Inventory::new();
    let mut equipment_slots = EquipmentSlots::new();
    let mut armor_to_spawn: Vec<ResolvedArmor<'a>> = Vec::new();
    let actor_level = npc.level;
    let mut expanded: Vec<u32> = Vec::new();

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
                    if let Some(model_path) = byroredux_plugin::equip::resolve_armor_mesh(
                        item,
                        gender,
                        npc.race_form_id,
                        index,
                        game,
                    ) {
                        armor_to_spawn.push(ResolvedArmor {
                            form_id: skin_fid,
                            model_path,
                            inv_idx,
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
                byroredux_plugin::equip::expand_leveled_form_id(
                    fid,
                    actor_level,
                    index,
                    &mut expanded,
                );
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
        if entry.count.max(0) > 0 {
            byroredux_plugin::equip::expand_leveled_form_id(
                entry.item_form_id,
                actor_level,
                index,
                &mut expanded,
            );
        }
    }

    for form_id in expanded {
        let stack = ItemStack::new(form_id, 1);
        let inv_idx = inventory.push(stack);

        let Some(item) = index.items.get(&form_id) else {
            // LVLI dispatcher already flattened to base records;
            // anything still unresolved here is a master / DLC
            // master-list miss. Silent — the inventory row stays.
            continue;
        };
        let ItemKind::Armor { biped_flags, .. } = item.kind else {
            // Non-armor inventory (food, ammo, weapons, MISC) keep
            // their inventory row but don't equip / spawn mesh.
            continue;
        };

        equipment_slots.equip(biped_flags, inv_idx);

        if let Some(model_path) =
            byroredux_plugin::equip::resolve_armor_mesh(item, gender, npc.race_form_id, index, game)
        {
            armor_to_spawn.push(ResolvedArmor {
                form_id,
                model_path,
                inv_idx,
            });
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
    armor_to_spawn.retain(|armor| {
        equipment_slots
            .occupants
            .iter()
            .any(|occupant| *occupant == Some(armor.inv_idx))
    });

    NpcEquipState {
        inventory,
        equipment_slots,
        armor_to_spawn,
    }
}

/// Spawn an NPC actor entity for the kf-era path (Oblivion / FO3 /
/// FNV) — M41.0 Phase 1b. Returns the placement-root `EntityId` for
/// the assembled actor (skeleton + body, parented under the root and
/// `CellRoot`-stamped). Returns `None` when the game is on the
/// pre-baked-FaceGen track (Skyrim / FO4 / FO76 / Starfield) — that
/// dispatch lands in Phase 4.
///
/// **Phase 1b scope**: skeleton + first race body model. The head is
/// deferred to Phase 1c / Phase 3 because resolving the per-NPC head
/// mesh requires either an HDPT lookup (per-NPC PNAM override, FO4+
/// semantic) or the race base head (Phase 3 morph evaluator
/// territory). NPCs spawned by this phase render as a headless body
/// in bind pose — visual confirmation that the spawn dispatcher,
/// skeleton hierarchy, shared `node_by_name` map, and body skinning
/// resolution all wire correctly.
///
/// `CellRoot` ownership is stamped post-load by
/// `cell_loader::stamp_cell_root`, which walks the entity-id range
/// from `first_entity` (captured before the load) to `next_entity_id`
/// (after) and inserts `CellRoot` on every fresh entity. So
/// `spawn_npc_entity` doesn't need to thread the cell-root id —
/// every entity it creates lands inside that range.
#[allow(clippy::too_many_arguments)]
pub fn spawn_npc_entity(
    world: &mut World,
    ctx: &mut VulkanContext,
    npc: &NpcRecord,
    race: Option<&RaceRecord>,
    game: GameKind,
    tex_provider: &TextureProvider,
    mut mat_provider: Option<&mut MaterialProvider>,
    idle_pool: &[u32],
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    index: &EsmIndex,
) -> Option<EntityId> {
    // Phase 1b only handles the kf-era path. The pre-baked-FaceGen
    // dispatch (Skyrim / FO4 / FO76 / Starfield) lands in Phase 4.
    if !game.has_runtime_facegen_recipe() {
        return None;
    }

    let gender = Gender::from_acbs_flags(npc.acbs_flags);

    // 1. Placement root — owns the world-space pose. Body / head
    //    parent under this so the transform-propagation system
    //    composes the NIF-local placements onto them each frame.
    let placement_root = world.spawn();
    world.insert(placement_root, Transform::new(ref_pos, ref_rot, ref_scale));
    world.insert(
        placement_root,
        GlobalTransform::new(ref_pos, ref_rot, ref_scale),
    );
    log::info!(
        "NPC {:08X} ({}) spawning at world [{:.0},{:.0},{:.0}] scale={:.2}",
        npc.form_id,
        npc.editor_id,
        ref_pos.x,
        ref_pos.y,
        ref_pos.z,
        ref_scale,
    );
    if !npc.editor_id.is_empty() {
        let mut pool = world.resource_mut::<StringPool>();
        let sym = pool.intern(&npc.editor_id);
        drop(pool);
        world.insert(placement_root, Name(sym));
    }
    stamp_faction_ranks(world, placement_root, npc);
    stamp_actor_values(world, placement_root, npc, index, game);
    stamp_character_components(world, placement_root, npc);

    // 2. Skeleton. Owns the per-bone entities the body / head will
    //    skin against.
    let skel_path = humanoid_skeleton_path(game)?;
    let skel_data = match tex_provider.extract_mesh(skel_path) {
        Some(d) => d,
        None => {
            log::warn!(
                "NPC {:08X} ({}): skeleton '{}' not found in archives — skipping spawn",
                npc.form_id,
                npc.editor_id,
                skel_path,
            );
            return Some(placement_root);
        }
    };
    let (_skel_count, skel_root, skel_map) = load_nif_bytes_with_skeleton(
        world,
        ctx,
        &skel_data,
        skel_path,
        tex_provider,
        mat_provider.as_deref_mut(),
        None,
        None,
        None,
    );
    // #1698 — keyframe the ragdoll bones so they don't free-simulate (and pin
    // the physics step) while the actor is alive. Must run before the first
    // physics_sync registers them.
    keyframe_live_ragdoll_bones(world, &skel_map);
    if let Some(sr) = skel_root {
        world.insert(sr, Parent(placement_root));
        add_child(world, placement_root, sr);
    } else {
        // Skeleton parsed but produced no root — cosmetic edge case;
        // skinning will fail to resolve below but the placement root
        // still anchors the actor for telemetry.
        log::debug!(
            "NPC {:08X}: skeleton '{}' produced no root entity",
            npc.form_id,
            skel_path,
        );
    }

    // Phase A.2 — pre-scan inventory for "armor covers main body."
    // The base `upperbody.nif` ships with the vanilla T-shirt-and-
    // briefs body texture; vanilla armors (button-up + slacks for Doc
    // Mitchell, NCR Trooper armor, etc.) include the actor's exposed
    // body parts inline rather than overlaying the base body. Loading
    // both produces z-fight on the overlapping vertices AND a 2×
    // skinned bone palette load. Skipping the base body when any
    // equipped armor occupies the game's main-body biped slot is the
    // canonical shape — verified against xEdit `dev-4.1.6` definitions.
    //
    // Hands stay loaded regardless — vanilla FNV armors typically
    // include arms but not the finger geometry from `lefthand.nif` /
    // `righthand.nif`, which the gun-grip animation poses against.
    //
    // FNV `Lvl*` template NPCs (PowderGangers, NCRTroopers, etc.)
    // author empty CNTO on themselves and inherit via TPLT —
    // `resolve_inherited_inventory` walks the chain when
    // `template_flags & 0x100` is set. Without this the body-skip
    // check below trivially fires "no armor" and every Lvl* NPC
    // spawns visibly naked.
    let effective_inventory =
        byroredux_plugin::equip::resolve_inherited_inventory(npc, npc.level, index);
    // `body_covered` must walk LVLI just like the equip dispatch below
    // — vanilla FNV settlers / civilians carry their outfit as an LVLI
    // (e.g. `WastelandSettlerOutfit` resolves to `OutfitRepublican04`
    // at level 1) rather than a direct ARMO reference. Pre-fix the
    // pre-scan saw the LVLI form ID in `items.get()`, missed, and
    // returned false → `upperbody.nif` loaded under the equipped
    // outfit, producing the visible "T-shirt + briefs through the
    // outfit" look on every CNTO-via-LVLI NPC.
    let mut body_covered_buf: Vec<u32> = Vec::new();
    let body_covered = effective_inventory.iter().any(|entry| {
        if entry.count.max(0) == 0 {
            return false;
        }
        body_covered_buf.clear();
        byroredux_plugin::equip::expand_leveled_form_id(
            entry.item_form_id,
            npc.level,
            index,
            &mut body_covered_buf,
        );
        body_covered_buf.iter().any(|&fid| {
            let Some(item) = index.items.get(&fid) else {
                return false;
            };
            let ItemKind::Armor { biped_flags, .. } = item.kind else {
                return false;
            };
            armor_covers_main_body(game, biped_flags)
        })
    });

    // 3. Body. Hardcoded vanilla paths (`upperbody.nif` + per-hand
    //    NIFs); the RACE record's MODL fields are head models on FNV /
    //    FO3, not body. Each missing NIF is skipped silently — modded
    //    setups may have replaced or removed individual paths, in
    //    which case the NPC still gets a skeleton + head + whatever
    //    of the body was actually loadable. Pre-#793 only `upperbody`
    //    shipped, so every kf-era NPC rendered handless.
    for body_path in humanoid_body_paths(game) {
        if body_covered && body_path.ends_with("upperbody.nif") {
            log::info!(
                "NPC {:08X} ({}): equipped armor covers torso — skipping {}",
                npc.form_id,
                npc.editor_id,
                body_path,
            );
            continue;
        }
        match tex_provider.extract_mesh(body_path) {
            Some(body_data) => {
                // M41.0 Phase 1b.x — body skinning catastrophically
                // misrenders interactively (long-spike vertex
                // artifact, see audit screenshots
                // `/tmp/audit/m41/qa_doc_mitchell_2026-04-29.png`).
                // The artifact is independent of `external_skeleton`
                // (verified empirically: same artifact with both
                // `Some(&skel_map)` and `None`), and `0 unresolved`
                // bones are reported per skinned sub-mesh, so the
                // bug is in the runtime entity transform / palette
                // composition, not the bone-name resolution.
                //
                // M29 standalone tests pass on the same upperbody.nif
                // because they don't go through the cell-loader's
                // placement_root parent chain — when the body NIF is
                // spawned in isolation at world-origin, the math
                // works. The cell-load path adds a `Parent` edge
                // from body_root to placement_root, and *something*
                // about that composition makes the bone palette
                // produce non-canonical matrices.
                //
                // Filed as Phase 1b.x with a concrete diagnostic
                // plan: dump the skinned-mesh's bones'
                // GlobalTransforms + bind_inverses at runtime,
                // compute the palette by hand, compare against
                // skinning_e2e's working palette to find the
                // diverging entity. Out of tonight's scope. The
                // `Some(&skel_map)` path stays — it's the
                // architecturally correct target once the
                // composition bug is fixed (single skeleton drives
                // all skinned meshes).
                let (_body_count, body_root, _body_map) = load_nif_bytes_with_skeleton(
                    world,
                    ctx,
                    &body_data,
                    body_path,
                    tex_provider,
                    mat_provider.as_deref_mut(),
                    Some(&skel_map),
                    None,
                    None,
                );
                if let Some(br) = body_root {
                    // Parent body under placement_root, NOT under
                    // skeleton root. Body NIFs ship their own
                    // skeleton-shaped NiNode hierarchy (cosmetic
                    // copies of "Bip01 Pelvis" etc.); leaving them
                    // as descendants of skel_root pollutes the
                    // animation system's BFS-from-skel_root subtree
                    // name map (last-write-wins puts the body's
                    // *local* `Bip01 Spine` in the slot, so KF
                    // channels write to body's orphan copy AND
                    // anything sharing those names — visible
                    // regression: NPCs vanished post-Phase-2
                    // when AnimationPlayer ran). Parenting to
                    // placement_root instead keeps the animation
                    // BFS strictly inside the skeleton's own
                    // subtree. Skinning math is unaffected because
                    // SkinnedMesh.bones already references the
                    // skeleton's entities by ID (resolved through
                    // `external_skeleton` at scene-import time).
                    world.insert(br, Parent(placement_root));
                    add_child(world, placement_root, br);
                }
            }
            None => {
                log::debug!(
                    "NPC {:08X} ({}): body '{}' not in archives — skipping body mesh",
                    npc.form_id,
                    npc.editor_id,
                    body_path,
                );
            }
        }
    }

    // 4. Head. RACE.body_models[0] is the per-race head NIF on FNV /
    //    FO3 (the path the FaceGen `.egm` morph evaluator will deform
    //    in Phase 3b). Authored relative to `meshes\`, so the path
    //    normalises before extraction. If the race resolution fails
    //    or the path isn't archived, NPC still gets a headless body.
    let head_path = race.and_then(|r| r.body_models.first().map(|s| s.as_str()));
    if let Some(raw_head_path) = head_path {
        let head_path = normalize_mesh_path(raw_head_path);
        match tex_provider.extract_mesh(head_path.as_ref()) {
            Some(head_data) => {
                // M41.0 Phase 3b — load and apply per-NPC FaceGen
                // FGGS sym morphs to the race base head. The EGM
                // sidecar lives alongside the head NIF (same dir,
                // `.egm` extension); we load its bytes once here so
                // the closure passed to `pre_spawn_hook` can borrow
                // a parsed `EgmFile` for the duration of the import.
                // Asym morphs (FGGA) and the FGTS texture-tint pass
                // ship in Phase 3c.
                let recipe = npc.runtime_facegen.as_ref();
                let egm_bytes = recipe
                    .and_then(|_| facegen_sidecar_path(head_path.as_ref(), "egm"))
                    .and_then(|p| tex_provider.extract_mesh(&p));
                let egm_file =
                    egm_bytes
                        .as_ref()
                        .and_then(|b| match byroredux_facegen::EgmFile::parse(b) {
                            Ok(e) => Some(e),
                            Err(err) => {
                                log::debug!(
                                    "NPC {:08X}: EGM parse failed for head '{}': {}",
                                    npc.form_id,
                                    head_path,
                                    err,
                                );
                                None
                            }
                        });
                let hook_state: Option<(&byroredux_facegen::EgmFile, [f32; 50], [f32; 30], u32)> =
                    match (recipe, egm_file.as_ref()) {
                        (Some(r), Some(egm)) => Some((egm, r.fggs, r.fgga, npc.form_id)),
                        _ => None,
                    };
                let has_hook = hook_state.is_some();
                let mut hook_state = hook_state;
                let mut hook = |scene: &mut byroredux_nif::import::ImportedScene| {
                    let Some((egm, fggs, fgga, form_id)) = hook_state.take() else {
                        return;
                    };
                    // FNV race-base head NIFs (e.g. headhuman.nif:
                    // 1211 verts) and their EGM sidecars (1449
                    // verts) deliberately disagree on vertex count
                    // — the EGM carries 238 extra entries that map
                    // to UV-shell duplicates the NIF unifies. The
                    // `.tri` file's remap table is the canonical
                    // bridge between the two; until Phase 3b.x lands
                    // that table the evaluator applies the EGM's
                    // first `mesh.positions.len()` deltas
                    // best-effort. Result: continuous-mesh interior
                    // verts deform per slider; UV-seam vertices may
                    // be slightly under-deformed (only a fraction
                    // sit on shell-duplicate edges, and the practical
                    // effect is < 1 mm jitter at the seam — visible
                    // in close-up but invisible at gameplay distance).
                    let mut deformed_meshes = 0usize;
                    for mesh in scene.meshes.iter_mut() {
                        if mesh.positions.is_empty() {
                            continue;
                        }
                        // M41.0 Phase 3b — symmetric (FGGS) deltas
                        // first; M41.0 Phase 3c — asymmetric (FGGA)
                        // deltas summed on top. Both passes use the
                        // same linear evaluator; the asym pass just
                        // targets the second morph table on disk
                        // (`fgga_morphs`) with the second slider
                        // array. Per-NPC effect: FGGS shapes the
                        // bilateral features (jaw, nose bridge, eye
                        // height) and FGGA shapes asymmetric ones
                        // (cheek skew, eyebrow tilt) — together they
                        // produce the per-NPC face Bethesda's
                        // FaceGen tool authored.
                        let after_sym = byroredux_facegen::apply_morphs(
                            &mesh.positions,
                            &egm.fggs_morphs,
                            &fggs,
                        );
                        let after_asym =
                            byroredux_facegen::apply_morphs(&after_sym, &egm.fgga_morphs, &fgga);
                        mesh.positions = after_asym;
                        deformed_meshes += 1;
                    }
                    log::debug!(
                        "M41.0 Phase 3b/3c: NPC {:08X} applied FGGS+FGGA morphs to {} head mesh(es) \
                         (EGM {} verts × {} sym + {} asym; \
                         best-effort prefix until Phase 3b.x parses .tri remap)",
                        form_id,
                        deformed_meshes,
                        egm.num_vertices,
                        egm.fggs_morphs.len(),
                        egm.fgga_morphs.len(),
                    );
                };
                let pre_spawn: Option<&mut dyn FnMut(&mut byroredux_nif::import::ImportedScene)> =
                    if has_hook { Some(&mut hook) } else { None };
                // Same Phase 1b.x note as the body load — head
                // skinning happens to render reasonably because the
                // head only has 5 bones in a short chain, so whatever
                // the runtime-composition bug is, it produces a
                // recognizable face shape rather than a spike.
                // External skeleton stays — it's the right target.
                let (_head_count, head_root, _head_map) = load_nif_bytes_with_skeleton(
                    world,
                    ctx,
                    &head_data,
                    head_path.as_ref(),
                    tex_provider,
                    mat_provider.as_deref_mut(),
                    Some(&skel_map),
                    None,
                    pre_spawn,
                );
                if let Some(hr) = head_root {
                    // Same reasoning as body: head NIF carries its
                    // own local skeleton-shaped hierarchy (head bones
                    // like "Bip01 Head", "Bip01 L Eye"); parenting
                    // under placement_root keeps it out of the
                    // animation BFS's path so KF channels resolve to
                    // the skeleton's entities, not the head's
                    // orphans.
                    world.insert(hr, Parent(placement_root));
                    add_child(world, placement_root, hr);
                }
            }
            None => {
                log::debug!(
                    "NPC {:08X} ({}): head '{}' not in archives — skipping head mesh",
                    npc.form_id,
                    npc.editor_id,
                    head_path,
                );
            }
        }
    } else {
        log::debug!(
            "NPC {:08X} ({}): race {:08X} has no head MODL — skipping head mesh",
            npc.form_id,
            npc.editor_id,
            npc.race_form_id,
        );
    }

    // Per-NPC FaceGen morphs (FGGS / FGGA / FGTS) deform the head
    // mesh in Phase 3b. Phase 1b leaves the head at its race-default
    // shape — every NPC of the same race renders identical until
    // Phase 3 lands.

    // 4.4. Hair + eyebrow head-parts. FNV / FO3 NPC_ records reference
    //      these via the FaceGen recipe (HNAM = hair, PNAM = eyebrow
    //      HDPT). Without them every NPC renders bald + browless on
    //      top of the race base head — the canonical Prospector
    //      Saloon screenshot from 2026-05-26. Pre-fix the form IDs
    //      were parsed onto `NpcFaceGenRecipe` but never consumed.
    //
    //      Both pieces spawn as standalone skinned meshes parented
    //      under `placement_root` and skinned against the same
    //      `skel_map` as the body — same as armor. The head NIF
    //      already includes the skull / skin / eyes geometry; hair
    //      and eyebrows are accessory meshes that slot on top.
    if let (Some(skel), Some(recipe)) = (skel_root, npc.runtime_facegen.as_ref()) {
        let _ = skel; // skinning routes through `skel_map` below; only the presence gate matters.
                      // Hair mesh — `HAIR.model_path` is authored relative to the
                      // meshes root (same as ARMO MODL), so `extract_mesh`'s
                      // `normalize_mesh_path` handles the prefix. `None` when the
                      // NPC's recipe didn't author HNAM (rare on humanoid actors;
                      // possible on creatures using the recipe shell).
        if let Some(hair_form) = recipe.hair_form_id {
            if let Some(hair) = index.hair.get(&hair_form) {
                if !hair.model_path.is_empty() {
                    match tex_provider.extract_mesh(&hair.model_path) {
                        Some(hair_data) => {
                            let (_count, hair_root, _map) = load_nif_bytes_with_skeleton(
                                world,
                                ctx,
                                &hair_data,
                                &hair.model_path,
                                tex_provider,
                                mat_provider.as_deref_mut(),
                                Some(&skel_map),
                                None,
                                None,
                            );
                            if let Some(hr) = hair_root {
                                world.insert(hr, Parent(placement_root));
                                add_child(world, placement_root, hr);
                            }
                        }
                        None => {
                            log::debug!(
                                "NPC {:08X} ({}): hair '{}' not in archives — skipping",
                                npc.form_id,
                                npc.editor_id,
                                hair.model_path,
                            );
                        }
                    }
                }
            }
        }
        // Eyebrow HDPT — same pattern. FNV `PNAM` points at a HDPT
        // record whose MODL is the eyebrow strip mesh. Missing on
        // generic settler / raider NPCs that fall back to the race
        // default; present on most named NPCs.
        if let Some(eyebrow_form) = recipe.eyebrow_form_id {
            if let Some(hdpt) = index.head_parts.get(&eyebrow_form) {
                if !hdpt.model_path.is_empty() {
                    match tex_provider.extract_mesh(&hdpt.model_path) {
                        Some(brow_data) => {
                            let (_count, brow_root, _map) = load_nif_bytes_with_skeleton(
                                world,
                                ctx,
                                &brow_data,
                                &hdpt.model_path,
                                tex_provider,
                                mat_provider.as_deref_mut(),
                                Some(&skel_map),
                                None,
                                None,
                            );
                            if let Some(br) = brow_root {
                                world.insert(br, Parent(placement_root));
                                add_child(world, placement_root, br);
                            }
                        }
                        None => {
                            log::debug!(
                                "NPC {:08X} ({}): eyebrow HDPT '{}' not in archives — skipping",
                                npc.form_id,
                                npc.editor_id,
                                hdpt.model_path,
                            );
                        }
                    }
                }
            }
        }
        // Eye left + right meshes. FNV's RACE record pairs INDX 7 /
        // INDX 8 with the eye NIF paths (e.g. `eyelefthuman.nif` /
        // `eyerighthuman.nif`); the spawner attaches them parented
        // under `placement_root` like hair / eyebrows. Per-NPC eye
        // color comes from `EYES.icon_path` (the `ENAM` form ID on
        // the FaceGen recipe) — applied via a pre-spawn hook that
        // overrides the eye mesh's diffuse texture_path. The eye
        // NIF itself binds a default (race-baseline blue) texture
        // that we replace.
        if let Some(race) = race {
            let eye_texture_override: Option<String> = recipe
                .eyes_form_id
                .and_then(|eye_form| index.eyes.get(&eye_form))
                .filter(|eyes| !eyes.icon_path.is_empty())
                .map(|eyes| eyes.icon_path.clone());
            // Gender → MNAM/FNAM section tag. Vanilla FNV authors the
            // shared head + first INDX 7 / 8 pair before the gender
            // split, then male-specific override variants under MNAM
            // and female under FNAM. Match the NPC's gender so we
            // spawn one pair per side, not all gender variants stacked.
            let want_gender_tag: u8 = match gender {
                Gender::Male => 0,
                Gender::Female => 1,
            };
            for &(part_idx, ref eye_path, sect) in &race.head_parts {
                if part_idx != byroredux_plugin::esm::records::actor::head_part::LEFT_EYE
                    && part_idx != byroredux_plugin::esm::records::actor::head_part::RIGHT_EYE
                {
                    continue;
                }
                if let Some(tag) = sect {
                    if tag != want_gender_tag {
                        continue;
                    }
                }
                if eye_path.is_empty() {
                    continue;
                }
                let eye_data = match tex_provider.extract_mesh(eye_path) {
                    Some(d) => d,
                    None => {
                        log::debug!(
                            "NPC {:08X} ({}): eye mesh '{}' not in archives — skipping",
                            npc.form_id,
                            npc.editor_id,
                            eye_path,
                        );
                        continue;
                    }
                };
                // Pre-spawn hook — swap the eye mesh's diffuse
                // `texture_path` to the per-NPC EYES.icon_path before
                // entity construction. The eye NIF has a single
                // `NiTriShape` for each eyeball, so a flat sweep is
                // safe — every loaded mesh in the scene is "the eye"
                // by construction. Pre-intern the texture path
                // outside the closure so the hook holds a FixedString
                // (no `world` borrow inside the closure → no
                // borrow-checker fight against the `world` mutable
                // borrow `load_nif_bytes_with_skeleton` needs).
                let interned_eye_tex: Option<byroredux_core::string::FixedString> =
                    eye_texture_override.as_ref().map(|path| {
                        let mut pool = world.resource_mut::<StringPool>();
                        pool.intern(path)
                    });
                let mut hook = |scene: &mut byroredux_nif::import::ImportedScene| {
                    let Some(interned) = interned_eye_tex else {
                        return;
                    };
                    for mesh in scene.meshes.iter_mut() {
                        mesh.texture_path = Some(interned);
                    }
                };
                let has_hook = interned_eye_tex.is_some();
                let pre_spawn: Option<&mut dyn FnMut(&mut byroredux_nif::import::ImportedScene)> =
                    if has_hook { Some(&mut hook) } else { None };
                let (_count, eye_root, _map) = load_nif_bytes_with_skeleton(
                    world,
                    ctx,
                    &eye_data,
                    eye_path,
                    tex_provider,
                    mat_provider.as_deref_mut(),
                    Some(&skel_map),
                    None,
                    pre_spawn,
                );
                if let Some(er) = eye_root {
                    world.insert(er, Parent(placement_root));
                    add_child(world, placement_root, er);
                }
            }
        }
    }

    // 4.5. Equipment (M41 Phase 2 / #896 — Phase A.1).
    //
    // Walk `npc.inventory`, populate the ECS `Inventory` component on
    // the placement_root, and for any entry that resolves to an ARMO
    // base record with a model path, load the armor NIF parented under
    // placement_root via the same `external_skeleton: Some(&skel_map)`
    // path the body uses. Skel-shared skin means armor's skinned
    // vertices follow the same bones as the body, so KF idle
    // animation drives both visibly.
    //
    // **Phase A.1 scope**: armor renders ON TOP of the base body NIF
    // (z-fight on overlapping vertices is acceptable for the visible-
    // progress milestone). Phase A.2 will skip the body NIF when any
    // equipped armor's biped slot mask covers the torso bit, once the
    // game-specific BipedObject bit constants are verified against
    // source rather than guessed.
    //
    // The walk also populates `EquipmentSlots.occupants` from each
    // armor's `slot_mask`, even though no consumer reads it yet —
    // M45 save round-trip + M42 AI equip-eval are the eventual
    // consumers, and threading the data through now keeps the
    // foundation honest.
    let mut npc_inventory = Inventory::new();
    let mut equipment_slots = EquipmentSlots::new();
    let mut equipped_armor_count = 0u32;
    let actor_level = npc.level;
    let mut resolved_buf: Vec<u32> = Vec::new();
    // #2094 / SKY-D3-NEW-02 — every armor whose mesh resolves is a
    // *candidate*; whether it still occupies a biped bit isn't known
    // until every inventory entry has run through `equip()` (a later
    // entry can displace an earlier one on an overlapping bit).
    // Collecting candidates and filtering once after the loop
    // (mirroring `build_npc_equip_state`'s prebaked-path fix) is what
    // makes that possible — the pre-fix version loaded each mesh
    // inline as it resolved, before the later displacement was known.
    struct KfArmorCandidate<'a> {
        model_path: &'a str,
        resolved_fid: u32,
        source_fid: u32,
        inv_idx: InventoryIndex,
    }
    let mut candidates: Vec<KfArmorCandidate> = Vec::new();
    // Same TPLT-inherited inventory used for `body_covered` above —
    // the equip dispatch walks this list, not `npc.inventory`, so
    // Lvl* template NPCs equip the gear authored on their base.
    for entry in effective_inventory.iter() {
        // Negative parsed counts are remove-from-inventory deltas, not
        // live state — clamp at runtime per `ItemStack::count` docs.
        let runtime_count = entry.count.max(0) as u32;
        if runtime_count == 0 {
            continue;
        }
        // Push the authored entry verbatim into the runtime inventory.
        // Preserves the LVLI / ARMO ref shape for the eventual save
        // round-trip (M45) — the visible-mesh dispatch below expands
        // for rendering only, without modifying what's stored.
        let stack = ItemStack::new(entry.item_form_id, runtime_count);
        let inv_idx = npc_inventory.push(stack);

        // Expand for visible-mesh dispatch — LVLI references resolve
        // to their level-gated ARMO base record; direct ARMO refs
        // pass through unchanged. Pre-fix the loop did
        // `index.items.get(&entry.item_form_id)` which silently
        // skipped LVLI entries; FO3 / FNV NPCs that reference
        // leveled lists in CNTO (rarer than the Skyrim+ outfit case
        // but it does happen with mod-added gear) silently spawned
        // unequipped. See M41 Phase 2 close-out / #896.
        resolved_buf.clear();
        byroredux_plugin::equip::expand_leveled_form_id(
            entry.item_form_id,
            actor_level,
            index,
            &mut resolved_buf,
        );
        for &resolved_fid in &resolved_buf {
            // Resolve the base record. Non-armor inventory entries
            // (food, ammo, weapons, MISC) still get an `Inventory`
            // row but no visible mesh dispatch — ARMO is the only
            // kind that spawns skinned geometry today. WEAP
            // visibility is a separate follow-up (one-handed pose
            // offset, equipped vs holstered).
            let Some(item) = index.items.get(&resolved_fid) else {
                continue;
            };
            let ItemKind::Armor {
                biped_flags,
                slot_mask,
                ..
            } = item.kind
            else {
                continue;
            };

            // Mark the slot mask occupied. `equip()` returns
            // displaced indices for "armor-replacing-armor" cases;
            // an NPC at spawn time normally lays out one armor per
            // slot, so displacement is rare. Logged at debug level to
            // flag suspicious load-order conflicts (two armors in
            // inventory both claiming UpperBody — happens with mod
            // overrides; also happens with multi-pick LVLIs resolving
            // to overlapping biped slots); the mesh-level exclusion
            // itself happens in the candidate filter below.
            let displaced = equipment_slots.equip(biped_flags, inv_idx);
            if !displaced.is_empty() {
                log::debug!(
                    "NPC {:08X} ({}): armor {:08X} (from CNTO {:08X}) \
                     displaced inventory slots {:?} on biped mask {:#010x}",
                    npc.form_id,
                    npc.editor_id,
                    resolved_fid,
                    entry.item_form_id,
                    displaced,
                    biped_flags,
                );
            }
            let _ = slot_mask;

            // Per-game ARMO → worn-mesh dispatch lives in
            // `byroredux_plugin::equip::resolve_armor_mesh` (Phase B.1).
            // On FNV/FO3/Oblivion the resolver returns
            // `armor.common.model_path`; on Skyrim+/FO4 it walks the
            // ARMA list and picks the race-matched gender-appropriate
            // biped mesh. Either way the spawn site stays uniform.
            let Some(model_path) = byroredux_plugin::equip::resolve_armor_mesh(
                item,
                gender,
                npc.race_form_id,
                index,
                game,
            ) else {
                continue;
            };
            candidates.push(KfArmorCandidate {
                model_path,
                resolved_fid,
                source_fid: entry.item_form_id,
                inv_idx,
            });
        }
    }

    // #2094 / SKY-D3-NEW-02 — drop any candidate whose inventory index
    // no longer occupies a biped bit (fully displaced by a later
    // overlapping entry). A candidate that kept at least one bit still
    // needs its mesh — partial displacement just means a different
    // item now owns the other bit.
    candidates.retain(|c| {
        equipment_slots
            .occupants
            .iter()
            .any(|occupant| *occupant == Some(c.inv_idx))
    });

    for c in &candidates {
        match tex_provider.extract_mesh(c.model_path) {
            Some(armor_data) => {
                let (_count, armor_root, _map) = load_nif_bytes_with_skeleton(
                    world,
                    ctx,
                    &armor_data,
                    c.model_path,
                    tex_provider,
                    mat_provider.as_deref_mut(),
                    Some(&skel_map),
                    None,
                    None,
                );
                if let Some(ar) = armor_root {
                    world.insert(ar, Parent(placement_root));
                    add_child(world, placement_root, ar);
                    equipped_armor_count += 1;
                }
            }
            None => {
                log::debug!(
                    "NPC {:08X} ({}): armor {:08X} (from CNTO {:08X}) \
                     model '{}' not in archives",
                    npc.form_id,
                    npc.editor_id,
                    c.resolved_fid,
                    c.source_fid,
                    c.model_path,
                );
            }
        }
    }
    if equipped_armor_count > 0 {
        log::info!(
            "NPC {:08X} ({}): equipped {} armor mesh(es) from {} inventory entries",
            npc.form_id,
            npc.editor_id,
            equipped_armor_count,
            npc_inventory.len(),
        );
    }
    world.insert(placement_root, npc_inventory);
    world.insert(placement_root, equipment_slots);

    // 5. Idle animation (M41.0 Phase 2 + M41.5 Phase A). Each NPC picks a
    //    clip from the shared per-cell idle pool by a stable FormId hash,
    //    then seeds a per-NPC start phase + playback-speed jitter
    //    (`idle_desync`) so a cell full of actors no longer loops the idle
    //    in lockstep ("mannequins breathing together"). The pool is
    //    pre-registered once per cell load (`load_idle_pool`), so the
    //    `AnimationClipRegistry` doesn't grow per-NPC.
    //
    // KF channels keyed by `Bip01 Spine`, `Bip01 Head`, etc. resolve
    // against the skeleton's BFS-scoped subtree map via
    // `with_root(skel_root)`.
    if let Some(skel) = skel_root {
        if let Some(handle) = pick_idle_handle(idle_pool, npc.form_id) {
            // Read the clip duration so the phase seed lands in
            // [0, duration); a released/empty slot yields 0 → phase 0.
            let duration = world
                .resource::<AnimationClipRegistry>()
                .get(handle)
                .map(|c| c.duration)
                .unwrap_or(0.0);
            let (start_time, speed) = idle_desync(npc.form_id, duration);
            let mut player = AnimationPlayer::new(handle).with_root(skel);
            player.local_time = start_time;
            player.prev_time = start_time;
            player.speed = speed;
            world.insert(placement_root, player);
        }
    }

    // 6. Behavior tagging (M42.1-M42.8) — see `apply_ai_package_behavior`.
    //    #2052 / TD1-003 — shared with `spawn_prebaked_npc_entity` so the
    //    two spawn paths can't diverge on this again (pre-fix the
    //    pre-baked path had no AI-package gating at all).
    apply_ai_package_behavior(world, placement_root, npc, index);

    tag_descendants_as_actor(world, placement_root);
    Some(placement_root)
}

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
fn apply_ai_package_behavior(
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
    };
    let condition_met = |pk: &byroredux_plugin::esm::records::PackRecord| {
        package_conditions_pass(&pk.conditions, world, &cond_ctx)
    };
    // Single resolve (borrows `world` via `condition_met`) — must happen
    // *before* any `world.insert` below; interleaving a read after an
    // insert here is a borrow-checker conflict, since `condition_met`
    // closes over `world: &World`.
    let active_pkg = byroredux_plugin::esm::records::active_package(
        npc.ai_packages.iter().filter_map(|pk| index.packages.get(pk)),
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
            let search_radius = pk.location.map(|loc| loc.radius as f32).filter(|r| *r > 0.0);
            world.insert(
                placement_root,
                byroredux_core::ecs::components::SandboxBehavior { search_radius },
            );
        } else if pk.is_wander() {
            // M42.3: same PLDT-radius-or-default pattern as Sandbox above,
            // same v0 "actor's own position is the center approximation"
            // rationale.
            let wander_radius = pk.location.map(|loc| loc.radius as f32).filter(|r| *r > 0.0);
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
            let travel_radius = pk.location.map(|loc| loc.radius as f32).filter(|r| *r > 0.0);
            let travel_target_form_id = pk.location.and_then(|loc| match loc.target {
                byroredux_plugin::esm::records::PackLocationTarget::NearReference(fid) => {
                    Some(fid)
                }
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
            let follow_distance = pk.target.map(|t| t.count_or_distance as f32).filter(|d| *d > 0.0);
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
            let escort_destination_radius =
                pk.location.map(|loc| loc.radius as f32).filter(|r| *r > 0.0);
            let escort_destination_form_id = pk.location.and_then(|loc| match loc.target {
                byroredux_plugin::esm::records::PackLocationTarget::NearReference(fid) => {
                    Some(fid)
                }
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
            let guard_radius = pk.location.map(|loc| loc.radius as f32).filter(|r| *r > 0.0);
            let guard_anchor_form_id = pk.location.and_then(|loc| match loc.target {
                byroredux_plugin::esm::records::PackLocationTarget::NearReference(fid) => {
                    Some(fid)
                }
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
            let patrol_radius = pk.location.map(|loc| loc.radius as f32).filter(|r| *r > 0.0);
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

/// Spawn an NPC actor entity for the pre-baked-FaceGen path
/// (Skyrim / FO4 / FO76 / Starfield) — M41.0 Phase 4. Returns the
/// placement-root `EntityId`. Returns `None` when the game is on
/// the kf-era runtime-FaceGen track (those route through
/// [`spawn_npc_entity`] instead).
///
/// Pre-baked path: `meshes\actors\character\facegendata\facegeom\
/// <plugin>\<formid:08x>.nif` carries the per-NPC **head only**
/// (matching Bethesda's FaceGen SDK head-only bake convention — a
/// real vanilla FaceGeom NIF has no torso/limb geometry; see #2093 /
/// SKY-D3-NEW-01) — no FaceGen morph evaluator (the SDK pre-applies
/// the slider table before shipping). Body coverage comes from
/// `RACE.WNAM`'s default skin ARMO (equipped as the lowest-priority
/// layer in [`build_npc_equip_state`]) plus whatever OTFT/CNTO armor
/// resolves on top of it. Skeleton load + skinning resolution stays
/// identical to the kf-era path; the head NIF replaces the race-
/// default head only.
///
/// **Animation deferred**: Skyrim+ vanilla ships zero `.kf` files
/// (Havok `.hkx` only). Pre-baked-track NPCs spawn in bind pose
/// today; M41.x lands a Havok stub for idle.
#[allow(clippy::too_many_arguments)]
pub fn spawn_prebaked_npc_entity(
    world: &mut World,
    ctx: &mut VulkanContext,
    npc: &NpcRecord,
    game: GameKind,
    tex_provider: &TextureProvider,
    mut mat_provider: Option<&mut MaterialProvider>,
    plugin_name: &str,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    index: &EsmIndex,
) -> Option<EntityId> {
    if !game.uses_prebaked_facegen() {
        return None;
    }
    let gender = Gender::from_acbs_flags(npc.acbs_flags);

    // 1. Placement root.
    let placement_root = world.spawn();
    world.insert(placement_root, Transform::new(ref_pos, ref_rot, ref_scale));
    world.insert(
        placement_root,
        GlobalTransform::new(ref_pos, ref_rot, ref_scale),
    );
    if !npc.editor_id.is_empty() {
        let mut pool = world.resource_mut::<StringPool>();
        let sym = pool.intern(&npc.editor_id);
        drop(pool);
        world.insert(placement_root, Name(sym));
    }
    stamp_faction_ranks(world, placement_root, npc);
    stamp_actor_values(world, placement_root, npc, index, game);
    stamp_character_components(world, placement_root, npc);

    // 2. Equip state — built from the NPC record + ESM index alone
    //    so it lands on the placement root **before** any archive
    //    I/O. Hoisting the build here keeps the equip data observable
    //    for diagnostics + the future save round-trip (M45) even when
    //    the mesh load fails. The armor-mesh dispatch loop further
    //    down still gates on a resolved `skel_map` — meshes don't
    //    materialise without bones, but the components do.
    //
    // 2026-05-26 retrospective: the pre-fix version of this comment
    // claimed FO4 vanilla data shipped *only* `_1stperson\skeleton.nif`
    // + `.hkx` for the 3rd-person humanoid skeleton. That conclusion
    // was based on a BA2 scan for the SSE-shaped `character assets\`
    // path (with space), which doesn't exist in FO4 — but the file
    // *is* in `Fallout4 - Meshes.ba2`, just under the renamed
    // `characterassets\` folder (no space). The hoisted equip insert
    // here was the right defence-in-depth, but the skeleton was never
    // actually missing — only the resolver was looking up the wrong
    // path. See `humanoid_skeleton_path` for the corrected table.
    let equip_state = build_npc_equip_state(npc, index, game, gender);
    let armor_to_spawn = equip_state.armor_to_spawn;
    world.insert(placement_root, equip_state.inventory);
    world.insert(placement_root, equip_state.equipment_slots);

    // 3. Skeleton — shared NIF resolved through `humanoid_skeleton_path`.
    //    Path differs per game family: FNV/FO3 use `\characters\_male\`,
    //    Skyrim uses `\character\character assets\` (with space), FO4/FO76
    //    use `\character\characterassets\` (no space), Starfield uses
    //    `\human\characterassets\`. The pre-baked head NIF carries its
    //    own `BSTriShape`-skinned mesh that resolves bones against this
    //    skeleton via the shared `external_skeleton` map.
    let skel_path = humanoid_skeleton_path(game)?;
    let skel_data = match tex_provider.extract_mesh(skel_path) {
        Some(d) => d,
        None => {
            log::warn!(
                "NPC {:08X} ({}): skeleton '{}' not in archives — skipping mesh spawn (equip state retained)",
                npc.form_id,
                npc.editor_id,
                skel_path,
            );
            return Some(placement_root);
        }
    };
    let (_skel_count, skel_root, skel_map) = load_nif_bytes_with_skeleton(
        world,
        ctx,
        &skel_data,
        skel_path,
        tex_provider,
        mat_provider.as_deref_mut(),
        None,
        None,
        None,
    );
    // #1698 — keyframe the ragdoll bones so they don't free-simulate (and pin
    // the physics step) while the actor is alive. Must run before the first
    // physics_sync registers them.
    keyframe_live_ragdoll_bones(world, &skel_map);
    if let Some(sr) = skel_root {
        world.insert(sr, Parent(placement_root));
        add_child(world, placement_root, sr);
    }

    // 4. Pre-baked FaceGen NIF (per-NPC head+body in one mesh).
    let Some(facegen_path) = prebaked_facegen_nif_path(plugin_name, npc.form_id) else {
        log::debug!(
            "NPC {:08X}: empty plugin name in load order; skipping pre-baked FaceGen",
            npc.form_id,
        );
        return Some(placement_root);
    };
    let facegen_data = match tex_provider.extract_mesh(&facegen_path) {
        Some(d) => d,
        None => {
            log::debug!(
                "NPC {:08X} ({}): pre-baked FaceGen '{}' not in archives — \
                 NPC visible as skeleton-only (no per-NPC mesh)",
                npc.form_id,
                npc.editor_id,
                facegen_path,
            );
            return Some(placement_root);
        }
    };
    // #2095 / SKY-D3-NEW-03 — per-NPC face-tint DDS. Checked against the
    // texture archives (not just computed) so an NPC that ships no tint
    // (modded / incomplete data) keeps the FaceGeom NIF's own baked
    // diffuse instead of falling back to the magenta "missing texture"
    // checker — `resolve_texture` can't tell "no override" apart from
    // "override authored but file missing" once a path reaches it.
    let tint_path = prebaked_facegen_tint_path(plugin_name, npc.form_id)
        .filter(|p| tex_provider.extract(p).is_some());
    let (_fg_count, fg_root, _fg_map) = load_nif_bytes_with_skeleton(
        world,
        ctx,
        &facegen_data,
        &facegen_path,
        tex_provider,
        mat_provider.as_deref_mut(),
        Some(&skel_map),
        tint_path.as_deref(),
        None,
    );
    if let Some(fr) = fg_root {
        world.insert(fr, Parent(placement_root));
        add_child(world, placement_root, fr);
    }

    // 5. Armor mesh dispatch — `Inventory` + `EquipmentSlots` are
    //    already inserted above (step 2). Loop the pre-built
    //    `armor_to_spawn` list and load each piece's worn mesh via
    //    `byroredux_plugin::equip::resolve_armor_mesh` — which
    //    dispatches per-game: Skyrim+ walks the ARMA list, race-
    //    matches, and picks the gender-appropriate biped path.
    //
    //    **Body suppression NOT applied here.** Body coverage on this
    //    path comes from the race-default skin ARMO (`RACE.WNAM`,
    //    #2093 / SKY-D3-NEW-01) queued into `armor_to_spawn` above —
    //    a single combined mesh, not per-body-part NIFs like the
    //    kf-era race body — so it can't be selectively hidden the way
    //    `armor_covers_main_body` skips `upperbody.nif` there without
    //    per-shape `BSDismemberSkinInstance` partition inspection.
    //    Phase B.2 renders armor on top of the skin body and accepts
    //    whatever clipping happens — same compromise Phase A.1 made
    //    before A.2 added the kf-era body-skip.
    let mut equipped_armor_count = 0u32;
    for armor in &armor_to_spawn {
        match tex_provider.extract_mesh(armor.model_path) {
            Some(armor_data) => {
                let (_count, armor_root, _map) = load_nif_bytes_with_skeleton(
                    world,
                    ctx,
                    &armor_data,
                    armor.model_path,
                    tex_provider,
                    mat_provider.as_deref_mut(),
                    Some(&skel_map),
                    None,
                    None,
                );
                if let Some(ar) = armor_root {
                    world.insert(ar, Parent(placement_root));
                    add_child(world, placement_root, ar);
                    equipped_armor_count += 1;
                }
            }
            None => {
                log::debug!(
                    "NPC {:08X} ({}): armor {:08X} model '{}' not in archives",
                    npc.form_id,
                    npc.editor_id,
                    armor.form_id,
                    armor.model_path,
                );
            }
        }
    }
    if equipped_armor_count > 0 {
        log::info!(
            "NPC {:08X} ({}): equipped {} armor mesh(es) on pre-baked path \
             ({} armor candidates queued)",
            npc.form_id,
            npc.editor_id,
            equipped_armor_count,
            armor_to_spawn.len(),
        );
    }

    // 6. Behavior tagging (M42.1-M42.8) — see `apply_ai_package_behavior`.
    //    #2052 / TD1-003 — this path previously had no AI-package gating
    //    at all, diverging from `spawn_npc_entity`'s kf-era path.
    apply_ai_package_behavior(world, placement_root, npc, index);

    tag_descendants_as_actor(world, placement_root);
    Some(placement_root)
}

/// Walk the subtree rooted at `root` and tag every descendant entity
/// carrying a [`MeshHandle`] with [`RenderLayer::Actor`]. Loose-NIF
/// spawns at `scene::load_nif_bytes` default each mesh entity to
/// `RenderLayer::Architecture` (no REFR base record available), so
/// every NPC body / head / armor / FaceGen mesh comes out of that path
/// with the wrong layer for depth-bias purposes — without this
/// override every standing NPC z-fights the floor at the foot-plant
/// patch. Called from each [`spawn_npc_entity`] / [`spawn_prebaked_npc_entity`]
/// success path before returning. BFS over `Children`, mirrors
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
mod tests {
    use super::*;
    use byroredux_core::ecs::components::{
        EscortBehavior, FollowBehavior, GuardBehavior, PatrolBehavior, SandboxBehavior,
        TravelBehavior, WanderBehavior,
    };

    // ── M41.5 Phase A — idle desync + pool selection ──────────────────

    #[test]
    fn idle_desync_is_deterministic_per_form_id() {
        // Same FormId → identical seed every call (save/reload + cell
        // re-stream must re-derive the same phase).
        let a = idle_desync(0x0010_A2F3, 2.0);
        let b = idle_desync(0x0010_A2F3, 2.0);
        assert_eq!(a, b);
    }

    #[test]
    fn idle_desync_phase_in_range_and_varies() {
        let duration = 2.0_f32;
        // Sequential FormIds (Bethesda hands them out in runs) must not
        // collapse to near-identical phases.
        let (p0, s0) = idle_desync(0x0100_0001, duration);
        let (p1, s1) = idle_desync(0x0100_0002, duration);
        let (p2, s2) = idle_desync(0x0100_0003, duration);
        for p in [p0, p1, p2] {
            assert!(p >= 0.0 && p < duration, "phase {p} out of [0,{duration})");
        }
        for s in [s0, s1, s2] {
            assert!(
                (s - 1.0).abs() <= IDLE_SPEED_JITTER + 1e-6,
                "speed {s} outside ±{IDLE_SPEED_JITTER}",
            );
        }
        // Adjacent ids diverge in phase (avalanche hash, not sequential).
        assert!((p0 - p1).abs() > 1e-4 && (p1 - p2).abs() > 1e-4);
    }

    #[test]
    fn idle_desync_zero_duration_yields_zero_phase() {
        // Empty / released clip slot: no phase, speed still jittered.
        let (phase, speed) = idle_desync(0xDEAD_BEEF, 0.0);
        assert_eq!(phase, 0.0);
        assert!((speed - 1.0).abs() <= IDLE_SPEED_JITTER + 1e-6);
    }

    #[test]
    fn pick_idle_handle_empty_pool_is_none() {
        assert_eq!(pick_idle_handle(&[], 0x1234), None);
    }

    #[test]
    fn pick_idle_handle_size_one_always_resolves() {
        // Today's floor: the size-1 pool always yields the single handle
        // regardless of FormId.
        assert_eq!(pick_idle_handle(&[7], 0x0001), Some(7));
        assert_eq!(pick_idle_handle(&[7], 0xF00D_BABE), Some(7));
    }

    #[test]
    fn pick_idle_handle_is_deterministic_and_in_bounds() {
        // Forward-compatible: a >1 pool selects stably and in-range.
        let pool = [10_u32, 11, 12, 13];
        for id in [0x01_u32, 0x02, 0xABCD, 0x0010_A2F3] {
            let h = pick_idle_handle(&pool, id).unwrap();
            assert!(pool.contains(&h));
            assert_eq!(pick_idle_handle(&pool, id), Some(h), "stable per id");
        }
    }

    #[test]
    fn prebaked_facegen_nif_path_matches_vanilla_layout() {
        // Vanilla SSE Whiterun Mikael (FormID 0x00013BBE in
        // Skyrim.esm). Path scheme verified by BSA scan 2026-04-28.
        assert_eq!(
            prebaked_facegen_nif_path("Skyrim.esm", 0x00013BBE),
            Some(
                r"meshes\actors\character\facegendata\facegeom\skyrim.esm\00013bbe.nif".to_string(),
            ),
        );
        // Plugin name is lower-cased; FormID rendered as 8 lowercase hex.
        assert_eq!(
            prebaked_facegen_nif_path("Dawnguard.esm", 0x0001684C),
            Some(
                r"meshes\actors\character\facegendata\facegeom\dawnguard.esm\0001684c.nif"
                    .to_string(),
            ),
        );
    }

    #[test]
    fn prebaked_facegen_tint_path_mirrors_geom_layout() {
        assert_eq!(
            prebaked_facegen_tint_path("Skyrim.esm", 0x00013BBE),
            Some(
                r"textures\actors\character\facegendata\facetint\skyrim.esm\00013bbe.dds"
                    .to_string(),
            ),
        );
    }

    #[test]
    fn prebaked_paths_reject_empty_plugin() {
        assert!(prebaked_facegen_nif_path("", 0x42).is_none());
        assert!(prebaked_facegen_tint_path("", 0x42).is_none());
    }

    #[test]
    fn facegen_sidecar_path_swaps_extension() {
        assert_eq!(
            facegen_sidecar_path(r"meshes\characters\head\headhuman.nif", "egm"),
            Some(r"meshes\characters\head\headhuman.egm".to_string()),
        );
        // Mixed-case suffix still matches.
        assert_eq!(
            facegen_sidecar_path(r"Characters\Head\HeadHuman.NIF", "egt"),
            Some(r"Characters\Head\HeadHuman.egt".to_string()),
        );
        // Wrong extension → None.
        assert!(facegen_sidecar_path(r"foo\bar\baz.dds", "egm").is_none());
    }

    #[test]
    fn gender_decodes_acbs_bit_0() {
        assert_eq!(Gender::from_acbs_flags(0), Gender::Male);
        assert_eq!(Gender::from_acbs_flags(0x0000_0001), Gender::Female);
        // High bits unrelated to gender; bit 0 is the only authority.
        assert_eq!(Gender::from_acbs_flags(0xFFFF_FFFE), Gender::Male);
        assert_eq!(Gender::from_acbs_flags(0xFFFF_FFFF), Gender::Female);
    }

    #[test]
    fn skeleton_path_per_game() {
        assert_eq!(
            humanoid_skeleton_path(GameKind::Fallout3NV),
            Some(r"meshes\characters\_male\skeleton.nif"),
        );
        // Skyrim alone uses the space-separated `character assets`
        // folder. Bethesda compressed it to `characterassets` for
        // FO4 onward; the function's docstring carries the BA2-scan
        // evidence (2026-05-26).
        assert_eq!(
            humanoid_skeleton_path(GameKind::Skyrim),
            Some(r"meshes\actors\character\character assets\skeleton.nif"),
        );
        assert_eq!(
            humanoid_skeleton_path(GameKind::Fallout4),
            Some(r"meshes\actors\character\characterassets\skeleton.nif"),
        );
        assert_eq!(
            humanoid_skeleton_path(GameKind::Fallout76),
            Some(r"meshes\actors\character\characterassets\skeleton.nif"),
        );
        // Starfield humanoids live under `\human\`, not `\character\`.
        assert_eq!(
            humanoid_skeleton_path(GameKind::Starfield),
            Some(r"meshes\actors\human\characterassets\skeleton.nif"),
        );
    }

    /// Regression test for #793: kf-era humanoids must surface
    /// `lefthand.nif` and `righthand.nif` alongside `upperbody.nif`.
    /// Pre-fix the resolver returned a single path and every NPC
    /// rendered handless because the hand mesh was never loaded.
    #[test]
    fn body_paths_kf_era_include_separate_hand_meshes() {
        for game in [GameKind::Oblivion, GameKind::Fallout3NV] {
            let paths = humanoid_body_paths(game);
            assert_eq!(
                paths.len(),
                3,
                "{game:?} should ship upperbody + 2 hands, got {paths:?}",
            );
            assert!(
                paths.iter().any(|p| p.ends_with("upperbody.nif")),
                "{game:?} missing upperbody: {paths:?}",
            );
            assert!(
                paths.iter().any(|p| p.ends_with("lefthand.nif")),
                "{game:?} missing lefthand: {paths:?}",
            );
            assert!(
                paths.iter().any(|p| p.ends_with("righthand.nif")),
                "{game:?} missing righthand: {paths:?}",
            );
        }
    }

    /// Skyrim+/FO4+ stand on the pre-baked-FaceGen track — head + body
    /// ship in one per-NPC `facegeom.nif`. The body resolver must
    /// return an empty slice for those variants so the NPC-spawn loop
    /// no-ops on body load and lets the FaceGen path (Phase 4) fill in.
    #[test]
    fn body_paths_facegen_era_returns_empty_slice() {
        for game in [
            GameKind::Skyrim,
            GameKind::Fallout4,
            GameKind::Fallout76,
            GameKind::Starfield,
        ] {
            let paths = humanoid_body_paths(game);
            assert!(
                paths.is_empty(),
                "{game:?} should defer body to FaceGen path, got {paths:?}",
            );
        }
    }

    #[test]
    fn idle_kf_path_only_for_kf_era_games() {
        // FNV / FO3 ship `.kf` clips.
        assert!(humanoid_default_idle_kf_path(GameKind::Fallout3NV).is_some());
        assert!(humanoid_default_idle_kf_path(GameKind::Oblivion).is_some());

        // Skyrim+ uses Havok `.hkx` — no `.kf` exists in vanilla.
        // Verified by BSA scan 2026-04-28 (Skyrim SE Meshes0 + Meshes1
        // + Animations BSAs all return 0 `.kf` hits).
        assert!(humanoid_default_idle_kf_path(GameKind::Skyrim).is_none());
        assert!(humanoid_default_idle_kf_path(GameKind::Fallout4).is_none());
        assert!(humanoid_default_idle_kf_path(GameKind::Fallout76).is_none());
        assert!(humanoid_default_idle_kf_path(GameKind::Starfield).is_none());
    }

    // ── #1658 (SKY-D3-02): prebaked equip state routes inventory through
    //    the TPLT chain, identical to the kf-era path. ──────────────────

    /// Minimal `NpcRecord` for the equip-state tests (mirrors the 21-field
    /// shape; callers tweak template / inventory fields).
    fn test_npc(form_id: u32, edid: &str) -> NpcRecord {
        NpcRecord {
            form_id,
            editor_id: edid.to_string(),
            full_name: String::new(),
            model_path: String::new(),
            race_form_id: 0,
            class_form_id: 0,
            voice_form_id: 0,
            factions: Vec::new(),
            inventory: Vec::new(),
            default_outfit: None,
            ai_packages: Vec::new(),
            death_item_form_id: 0,
            level: 1,
            disposition_base: 50,
            acbs_flags: 0,
            has_script: false,
            script_form_id: 0,
            script_instance: None,
            face_morphs: None,
            runtime_facegen: None,
            template_form_id: 0,
            template_flags: 0,
            ..Default::default()
        }
    }

    /// A known (non-leveled) MISC item so `expand_leveled_form_id` lands
    /// the form in the inventory (it only pushes forms present in
    /// `index.items` or expandable as an LVLI).
    fn misc_item(form_id: u32) -> byroredux_plugin::esm::records::ItemRecord {
        byroredux_plugin::esm::records::ItemRecord {
            form_id,
            common: byroredux_plugin::esm::records::common::CommonItemFields::default(),
            kind: ItemKind::Misc,
        }
    }

    /// A templated Skyrim NPC with an empty own CNTO and
    /// `TEMPLATE_FLAG_USE_INVENTORY` set must inherit its base's gear via
    /// the TPLT walk — pre-fix `build_npc_equip_state` read `npc.inventory`
    /// directly and the actor spawned naked.
    #[test]
    fn prebaked_equip_state_inherits_templated_inventory() {
        use byroredux_core::ecs::components::InventoryIndex;
        use byroredux_plugin::equip::TEMPLATE_FLAG_USE_INVENTORY;
        use byroredux_plugin::esm::records::NpcInventoryEntry;

        const TEMPLATE: u32 = 0x0100_0001;
        const BASE: u32 = 0x0100_0002;
        const GEAR: u32 = 0x0000_AAAA;

        let mut base = test_npc(BASE, "BaseTemplatedNpc");
        base.inventory.push(NpcInventoryEntry {
            item_form_id: GEAR,
            count: 1,
        });

        let mut templated = test_npc(TEMPLATE, "LvlTemplatedNpc");
        templated.template_form_id = BASE;
        templated.template_flags = TEMPLATE_FLAG_USE_INVENTORY;

        let mut index = EsmIndex {
            game: GameKind::Skyrim,
            ..Default::default()
        };
        index.npcs.insert(BASE, base);
        index.items.insert(GEAR, misc_item(GEAR));

        let state = build_npc_equip_state(&templated, &index, GameKind::Skyrim, Gender::Male);

        assert_eq!(
            state.inventory.len(),
            1,
            "templated NPC must inherit its base's CNTO via TPLT (#1658) — \
             pre-fix the inventory was empty (naked actor)"
        );
        assert_eq!(
            state.inventory.get(InventoryIndex(0)).map(|s| s.base_form_id),
            Some(GEAR),
            "the inherited gear form must be the one that landed in the inventory"
        );
    }

    /// Control: an NPC with its own CNTO and no template still equips from
    /// its own inventory (the named Bannered Mare NPCs the audit flagged as
    /// unaffected). `resolve_inherited_inventory` returns the own inventory
    /// when no template applies, so this path is unchanged.
    #[test]
    fn prebaked_equip_state_uses_own_inventory_without_template() {
        use byroredux_core::ecs::components::InventoryIndex;
        use byroredux_plugin::esm::records::NpcInventoryEntry;

        const NPC: u32 = 0x0100_0003;
        const GEAR: u32 = 0x0000_BBBB;

        let mut npc = test_npc(NPC, "OwnInventoryNpc");
        npc.inventory.push(NpcInventoryEntry {
            item_form_id: GEAR,
            count: 1,
        });

        let mut index = EsmIndex {
            game: GameKind::Skyrim,
            ..Default::default()
        };
        index.items.insert(GEAR, misc_item(GEAR));

        let state = build_npc_equip_state(&npc, &index, GameKind::Skyrim, Gender::Male);
        assert_eq!(
            state.inventory.get(InventoryIndex(0)).map(|s| s.base_form_id),
            Some(GEAR),
            "no-template NPC equips from its own inventory unchanged"
        );
    }

    // ── #2093 (SKY-D3-NEW-01) / #2094 (SKY-D3-NEW-02) — RACE.WNAM skin
    //    fallback + displaced-mesh exclusion on the prebaked path. ──────

    fn skyrim_armor_item(
        form_id: u32,
        biped_flags: u32,
        armatures: Vec<u32>,
    ) -> byroredux_plugin::esm::records::ItemRecord {
        byroredux_plugin::esm::records::ItemRecord {
            form_id,
            common: byroredux_plugin::esm::records::common::CommonItemFields::default(),
            kind: ItemKind::Armor {
                biped_flags,
                dt: 0.0,
                dr: 0,
                health: 0,
                slot_mask: 0,
                armor_rating_x100: 0,
                armor_type: Some(1),
                armatures,
            },
        }
    }

    fn arma(form_id: u32, mesh_path: &str) -> byroredux_plugin::esm::records::ArmaRecord {
        byroredux_plugin::esm::records::ArmaRecord {
            form_id,
            editor_id: String::new(),
            biped_flags: 0,
            general_flags: 0,
            dt: 0,
            dr: 0,
            race_form_id: 0,
            male_biped_model: mesh_path.to_string(),
            female_biped_model: mesh_path.to_string(),
            additional_races: Vec::new(),
        }
    }

    /// The Hulda/Mikael scenario from the audit: OTFT covers only Feet
    /// (`0x80`), so without a `WNAM` skin fallback the NPC has zero mesh
    /// source for torso/hands. With the fallback wired, the race skin
    /// (covering the non-overlapping Torso+Hands bits) and the feet
    /// armor both survive into `armor_to_spawn`.
    #[test]
    fn prebaked_equip_state_falls_back_to_race_skin_for_uncovered_slots() {
        const RACE: u32 = 0x0100_0020;
        const SKIN: u32 = 0x0100_0021;
        const SKIN_ARMA: u32 = 0x0100_0022;
        const FEET: u32 = 0x0100_0023;
        const FEET_ARMA: u32 = 0x0100_0024;
        const TORSO_HANDS: u32 = 0x0004 | 0x0010;
        const FEET_BIT: u32 = 0x0080;

        let mut race = byroredux_plugin::esm::records::RaceRecord {
            form_id: RACE,
            editor_id: String::new(),
            full_name: String::new(),
            description: String::new(),
            skill_bonuses: Vec::new(),
            body_models: Vec::new(),
            head_parts: Vec::new(),
            base_height: (1.0, 1.0),
            base_weight: (1.0, 1.0),
            race_flags: 0,
            base_attributes: None,
            default_hair: None,
            voice_forms: None,
            facegen_main_clamp: None,
            facegen_face_clamp: None,
            race_reactions: Vec::new(),
            default_skin: None,
        };
        race.default_skin = Some(SKIN);

        let mut npc = test_npc(0x0100_0025, "SkinFallbackNpc");
        npc.race_form_id = RACE;
        npc.inventory.push(byroredux_plugin::esm::records::NpcInventoryEntry {
            item_form_id: FEET,
            count: 1,
        });

        let mut index = EsmIndex {
            game: GameKind::Skyrim,
            ..Default::default()
        };
        index.races.insert(RACE, race);
        index
            .items
            .insert(SKIN, skyrim_armor_item(SKIN, TORSO_HANDS, vec![SKIN_ARMA]));
        index
            .armor_addons
            .insert(SKIN_ARMA, arma(SKIN_ARMA, r"actors\character\skin.nif"));
        index
            .items
            .insert(FEET, skyrim_armor_item(FEET, FEET_BIT, vec![FEET_ARMA]));
        index
            .armor_addons
            .insert(FEET_ARMA, arma(FEET_ARMA, r"armor\boots\boots.nif"));

        let state = build_npc_equip_state(&npc, &index, GameKind::Skyrim, Gender::Male);

        assert_eq!(
            state.armor_to_spawn.len(),
            2,
            "non-overlapping skin + feet armor must both queue a mesh, got {:?}",
            state
                .armor_to_spawn
                .iter()
                .map(|a| a.model_path)
                .collect::<Vec<_>>()
        );
        assert!(
            state
                .armor_to_spawn
                .iter()
                .any(|a| a.model_path == r"actors\character\skin.nif"),
            "race skin must fall back to cover the torso/hands OTFT/CNTO left uncovered"
        );
        assert!(
            state
                .armor_to_spawn
                .iter()
                .any(|a| a.model_path == r"armor\boots\boots.nif"),
            "the actually-equipped feet armor must still spawn"
        );
    }

    /// #2094 (SKY-D3-NEW-02) — when the equipped gear fully overlaps the
    /// race skin's biped bit, the skin is displaced and must NOT spawn a
    /// second (z-fighting) mesh alongside the winner.
    #[test]
    fn prebaked_equip_state_drops_skin_mesh_fully_displaced_by_gear() {
        const RACE: u32 = 0x0100_0030;
        const SKIN: u32 = 0x0100_0031;
        const SKIN_ARMA: u32 = 0x0100_0032;
        const TORSO: u32 = 0x0100_0033;
        const TORSO_ARMA: u32 = 0x0100_0034;
        const TORSO_BIT: u32 = 0x0004;

        let mut race = byroredux_plugin::esm::records::RaceRecord {
            form_id: RACE,
            editor_id: String::new(),
            full_name: String::new(),
            description: String::new(),
            skill_bonuses: Vec::new(),
            body_models: Vec::new(),
            head_parts: Vec::new(),
            base_height: (1.0, 1.0),
            base_weight: (1.0, 1.0),
            race_flags: 0,
            base_attributes: None,
            default_hair: None,
            voice_forms: None,
            facegen_main_clamp: None,
            facegen_face_clamp: None,
            race_reactions: Vec::new(),
            default_skin: None,
        };
        race.default_skin = Some(SKIN);

        let mut npc = test_npc(0x0100_0035, "SkinDisplacedNpc");
        npc.race_form_id = RACE;
        npc.inventory.push(byroredux_plugin::esm::records::NpcInventoryEntry {
            item_form_id: TORSO,
            count: 1,
        });

        let mut index = EsmIndex {
            game: GameKind::Skyrim,
            ..Default::default()
        };
        index.races.insert(RACE, race);
        index
            .items
            .insert(SKIN, skyrim_armor_item(SKIN, TORSO_BIT, vec![SKIN_ARMA]));
        index
            .armor_addons
            .insert(SKIN_ARMA, arma(SKIN_ARMA, r"actors\character\skin.nif"));
        index.items.insert(
            TORSO,
            skyrim_armor_item(TORSO, TORSO_BIT, vec![TORSO_ARMA]),
        );
        index
            .armor_addons
            .insert(TORSO_ARMA, arma(TORSO_ARMA, r"armor\robe\robe.nif"));

        let state = build_npc_equip_state(&npc, &index, GameKind::Skyrim, Gender::Male);

        assert_eq!(
            state.armor_to_spawn.len(),
            1,
            "the skin's fully-overlapping bit must be displaced, leaving only the winner, got {:?}",
            state
                .armor_to_spawn
                .iter()
                .map(|a| a.model_path)
                .collect::<Vec<_>>()
        );
        assert_eq!(state.armor_to_spawn[0].model_path, r"armor\robe\robe.nif");
    }

    // ── #2052 / TD1-003 — `apply_ai_package_behavior` shared helper ───
    //
    // Extracted out of `spawn_npc_entity` and now also called by
    // `spawn_prebaked_npc_entity` (which previously had no AI-package
    // gating at all — the SIBLING gap the issue flagged). Needs only a
    // `World` + `NpcRecord` + `EsmIndex`, no Vulkan device, so it's
    // testable in isolation unlike the two spawn functions themselves.

    fn pack_with_procedure(form_id: u32, procedure_type: u32) -> byroredux_plugin::esm::records::PackRecord {
        byroredux_plugin::esm::records::PackRecord {
            form_id,
            procedure_type,
            ..Default::default()
        }
    }

    #[test]
    fn apply_ai_package_behavior_tags_sandbox_from_active_package() {
        let mut world = World::new();
        let placement_root = world.spawn();
        let npc = NpcRecord {
            ai_packages: vec![0xAAAA],
            ..Default::default()
        };
        let mut index = EsmIndex::default();
        index.packages.insert(
            0xAAAA,
            pack_with_procedure(0xAAAA, byroredux_plugin::esm::records::misc::ai::PROCEDURE_SANDBOX),
        );

        apply_ai_package_behavior(&mut world, placement_root, &npc, &index);

        assert!(
            world.get::<SandboxBehavior>(placement_root).is_some(),
            "active Sandbox package must tag SandboxBehavior"
        );
        assert!(world.get::<WanderBehavior>(placement_root).is_none());
        assert!(world.get::<TravelBehavior>(placement_root).is_none());
    }

    #[test]
    fn apply_ai_package_behavior_tags_travel_with_location_from_active_package() {
        let mut world = World::new();
        let placement_root = world.spawn();
        let npc = NpcRecord {
            form_id: 0x1234,
            ai_packages: vec![0xBBBB],
            ..Default::default()
        };
        let mut pk = pack_with_procedure(
            0xBBBB,
            byroredux_plugin::esm::records::misc::ai::PROCEDURE_TRAVEL,
        );
        pk.location = Some(byroredux_plugin::esm::records::PackLocation {
            location_type: 0,
            target: byroredux_plugin::esm::records::PackLocationTarget::NearReference(0xC0FF_EE00),
            radius: 256,
        });
        let mut index = EsmIndex::default();
        index.packages.insert(0xBBBB, pk);

        apply_ai_package_behavior(&mut world, placement_root, &npc, &index);

        let travel = world
            .get::<TravelBehavior>(placement_root)
            .expect("active Travel package must tag TravelBehavior");
        assert_eq!(travel.radius, Some(256.0));
        assert_eq!(travel.target_form_id, Some(0xC0FF_EE00));
        assert_eq!(travel.form_id, 0x1234);
        assert!(world.get::<SandboxBehavior>(placement_root).is_none());
    }

    /// No `ai_packages` at all → no active package → no Behavior
    /// component of any kind gets tagged. Mirrors the pre-#2052 pre-baked
    /// path's behavior for NPCs with no packages, and confirms the
    /// shared helper doesn't tag anything speculatively.
    #[test]
    fn apply_ai_package_behavior_tags_nothing_without_ai_packages() {
        let mut world = World::new();
        let placement_root = world.spawn();
        let npc = NpcRecord::default(); // ai_packages: vec![]
        let index = EsmIndex::default();

        apply_ai_package_behavior(&mut world, placement_root, &npc, &index);

        assert!(world.get::<SandboxBehavior>(placement_root).is_none());
        assert!(world.get::<WanderBehavior>(placement_root).is_none());
        assert!(world.get::<TravelBehavior>(placement_root).is_none());
        assert!(world.get::<FollowBehavior>(placement_root).is_none());
        assert!(world.get::<EscortBehavior>(placement_root).is_none());
        assert!(world.get::<GuardBehavior>(placement_root).is_none());
        assert!(world.get::<PatrolBehavior>(placement_root).is_none());
    }
}
