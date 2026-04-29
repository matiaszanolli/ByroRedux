//! NPC spawning — assemble a placed NPC actor entity from its NPC_,
//! RACE, HDPT/EYES/HAIR, and FaceGen content.
//!
//! M41.0 lands the spawn function itself; this Phase 0 file ships the
//! game-variant path helpers that the spawn function will consume.
//! Each helper maps (game, gender) → a vanilla archive path string for
//! the per-game content layout.

use byroredux_core::ecs::components::{GlobalTransform, Name, Parent, Transform};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{NpcRecord, RaceRecord};
use byroredux_renderer::VulkanContext;

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
/// FNV / FO3 default idle clips are listed exhaustively in
/// `meshes\characters\_male\idleanims\` (962 entries on FNV vanilla);
/// `idle.kf` is the canonical resting-state base. Per-NPC overrides
/// from IDLE form records and AI packages slot in on top later.
// Consumed by Phase 2 (universal idle animation binding) — staged
// here in Phase 0/1 so the per-game path table stays in one place.
// Tests below pin every case so the API is locked-in by tests until
// the runtime caller lands.
#[allow(dead_code)]
pub fn humanoid_default_idle_kf_path(
    game: GameKind,
    _gender: Gender,
) -> Option<&'static str> {
    match game {
        GameKind::Oblivion | GameKind::Fallout3NV => {
            Some(r"meshes\characters\_male\idle.kf")
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
                );
                if let (Some(br), Some(sr)) = (body_root, skel_root) {
                    world.insert(br, Parent(sr));
                    add_child(world, sr, br);
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
                let (_head_count, head_root, _head_map) = load_nif_bytes_with_skeleton(
                    world,
                    ctx,
                    &head_data,
                    head_path.as_ref(),
                    tex_provider,
                    mat_provider,
                    Some(&skel_map),
                );
                if let (Some(hr), Some(sr)) = (head_root, skel_root) {
                    world.insert(hr, Parent(sr));
                    add_child(world, sr, hr);
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

    Some(placement_root)
}

#[cfg(test)]
mod tests {
    use super::*;

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
