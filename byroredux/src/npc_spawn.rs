//! NPC spawning — assemble a placed NPC actor entity from its NPC_,
//! RACE, HDPT/EYES/HAIR, and FaceGen content.
//!
//! M41.0 lands the spawn function itself; this Phase 0 file ships the
//! game-variant path helpers that the spawn function will consume.
//! Each helper maps (game, gender) → a vanilla archive path string for
//! the per-game content layout.

// `#[allow(dead_code)]` is intentional for the duration of M41.0 Phase 0:
// these helpers are staged for the Phase 1 `spawn_npc_entity()` consumer
// (which lives in the same module). The tests below exercise every public
// item, so the API is locked-in by tests even before the runtime caller
// lands. The allow goes away when Phase 1 commits the consumer.
#![allow(dead_code)]

use byroredux_plugin::esm::reader::GameKind;

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
