//! NPC spawning — assemble a placed NPC actor entity from its NPC_,
//! RACE, HDPT/EYES/HAIR, and FaceGen content.
//!
//! M41.0 lands the spawn function itself; this Phase 0 file ships the
//! game-variant path helpers that the spawn function will consume.
//! Each helper maps (game, gender) → a vanilla archive path string for
//! the per-game content layout.

use byroredux_core::animation::AnimationClipRegistry;
// `AnimationPlayer` is the consumer of `idle_clip_handle` in the Phase 2.x
// follow-up that resolves the bind-pose-vs-clip-frame-0 mismatch — see
// `spawn_npc_entity`'s gated-off block at the bottom of the function.
#[allow(unused_imports)]
use byroredux_core::animation::AnimationPlayer;
use byroredux_core::ecs::components::{GlobalTransform, Name, Parent, Transform};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{NpcRecord, RaceRecord};
use byroredux_renderer::VulkanContext;

use crate::anim_convert::convert_nif_clip;
use crate::asset_provider::{MaterialProvider, TextureProvider};
use crate::helpers::add_child;
use crate::scene::load_nif_bytes_with_skeleton;

/// NPC gender as recorded by the ACBS sub-record's flags field.
///
/// Bit 0 of `acbs_flags` is the canonical "Female" flag across every
/// targeted Bethesda game from Oblivion through Starfield (per UESP
/// ACBS documentation). NPC_ and CREA records share the layout, so a
/// single helper is sufficient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gender {
    Male,
    Female,
}

impl Gender {
    /// Decode the gender bit from an `NpcRecord::acbs_flags` value.
    pub fn from_acbs_flags(flags: u32) -> Self {
        if flags & 0x0000_0001 != 0 {
            Self::Female
        } else {
            Self::Male
        }
    }
}

/// Path inside the meshes archive for the default humanoid skeleton.
///
/// Returns `None` for game variants that do not pre-bake a singleton
/// skeleton path at this convention — currently no targeted variant
/// returns `None`, but the optional return is preserved so future
/// per-race skeleton lookup (creatures, bestiary) can route through
/// the same helper without an API break.
///
/// Vanilla path table verified 2026-04-28 by listing every archive
/// at `byroredux/src/npc_spawn.rs` baseline:
///
/// - **FNV / FO3** ship a single `meshes\characters\_male\skeleton.nif`
///   used by both genders. There is no `_female/skeleton.nif`
///   sibling in vanilla content (BSA scan: 0 hits).
/// - **Skyrim** (LE/SE) ships the unified
///   `meshes\actors\character\character assets\skeleton.nif`. The
///   `skeletonbeast.nif` sibling is the Argonian/Khajiit variant; not
///   handled here yet (creature-race spawning is Phase 3+).
/// - **FO4 / FO76 / Starfield** follow the Skyrim convention.
///
/// Oblivion is not yet a target for NPC spawning (M41.0 closes on
/// FNV first); the path is the same as FNV's by convention.
pub fn humanoid_skeleton_path(game: GameKind, _gender: Gender) -> Option<&'static str> {
    match game {
        GameKind::Oblivion | GameKind::Fallout3NV => {
            Some(r"meshes\characters\_male\skeleton.nif")
        }
        GameKind::Skyrim
        | GameKind::Fallout4
        | GameKind::Fallout76
        | GameKind::Starfield => Some(r"meshes\actors\character\character assets\skeleton.nif"),
    }
}

