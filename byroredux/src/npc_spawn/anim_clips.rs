//! Idle / walk / sit animation clip loading and the idle desync that keeps a crowd from moving in lockstep.

use super::*;

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
pub(crate) fn load_kf_clip_by_path(
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

/// Archive paths of the humanoid **walk-forward** cycles for a game, for
/// warm-up (`load_walk_clips`): every variant the game ships, so a spawn
/// job's finalize lookup by `(game, gender, is_child)` never pays BSA
/// extract + parse on the spawn path. Order is irrelevant — the registry
/// is path-keyed.
///
/// Like [`humanoid_default_idle_kf_path`], the paths are verified against
/// the named archives, not guessed (BSA scans 2026-09-18):
///
/// - **FNV / FO3** ship the unarmed move-type walk split by body class:
///   `locomotion\male\mtforward.kf`, `locomotion\female\mtforward.kf`,
///   and `locomotion\child\mtforward.kf` under `meshes\characters\_male\`
///   (verified present in both games' `Fallout - Meshes.bsa`).
/// - **Oblivion** has no `locomotion\` directory at all (BSA scan
///   2026-09-18: zero hits). Its closest verified forward walk is the
///   hand-to-hand stance cycle `meshes\characters\_male\handtohandforward.kf`
///   — weapon-stance-authored, so an unarmed civilian walks with slightly
///   posed arms. That is the accepted trade-off against no walk animation
///   at all, the same class of call `sandbox_sit_enter_kf_path` documents
///   for its own single-clip Phase A.
pub fn humanoid_walk_kf_paths(game: GameKind) -> &'static [&'static str] {
    match game {
        GameKind::Fallout3NV => &[
            r"meshes\characters\_male\locomotion\male\mtforward.kf",
            r"meshes\characters\_male\locomotion\female\mtforward.kf",
            r"meshes\characters\_male\locomotion\child\mtforward.kf",
        ],
        GameKind::Oblivion => &[r"meshes\characters\_male\handtohandforward.kf"],
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield => &[],
    }
}

/// The humanoid walk-forward clip for one actor's `(game, gender,
/// is_child)` — the walk `npc_walk_animation_system` swaps in while an AI
/// package (or combat chase) moves the actor. `None` for Skyrim+ (Havok
/// `.hkx` track — see `asset_provider::populate_skyrim_walk_clip`). A
/// variant whose file turns out not to be archived still yields `Some`
/// here and `None` from the registry lookup — the actor simply spawns
/// without a walk clip, matching the idle path's graceful fallback.
pub fn humanoid_walk_kf_path(
    game: GameKind,
    gender: Gender,
    is_child: bool,
) -> Option<&'static str> {
    match game {
        GameKind::Fallout3NV => {
            if is_child {
                Some(r"meshes\characters\_male\locomotion\child\mtforward.kf")
            } else {
                match gender {
                    Gender::Female => {
                        Some(r"meshes\characters\_male\locomotion\female\mtforward.kf")
                    }
                    Gender::Male => {
                        Some(r"meshes\characters\_male\locomotion\male\mtforward.kf")
                    }
                }
            }
        }
        GameKind::Oblivion => Some(r"meshes\characters\_male\handtohandforward.kf"),
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield => None,
    }
}

/// A creature's own walk-forward clip candidates, beside its skeleton, first
/// that loads wins (#2567 convention — the MODL directory *is* the creature's
/// asset namespace; #6078 — the name is per game). Same census as
/// [`creature_idle_kf_paths`]: Oblivion `forward.kf` 36, `forwardwalk.kf` 3
/// (the Shivering Isles `grummite` / `elytra` / `gnarl`), `walkforward.kf` 1
/// (`hunger`); FNV `locomotion\mtforward.kf` 22, `mtforward.kf` 7, and the
/// shipped misspelling `locomotion\mtfoward.kf` 2 (`deathclaw`,
/// `nightstalker`); FO3 15 / 6 / 1 of the same three. A creature whose
/// directory has none gets no walk animation, which matches the idle path's
/// fallback posture.
pub fn creature_walk_kf_paths(game: GameKind, dir: &str) -> Vec<String> {
    let names: &[&str] = match game {
        GameKind::Oblivion => &["forward.kf", "forwardwalk.kf", "walkforward.kf"],
        GameKind::Fallout3NV => &[
            r"locomotion\mtforward.kf",
            "mtforward.kf",
            r"locomotion\mtfoward.kf",
        ],
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield => &[],
    };
    names.iter().map(|name| format!("{dir}{name}")).collect()
}

/// Warm the registry with every humanoid walk variant the game ships
/// (M42.10, per-gender since M42.11) so each spawn job's finalize path
/// lookup never pays BSA extract + parse on the spawn path. Path-keyed
/// memoised through [`load_kf_clip_by_path`]; a variant that isn't
/// archived logs at debug and is skipped. No-op for Havok-animation games
/// (Skyrim+/FO4+, which resolve their walk through `SkyrimWalkClip`).
pub fn load_walk_clips(world: &mut World, tex_provider: &TextureProvider, game: GameKind) {
    if !game.has_kf_animations() {
        return;
    }
    for kf_path in humanoid_walk_kf_paths(game) {
        if load_kf_clip_by_path(world, tex_provider, kf_path).is_none() {
            log::debug!("walk clip '{kf_path}' not archived — skipped");
        }
    }
}

