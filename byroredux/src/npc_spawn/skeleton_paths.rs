//! Skeleton, body and creature mesh path resolution, plus the prebaked FaceGen sidecar paths.

use super::*;

/// Path inside the meshes archive for the default humanoid skeleton.
///
/// Race-specific skeletons take precedence in [`npc_skeleton_path`]; this
/// table supplies the game's shared humanoid fallback.
///
/// - **Oblivion / FO3 / FNV** use `meshes\characters\_male\skeleton.nif`.
///   Oblivion targets NPC spawning and shares this path with FO3/FNV; its
///   beast rig is handled separately (OBL-D3-01).
/// - **Skyrim** (LE/SE) uses
///   `meshes\actors\character\character assets\skeleton.nif`; the
///   `skeletonbeast.nif` sibling is Skyrim's Argonian/Khajiit variant.
/// - **FO4 / FO76** use `meshes\actors\character\characterassets\skeleton.nif`.
/// - **Starfield** uses `meshes\actors\human\characterassets\skeleton_facebones.nif`,
///   which includes the face joints required by NPC FaceGen parts.
pub fn humanoid_skeleton_path(game: GameKind) -> Option<&'static str> {
    match game {
        GameKind::Oblivion | GameKind::Fallout3NV => Some(r"meshes\characters\_male\skeleton.nif"),
        GameKind::Skyrim => Some(r"meshes\actors\character\character assets\skeleton.nif"),
        GameKind::Fallout4 | GameKind::Fallout76 => {
            Some(r"meshes\actors\character\characterassets\skeleton.nif")
        }
        // Starfield splits the human rig into a body-only `skeleton.nif`
        // and a complete `skeleton_facebones.nif`. NPC FaceGen heads, hair,
        // teeth, lashes, and brows skin to `faceBone_*` joints that only the
        // latter contains. The body bones are shared, so the complete rig is
        // the canonical runtime skeleton for every humanoid actor.
        GameKind::Starfield => Some(r"meshes\actors\human\characterassets\skeleton_facebones.nif"),
    }
}

/// Oblivion's beast rig: the humanoid skeleton plus the eight `Bip01 Tail01..08`
/// bones the Argonian and Khajiit tail meshes skin to. `skeleton.nif` carries none
/// of them (0 `Tail0*` names against this rig's 8), so a beast actor bound to
/// the plain humanoid rig could not pose a tail even if one were spawned (#5753).
pub const OBLIVION_BEAST_SKELETON_PATH: &str = r"meshes\characters\_male\skeletonbeast.nif";

/// The tail mesh an Oblivion race authors for `gender` (RACE body section, xEdit
/// "Tail Model"), normalised to the archive's `meshes\…` form; `None` for every
/// other game and for a race with no tail. Per gender because the section is: of
/// the shipped races only Argonian and Khajiit author one, each for both genders.
pub(crate) fn oblivion_race_tail_path(
    game: GameKind,
    race: Option<&RaceRecord>,
    gender: Gender,
) -> Option<String> {
    if game != GameKind::Oblivion {
        return None;
    }
    let slot = match gender {
        Gender::Male => 0,
        Gender::Female => 1,
    };
    let path = race?.tail_models[slot].as_str();
    (!path.is_empty()).then(|| normalize_mesh_path(path).into_owned())
}

/// The skeleton a KF-era humanoid actor of `race` / `gender` binds to: the game's
/// shared humanoid rig, except that an Oblivion race authoring a tail takes the
/// beast rig ([`OBLIVION_BEAST_SKELETON_PATH`]). A tail is the discriminator
/// because the RACE record carries no beast flag (Argonian and Khajiit author
/// DATA flags `0x1`, Playable only) — and a race that authors a tail is exactly
/// one that needs the tail bones.
pub(crate) fn humanoid_actor_skeleton_path(
    game: GameKind,
    race: Option<&RaceRecord>,
    gender: Gender,
) -> Option<String> {
    if oblivion_race_tail_path(game, race, gender).is_some() {
        return Some(OBLIVION_BEAST_SKELETON_PATH.to_owned());
    }
    humanoid_skeleton_path(game).map(str::to_owned)
}