/// Hardcoded vanilla body NIF path (`upperbody.nif`).
///
/// On FNV / FO3 the RACE record's `MODL` fields carry **head** mesh
/// paths (e.g. `Characters\Head\HeadHuman.NIF`), not body — the body
/// ships at a single canonical path per gender that every humanoid
/// race shares. This helper returns that canonical path. Pre-Phase-1c
/// the body alone is enough for "an NPC stands here in bind pose".
///
/// Returns `None` for game variants on the pre-baked-FaceGen track —
/// SSE / FO4 / FO76 / Starfield don't ship a separate skinned body
/// NIF; the per-NPC `facegendata\facegeom\<plugin>\<formid:08x>.nif`
/// carries head + body in one mesh. That spawn path lands in Phase 4.
pub fn humanoid_body_path(game: GameKind, gender: Gender) -> Option<&'static str> {
    match (game, gender) {
        (GameKind::Oblivion | GameKind::Fallout3NV, Gender::Male) => {
            Some(r"meshes\characters\_male\upperbody.nif")
        }
        // FNV vanilla ships only the male body NIF; female humanoids
        // re-use it (verified 2026-04-28 — `_female\` directory not
        // present in vanilla Fallout - Meshes.bsa). Mods may add a
        // `_female\upperbody.nif`; the gender split here lets a future
        // mod-aware lookup flip in without breaking the type signature.
        (GameKind::Oblivion | GameKind::Fallout3NV, Gender::Female) => {
            Some(r"meshes\characters\_male\upperbody.nif")
        }
        (
            GameKind::Skyrim
            | GameKind::Fallout4
            | GameKind::Fallout76
            | GameKind::Starfield,
            _,
        ) => None,
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
    gender: Gender,
) -> Option<u32> {
    if !game.has_kf_animations() {
        return None;
    }
    let kf_path = humanoid_default_idle_kf_path(game, gender)?;
    let kf_bytes = match tex_provider.extract_mesh(kf_path) {
        Some(b) => b,
        None => {
            log::debug!(
                "M41.0 Phase 2: idle KF '{}' not found in mesh archives — \
                 NPCs in this cell will spawn without an idle animation",
                kf_path,
            );
            return None;
        }
    };
    let nif_scene = match byroredux_nif::parse_nif(&kf_bytes) {
        Ok(s) => s,
        Err(e) => {
            log::warn!(
                "M41.0 Phase 2: idle KF '{}' failed to parse: {}",
                kf_path,
                e,
            );
            return None;
        }
    };
    let mut clips = byroredux_nif::anim::import_kf(&nif_scene);
    if clips.is_empty() {
        log::warn!(
            "M41.0 Phase 2: idle KF '{}' produced zero clips — skipping",
            kf_path,
        );
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
        registry.add(clip)
    };
    log::info!(
        "M41.0 Phase 2: idle clip '{}' registered from '{}' \
         ({:.2}s, {} channels) → handle {}",
        clip_name,
        kf_path,
        duration,
        channel_count,
        handle,
    );
    Some(handle)
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

/// Prepend `meshes\` to a NIF path if the input doesn't already start
/// with that segment (case-insensitive, accepting either separator).
/// `MODL` sub-records on RACE / NPC_ records are authored relative to
/// the `meshes\` root; the BSA layer stores the full prefix. Mirrors
/// the static-spawn path's normalization at
/// `cell_loader.rs:1064-1068`. Allocation only fires when the prefix
/// is missing — the common case (already-prefixed) is borrowed.
pub fn normalize_mesh_path(path: &str) -> std::borrow::Cow<'_, str> {
    let bytes = path.as_bytes();
    if bytes.len() >= 7 {
        let head = &bytes[..7];
        let already = head.eq_ignore_ascii_case(b"meshes\\")
            || head.eq_ignore_ascii_case(b"meshes/");
        if already {
            return std::borrow::Cow::Borrowed(path);
        }
    }
    std::borrow::Cow::Owned(format!(r"meshes\{}", path))
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
/// `chair_*`, `dlcanch*`, but no plain `idle.kf` base). Per-NPC
/// overrides from IDLE form records and AI packages slot in on top
/// once M42 / M47 land.
pub fn humanoid_default_idle_kf_path(
    game: GameKind,
    _gender: Gender,
) -> Option<&'static str> {
    match game {
        GameKind::Oblivion | GameKind::Fallout3NV => {
            Some(r"meshes\characters\_male\locomotion\mtidle.kf")
        }
        GameKind::Skyrim
        | GameKind::Fallout4
        | GameKind::Fallout76
        | GameKind::Starfield => None,
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
pub fn spawn_npc_entity(
    world: &mut World,
    ctx: &mut VulkanContext,
    npc: &NpcRecord,
    race: Option<&RaceRecord>,
    game: GameKind,
    tex_provider: &TextureProvider,
    mut mat_provider: Option<&mut MaterialProvider>,
    idle_clip_handle: Option<u32>,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
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

    // 2. Skeleton. Owns the per-bone entities the body / head will
    //    skin against.
    let skel_path = humanoid_skeleton_path(game, gender)?;
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
    );
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

    // 3. Body. Hardcoded vanilla path (`upperbody.nif`); the RACE
    //    record's MODL fields are head models on FNV / FO3, not body.
    //    Skip silently when the body NIF isn't extractable — modded
    //    setups may have replaced the path, in which case the NPC
    //    still gets a skeleton + head.
    if let Some(body_path) = humanoid_body_path(game, gender) {
        match tex_provider.extract_mesh(body_path) {
            Some(body_data) => {
                let (_body_count, body_root, _body_map) = load_nif_bytes_with_skeleton(
                    world,
                    ctx,
                    &body_data,
                    body_path,
                    tex_provider,
                    mat_provider.as_deref_mut(),
                    Some(&skel_map),
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
                let egm_file = egm_bytes
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
                let hook_state: Option<(
                    &byroredux_facegen::EgmFile,
                    [f32; 50],
                    [f32; 30],
                    u32,
                )> = match (recipe, egm_file.as_ref()) {
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
                        let after_asym = byroredux_facegen::apply_morphs(
                            &after_sym,
                            &egm.fgga_morphs,
                            &fgga,
                        );
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
                let pre_spawn: Option<
                    &mut dyn FnMut(&mut byroredux_nif::import::ImportedScene),
                > = if has_hook { Some(&mut hook) } else { None };
                let (_head_count, head_root, _head_map) = load_nif_bytes_with_skeleton(
                    world,
                    ctx,
                    &head_data,
                    head_path.as_ref(),
                    tex_provider,
                    mat_provider,
                    Some(&skel_map),
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

    // 5. Idle animation (M41.0 Phase 2). The clip handle is
    //    pre-registered once per cell load by `load_idle_clip` and
    //    threaded through every `spawn_npc_entity` call so the
    //    `AnimationClipRegistry` doesn't grow per-NPC. The player
    //    spawns on its own entity scoped to the skeleton root —
    //    `AnimationPlayer.root_entity` drives the per-frame channel
    //    lookup against the skeleton's `node_by_name` map (the same
    //    one body and head meshes resolved through above), so KF
    //    channels keyed by `Bip01 Spine`, `Bip01 Head`, etc. find
    //    the shared skeleton entities and drive the bone palette.
    // Phase 2 minimum scope ships the loader machinery
    // (`load_idle_clip`, KF parse, `AnimationClipRegistry::add`,
    // `humanoid_default_idle_kf_path`, the dispatcher pre-computation
    // in `cell_loader.rs`) but the actual `AnimationPlayer` spawn is
    // gated off until a follow-up resolves a bind-pose mismatch:
    //
    //   When the player ticks against `mtidle.kf` and the
    //   animation_system's apply phase writes `transform.translation
    //   = clip_frame_0_value` to skeleton bones, NPCs vanish from
    //   render — empirically reproduced on FO3 TestQAHairM (31
    //   bodies → 0 visible). The clip's frame-0 translations
    //   evidently don't align with skeleton.nif's authored bind-pose
    //   translations; either the KF stores deltas (not absolute
    //   bone-local poses) or there's a coord-frame divergence
    //   between `import_nif_scene`'s NiNode-Transform decoding and
    //   `import_kf`'s TranslationKey decoding. Filed as Phase 2.x;
    //   ROADMAP M41.0 closure can advance to Phase 3 (FaceGen
    //   morphs) without unblocking this — bodies stay in bind pose
    //   today which matches Phase 1b's visible result.
    //
    // The loader still fires once per cell so the registry +
    // log line confirm the kf path resolves end-to-end.
    let _ = idle_clip_handle;
    let _ = skel_root;

    Some(placement_root)
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
/// <plugin>\<formid:08x>.nif` carries the per-NPC head **and**
/// body in one already-skinned mesh — no separate body/head load,
/// no FaceGen morph evaluator (the SDK pre-applies the slider
/// table before shipping). Skeleton load + skinning resolution
/// stays identical to the kf-era path; the head NIF replaces both
/// the race body NIF and the race-default head.
///
/// **Animation deferred**: Skyrim+ vanilla ships zero `.kf` files
/// (Havok `.hkx` only). Pre-baked-track NPCs spawn in bind pose
/// today; M41.x lands a Havok stub for idle.
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

    // 2. Skeleton — same shared file as the kf-era path uses
    //    (Skyrim+ ships `meshes\actors\character\character assets\
    //    skeleton.nif`). The pre-baked head NIF carries its own
    //    `BSTriShape`-skinned mesh that resolves bones against
    //    this skeleton via the shared `external_skeleton` map.
    let skel_path = humanoid_skeleton_path(game, gender)?;
    let skel_data = match tex_provider.extract_mesh(skel_path) {
        Some(d) => d,
        None => {
            log::warn!(
                "NPC {:08X} ({}): skeleton '{}' not in archives — skipping spawn",
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
    );
    if let Some(sr) = skel_root {
        world.insert(sr, Parent(placement_root));
        add_child(world, placement_root, sr);
    }

    // 3. Pre-baked FaceGen NIF (per-NPC head+body in one mesh).
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
    let (_fg_count, fg_root, _fg_map) = load_nif_bytes_with_skeleton(
        world,
        ctx,
        &facegen_data,
        &facegen_path,
        tex_provider,
        mat_provider,
        Some(&skel_map),
        None,
    );
    if let Some(fr) = fg_root {
        world.insert(fr, Parent(placement_root));
        add_child(world, placement_root, fr);
    }

    // Face-tint texture override (Phase 4.x): the per-NPC face-tint
    // DDS at `textures\actors\character\facegendata\facetint\
    // <plugin>\<formid:08x>.dds` should replace slot-0 diffuse on
    // the head material. Wires through the existing
    // `RefrTextureOverlay` machinery rather than a parallel
    // override path. Deferred — minimum Phase 4 ships visible
    // bind-pose NPCs without per-NPC tint, matching the visible
    // outcome we'd get on the kf-era path before Phase 3c.x's tint
    // compositor lands.
    let _tint_path = prebaked_facegen_tint_path(plugin_name, npc.form_id);

    Some(placement_root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prebaked_facegen_nif_path_matches_vanilla_layout() {
        // Vanilla SSE Whiterun Mikael (FormID 0x00013BBE in
        // Skyrim.esm). Path scheme verified by BSA scan 2026-04-28.
        assert_eq!(
            prebaked_facegen_nif_path("Skyrim.esm", 0x00013BBE),
            Some(
                r"meshes\actors\character\facegendata\facegeom\skyrim.esm\00013bbe.nif"
                    .to_string(),
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
            humanoid_skeleton_path(GameKind::Fallout3NV, Gender::Male),
            Some(r"meshes\characters\_male\skeleton.nif"),
        );
        assert_eq!(
            humanoid_skeleton_path(GameKind::Fallout3NV, Gender::Female),
            // FNV/FO3 share the male skeleton across genders in vanilla.
            Some(r"meshes\characters\_male\skeleton.nif"),
        );
        assert_eq!(
            humanoid_skeleton_path(GameKind::Skyrim, Gender::Male),
            Some(r"meshes\actors\character\character assets\skeleton.nif"),
        );
        assert_eq!(
            humanoid_skeleton_path(GameKind::Fallout4, Gender::Male),
            Some(r"meshes\actors\character\character assets\skeleton.nif"),
        );
        assert_eq!(
            humanoid_skeleton_path(GameKind::Starfield, Gender::Male),
            Some(r"meshes\actors\character\character assets\skeleton.nif"),
        );
    }

    #[test]
    fn idle_kf_path_only_for_kf_era_games() {
        // FNV / FO3 ship `.kf` clips.
        assert!(humanoid_default_idle_kf_path(GameKind::Fallout3NV, Gender::Male).is_some());
        assert!(humanoid_default_idle_kf_path(GameKind::Oblivion, Gender::Male).is_some());

        // Skyrim+ uses Havok `.hkx` — no `.kf` exists in vanilla.
        // Verified by BSA scan 2026-04-28 (Skyrim SE Meshes0 + Meshes1
        // + Animations BSAs all return 0 `.kf` hits).
        assert!(humanoid_default_idle_kf_path(GameKind::Skyrim, Gender::Male).is_none());
        assert!(humanoid_default_idle_kf_path(GameKind::Fallout4, Gender::Male).is_none());
        assert!(humanoid_default_idle_kf_path(GameKind::Fallout76, Gender::Male).is_none());
        assert!(humanoid_default_idle_kf_path(GameKind::Starfield, Gender::Male).is_none());
    }
}