/// Sanity bounds for an authored walk stride (world units/second). FNV's
/// humanoid `mtforward.kf` measures ~97 u/s of accum-root travel per
/// 1.2 s loop; a clip outside `[30, 250]` is either an in-place cycle
/// wearing an accum channel or a mis-derived root — either way the engine
/// default is the safer speed than trusting the measurement.
pub(super) const WALK_SPEED_MIN: f32 = 30.0;
pub(super) const WALK_SPEED_MAX: f32 = 250.0;

/// Derive an actor's [`crate::components::WalkSpeed`] from its walk clip's
/// accumulation-root travel (M42.11), falling back to the engine default
/// when the clip has no measurable stride. Reads the registry only — the
/// clip was registered moments earlier on this path.
pub(crate) fn walk_speed_for(world: &World, walk_handle: u32) -> f32 {
    world
        .resource::<AnimationClipRegistry>()
        .get(walk_handle)
        .and_then(|clip| clip.authored_horizontal_speed())
        .filter(|speed| (WALK_SPEED_MIN..=WALK_SPEED_MAX).contains(speed))
        .unwrap_or(crate::systems::LOCOMOTION_WALK_SPEED)
}

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
/// `chair_*`, `dlcanch*`, but no plain `idle.kf` base). Oblivion is the
/// opposite: no `locomotion\`, and a plain `_male\idle.kf` (#6100). Per-NPC
/// overrides from IDLE form records and AI packages slot in on top
/// once M42 / M47 land.
pub fn humanoid_default_idle_kf_path(game: GameKind) -> Option<&'static str> {
    match game {
        // Oblivion ships no `locomotion\` directory at all (BSA scan, zero hits
        // across the base archive and all 17 DLC / Knights archives, 2026-10-19);
        // its standing idle is `characters\_male\idle.kf` (#6100).
        GameKind::Oblivion => Some(r"meshes\characters\_male\idle.kf"),
        GameKind::Fallout3NV => Some(r"meshes\characters\_male\locomotion\mtidle.kf"),
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield => None,
    }
}

/// Speed jitter half-range for idle desync — NPC playback rate lands in
/// `[1 - IDLE_SPEED_JITTER, 1 + IDLE_SPEED_JITTER]`. Small enough to read
/// as natural variation, large enough that two NPCs on the same clip
/// visibly drift apart within a few seconds instead of breathing in
/// lockstep. `± 8 %` keeps the loop period within ~half a second of the
/// authored `mtidle.kf` cadence.
pub(super) const IDLE_SPEED_JITTER: f32 = 0.08;

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

/// An idle player on `clip` driving `skeleton`, started at the phase and speed
/// [`idle_desync`] gives `desync_key`. The one constructor every idle
/// (re-)insert goes through — spawn, the player body, the FO4 archetype idle
/// and [`stand_up`](crate::systems::revive::stand_up) — so a re-inserted idle
/// cannot drift from the spawned one (#6231).
pub(crate) fn desynced_idle_player(world: &World, clip: u32, desync_key: u32, skeleton: EntityId) -> AnimationPlayer {
    let duration = world
        .try_resource::<AnimationClipRegistry>()
        .and_then(|registry| registry.get(clip).map(|clip| clip.duration))
        .unwrap_or(0.0);
    let (start_time, speed) = idle_desync(desync_key, duration);
    let mut player = AnimationPlayer::new(clip).with_root(skeleton);
    player.local_time = start_time;
    player.prev_time = start_time;
    player.speed = speed;
    player
}

/// The idle an actor was spawned with: its own clip (a creature's beside its
/// skeleton, a humanoid's from the shared pool) and the key its phase was
/// desynced by. Getting up after a revive or a knockdown plays it again
/// (#6231) — a creature's rig binds none of the humanoid idle's bones.
/// `NOT_SAVED_BY_DESIGN`: spawn-derived, re-recorded when the actor respawns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpawnIdle {
    pub(crate) clip: u32,
    pub(crate) desync_key: u32,
}

impl byroredux_core::ecs::Component for SpawnIdle {
    type Storage = byroredux_core::ecs::SparseSetStorage<Self>;
}

/// Deterministically pick one idle-clip handle from the shared per-cell
/// pool for an NPC, keyed by its stable FormId. `None` when the pool is
/// empty (Havok-animation game, or the KF wasn't archived). The choice
/// is stable across loads and balanced across a >1 pool; with today's
/// size-1 pool it always resolves the single generic idle. Uses a hash
/// multiplier distinct from [`idle_desync`]'s so clip choice and start
/// phase don't correlate.
pub(super) fn pick_idle_handle(pool: &[u32], form_id: u32) -> Option<u32> {
    if pool.is_empty() {
        return None;
    }
    let h = (form_id as u64)
        .wrapping_mul(0xD6E8_FEB8_6659_FD93)
        .rotate_left(29);
    pool.get((h % pool.len() as u64) as usize).copied()
}