/// Prefer the race's gender-specific skeleton over the game's humanoid
/// convention. The caller supplies the resolved traits record, so templated
/// creatures and female actors select the same race/gender as their skin.
pub(super) fn npc_skeleton_path(game: GameKind, traits: &NpcRecord, index: &EsmIndex) -> Option<String> {
    let gender = match Gender::from_acbs_flags(traits.acbs_flags) {
        Gender::Male => 0,
        Gender::Female => 1,
    };
    if let Some(path) = index
        .races
        .get(&traits.race_form_id)
        .map(|race| &race.skeleton_models[gender])
        .filter(|path| !path.is_empty())
    {
        let path = normalize_mesh_path(path).into_owned();
        if game == GameKind::Starfield {
            return Some(starfield_face_rig(path));
        }
        return Some(path);
    }
    humanoid_skeleton_path(game).map(str::to_owned)
}

/// Starfield's human RACE authors the body-only rig (`...\characterassets\
/// skeleton.nif`, and `...\female\skeleton.nif`), but NPC FaceGen heads, hair,
/// teeth, lashes and brows skin to `faceBone_*` joints only the complete
/// `skeleton_facebones.nif` beside it carries (see [`humanoid_skeleton_path`]).
/// Both human rigs ship that sibling in `Starfield - Meshes01/02.ba2`
/// (#6122); any other race's rig (ModelA, turrets, critters) has no face parts
/// and is left as authored.
pub(super) fn starfield_face_rig(path: String) -> String {
    let lower = path.to_ascii_lowercase().replace('/', "\\");
    let human_body_rig = [
        r"meshes\actors\human\characterassets\skeleton.nif",
        r"meshes\actors\human\characterassets\female\skeleton.nif",
    ];
    if human_body_rig.contains(&lower.as_str()) {
        let stem = path.len() - "skeleton.nif".len();
        return format!("{}skeleton_facebones.nif", &path[..stem]);
    }
    path
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

/// A creature's own idle clip candidates, beside its skeleton, first that
/// loads wins (#2567, #6078).
///
/// Creatures do not share the humanoid `idle.kf` — a rat's skeleton has
/// none of the humanoid bone names, so the shared per-cell idle pool
/// animates nothing. The file name is per game. Census of the creature
/// directories in every shipped mesh archive (base + DLC): Oblivion `idle.kf`
/// 43; FNV `mtidle.kf` 39, `idle.kf` 1 (`zaxeye`); FO3 `mtidle.kf` 24,
/// `idle.kf` 1. A creature whose directory has none simply gets no idle,
/// the pre-#2567 status quo rather than a regression. Empty for the HKX
/// games, which have no `CREA`.
pub fn creature_idle_kf_paths(game: GameKind, dir: &str) -> Vec<String> {
    let names: &[&str] = match game {
        GameKind::Oblivion => &["idle.kf"],
        GameKind::Fallout3NV => &["mtidle.kf", "idle.kf"],
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield => &[],
    };
    names.iter().map(|name| format!("{dir}{name}")).collect()
}

/// The FO3/FNV "Child" race translation: RACE DATA bit 2, gated on the
/// game because Oblivion reuses that bit for BeastRace. TES5 also defines
/// bit 2 as `Child` (xEdit `wbDefinitionsTES5.pas`; set on exactly the five
/// vanilla child races), but this helper feeds only the KF-era body-path and
/// walk-clip ladders, which Skyrim never takes (HKX walk, no humanoid body
/// table) — so Skyrim is excluded as "no consumer", not as "no flag". Admit
/// `GameKind::Skyrim` here when a Skyrim consumer of "is child" appears. One canonical
/// helper (#5079) — the NPC spawn path, the player-body walk-clip
/// resolution, and any future consumer must agree on whether a race is
/// child, or the player and NPCs of one race pick different body or walk
/// variants. `None` covers callers without a resolved RACE record.
pub fn is_child_race(game: GameKind, race_flags: Option<u32>) -> bool {
    matches!(game, GameKind::Fallout3NV) && race_flags.is_some_and(|flags| flags & 0x04 != 0)
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
        // #5487 — Oblivion ships exactly `upperbody / lowerbody / hand /
        // foot` (+ `female*` twins) under `characters\_male`, and does
        // NOT ship `lefthand.nif` / `righthand.nif` (those are FO3/FNV
        // names; the only `*hand*.nif` Oblivion ships are creature
        // parts). Pre-fix this arm shared the Fallout table, so the two
        // hand requests missed silently at `debug` and the lower body
        // and feet were never requested at all — most Oblivion
        // humanoids rendered handless, and partly-dressed ones legless.
        (GameKind::Oblivion, Gender::Female, _) => &[
            r"meshes\characters\_male\femaleupperbody.nif",
            r"meshes\characters\_male\femalelowerbody.nif",
            r"meshes\characters\_male\femalehand.nif",
            r"meshes\characters\_male\femalefoot.nif",
        ],
        (GameKind::Oblivion, Gender::Male, _) => &[
            r"meshes\characters\_male\upperbody.nif",
            r"meshes\characters\_male\lowerbody.nif",
            r"meshes\characters\_male\hand.nif",
            r"meshes\characters\_male\foot.nif",
        ],
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
        (GameKind::Fallout3NV, Gender::Female, _) => &[
            r"meshes\characters\_male\femaleupperbody.nif",
            r"meshes\characters\_male\femalelefthand.nif",
            r"meshes\characters\_male\femalerighthand.nif",
        ],
        (GameKind::Fallout3NV, Gender::Male, _) => &[
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
/// shared Hand bit for its single `hand.nif`, plus Lower Body (`0x08`)
/// and Foot (`0x20`) bits for the meshes its archive actually ships
/// (#5487 — pre-fix there was no arm that could request them).
pub(super) fn humanoid_body_path_biped_mask(game: GameKind, path: &str) -> u32 {
    if path.ends_with("upperbody.nif") {
        return 1 << 2;
    }
    match game {
        GameKind::Fallout3NV if path.ends_with("lefthand.nif") => 1 << 3,
        GameKind::Fallout3NV if path.ends_with("righthand.nif") => 1 << 4,
        GameKind::Oblivion if path.ends_with("hand.nif") => 1 << 4,
        GameKind::Oblivion if path.ends_with("lowerbody.nif") => 1 << 3,
        GameKind::Oblivion if path.ends_with("foot.nif") => 1 << 5,
        // #5753 — TES4 BMDT bit 15, the "Tail" slot armor can claim.
        GameKind::Oblivion if path.ends_with("tail.nif") => 1 << 15,
        _ => 0,
    }
}

/// #5487/#6090 — the RACE body-section `ICON` index a loose body mesh
/// takes its skin from. Oblivion: 0 UpperBody, 1 Leg, 2 Hand, 3 Foot,
/// 4 Tail (the tail mesh is the race's own, [`oblivion_race_tail_path`],
/// #5753). FO3/FNV author the same section under `NAM1` with a
/// three-slot vocabulary — 0 UpperBody, 1 Left Hand, 2 Right Hand
/// (`Fallout3.esm`: `Ghoul` -> `Characters\Ghoul\UpperBodyMale.dds` +
/// `HandMale.dds`; the `*Child`/`*Raider` races likewise) — and their
/// body NIFs bake the *default* skin (incl. `childupperbody.nif`
/// baking the adult male one), so without this arm every FO3/FNV
/// humanoid kept the generic skin.
pub(crate) fn humanoid_body_path_icon_index(game: GameKind, path: &str) -> Option<u32> {
    if game == GameKind::Fallout3NV {
        if path.ends_with("upperbody.nif") {
            // Covers the plain, `female…` and `child…` variants.
            Some(0)
        } else if path.ends_with("lefthand.nif") {
            Some(1)
        } else if path.ends_with("righthand.nif") {
            Some(2)
        } else {
            None
        }
    } else if game == GameKind::Oblivion {
        if path.ends_with("upperbody.nif") {
            Some(0)
        } else if path.ends_with("lowerbody.nif") {
            Some(1)
        } else if path.ends_with("hand.nif") {
            Some(2)
        } else if path.ends_with("foot.nif") {
            Some(3)
        } else if path.ends_with("tail.nif") {
            // Argonian `tail.nif` and Khajiit `khajiittail.nif`.
            Some(4)
        } else {
            None
        }
    } else {
        None
    }
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
