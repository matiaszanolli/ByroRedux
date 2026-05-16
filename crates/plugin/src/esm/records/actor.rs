//! Actor-related record parsers — NPC_, RACE, CLAS, FACT.
//!
//! NPC parsing pulls the essentials needed to spawn the NPC into the world:
//! base race/class form IDs, faction memberships, inventory list, and a
//! pointer to the head/body model. Combat stats and AI packages are stored
//! as raw form IDs for now; full evaluation lands when the AI/combat
//! systems come online.

use super::common::{read_lstring_or_zstring, read_u32_at, read_zstring, CommonNamedFields};
use crate::esm::reader::{GameKind, SubRecord};

/// One faction the NPC belongs to, with their rank within it.
#[derive(Debug, Clone, Copy)]
pub struct FactionMembership {
    pub faction_form_id: u32,
    pub rank: i8,
}

/// One inventory entry on an NPC (`CNTO` sub-record).
#[derive(Debug, Clone, Copy)]
pub struct NpcInventoryEntry {
    pub item_form_id: u32,
    pub count: i32,
}

/// One face-morph entry on an FO4 NPC. Built from a paired
/// `FMRI` / `FMRS` sub-record sequence — they appear alternating on the
/// wire (I, S, I, S, …) and pair 1-to-1.
///
/// `setting` is 9 floats laid out as `position[3]`, `rotation[3]`,
/// `scale[3]` per the morph target identified by `form_id`. Verified
/// against vanilla `Fallout4.esm` named-NPC records (Hancock, Piper,
/// MQ101 player duplicates) — the audit's claim that FMRS is a single
/// `f32` slider was stale; FMRS payloads are 36 bytes everywhere they
/// appear. See #591 / FO4-DIM6-06.
#[derive(Debug, Clone, Copy)]
pub struct NpcFaceMorph {
    /// FMRI — morph-target FormID (HDPT or face-morph-data form).
    pub form_id: u32,
    /// FMRS — 9 floats: position[3], rotation[3], scale[3].
    pub setting: [f32; 9],
}

/// FO4 NPC face-morph block. Set on `NpcRecord` only when at least one
/// of the underlying sub-records was present — most generic settler
/// NPCs ship none and stay at `None`. See #591 / FO4-DIM6-06.
#[derive(Debug, Clone, Default)]
pub struct NpcFaceMorphs {
    /// Paired FMRI + FMRS entries. The wire format alternates the two
    /// sub-records; the parser pairs them positionally and truncates
    /// to the shorter of the two arrays if a record is malformed.
    pub morphs: Vec<NpcFaceMorph>,
    /// MSDK — slider key FormIDs (parallel to `slider_values`).
    pub slider_keys: Vec<u32>,
    /// MSDV — slider values (parallel to `slider_keys`).
    pub slider_values: Vec<f32>,
    /// QNAM — RGB texture-lighting tint + alpha (4 × f32). The trailing
    /// component on every vanilla FO4 record sampled was `1.0`; preserve
    /// it verbatim for round-trip rather than dropping it.
    pub texture_lighting: Option<[f32; 4]>,
    /// HCLF — hair-color FormID.
    pub hair_color: Option<u32>,
    /// BCLF — body-color override FormID. Rare on vanilla; preserved
    /// when present so future renderer work doesn't have to re-walk the
    /// record.
    pub body_color: Option<u32>,
    /// PNAM — head-part FormIDs (one FormID per sub-record, multiple).
    pub head_parts: Vec<u32>,
}

impl NpcFaceMorphs {
    fn is_empty(&self) -> bool {
        self.morphs.is_empty()
            && self.slider_keys.is_empty()
            && self.slider_values.is_empty()
            && self.texture_lighting.is_none()
            && self.hair_color.is_none()
            && self.body_color.is_none()
            && self.head_parts.is_empty()
    }
}

/// Pre-FO4 NPC FaceGen recipe — the slider-array form that the
/// legacy engine evaluates at load time against the race base head
/// NIF + its `.egm` (geometry) / `.egt` (texture) / `.tri` (animated
/// targets) sidecars. Carried by Oblivion / Fallout 3 / Fallout NV
/// NPCs; FO4+ uses the typed-morph-target form in [`NpcFaceMorphs`]
/// instead.
///
/// **None** of the floats are validated at parse time — slider arrays
/// in the wild include negative values (under-the-norm features) and
/// values > 1 (exaggerated features). The evaluator (Phase 3b) is
/// responsible for whatever clamping the renderer wants.
///
/// The `eyebrow_form_id` slot captures FNV's actual `PNAM` semantic
/// (a single eyebrow HDPT FormID). Pre-fix, FNV `PNAM` was being
/// accumulated into [`NpcFaceMorphs::head_parts`] (the FO4 semantic),
/// which silently misclassified every FNV named NPC as having FO4
/// face-morph data.
#[derive(Debug, Clone)]
pub struct NpcFaceGenRecipe {
    /// FGGS — 50 symmetric morph weights (left-right mirrored sliders
    /// like nose-bridge-width, jaw-depth, eye-vertical). Indexed by
    /// position in the 50-slot table; the slot semantics are baked
    /// into the race's `.egm` file. Most NPCs carry the full 50; the
    /// parser pads or truncates to 50 if the on-disk count differs.
    pub fggs: [f32; 50],
    /// FGGA — 30 asymmetric morph weights (left-only / right-only
    /// features that FGGS can't express). Same indexing scheme.
    pub fgga: [f32; 30],
    /// FGTS — 50 texture-morph weights driving complexion / age-line
    /// / makeup deltas via the race's `.egt` file. Applied in the
    /// face-tint compositor (Phase 3c).
    pub fgts: [f32; 50],
    /// HCLR — RGB hair color (3 bytes, `r/g/b`). Some FNV records
    /// carry a 4th byte (alpha or padding); per UESP only the first
    /// 3 are authoritative, so the parser drops the tail.
    pub hair_color_rgb: Option<[u8; 3]>,
    /// HNAM — hair style FormID (HAIR record).
    pub hair_form_id: Option<u32>,
    /// LNAM — unused on FNV / FO3 vanilla; preserved as opaque u32 so
    /// future authors can wire it without revisiting the parser.
    pub unused_lnam: Option<u32>,
    /// ENAM — eyes FormID (EYES record).
    pub eyes_form_id: Option<u32>,
    /// PNAM — eyebrow HDPT FormID. `None` when the record carries no
    /// PNAM. **Note:** FO4 reuses the `PNAM` tag for a head-parts
    /// list (multiple sub-records), so the FO4 path captures it in
    /// [`NpcFaceMorphs::head_parts`] instead.
    pub eyebrow_form_id: Option<u32>,
}

impl Default for NpcFaceGenRecipe {
    fn default() -> Self {
        // `[f32; 50]` and `[f32; 30]` have no built-in `Default` impl
        // (the std blanket only covers arrays up to length 32), so the
        // derive can't synthesise one. Hand-roll it; all-zeros matches
        // the slider-array zero-default the legacy engine assumes for
        // any NPC that doesn't override a slot.
        Self {
            fggs: [0.0; 50],
            fgga: [0.0; 30],
            fgts: [0.0; 50],
            hair_color_rgb: None,
            hair_form_id: None,
            unused_lnam: None,
            eyes_form_id: None,
            eyebrow_form_id: None,
        }
    }
}

impl NpcFaceGenRecipe {
    fn is_empty(&self) -> bool {
        self.fggs.iter().all(|f| *f == 0.0)
            && self.fgga.iter().all(|f| *f == 0.0)
            && self.fgts.iter().all(|f| *f == 0.0)
            && self.hair_color_rgb.is_none()
            && self.hair_form_id.is_none()
            && self.unused_lnam.is_none()
            && self.eyes_form_id.is_none()
            && self.eyebrow_form_id.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct NpcRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Model path (typically from MODL — head/body mesh, optional).
    pub model_path: String,
    /// Race form ID (RNAM).
    pub race_form_id: u32,
    /// Class form ID (CNAM).
    pub class_form_id: u32,
    /// Voice type form ID.
    pub voice_form_id: u32,
    /// Faction memberships (`SNAM` sub-records).
    pub factions: Vec<FactionMembership>,
    /// Inventory list (`CNTO` sub-records).
    pub inventory: Vec<NpcInventoryEntry>,
    /// Default outfit FormID (Skyrim+ `DOFT`). Resolves to an `OTFT`
    /// record whose `INAM` array names the actor's default-equipped
    /// armor pieces. `None` on FO3 / FNV / Oblivion (those games
    /// equip from the inventory list directly). `None` on Skyrim+
    /// when the NPC ships no DOFT — generic settlers etc.
    pub default_outfit: Option<u32>,
    /// AI packages (`PKID` sub-records, in priority order).
    pub ai_packages: Vec<u32>,
    /// Death item leveled list (DEST in some games, INAM in others).
    pub death_item_form_id: u32,
    /// Base level (from DATA).
    pub level: i16,
    /// Disposition base (from ACBS — i16 at offset 20). FNV vanilla
    /// default is 50; values are signed so unfriendly NPCs can sit
    /// below 0. Reading the high byte was being dropped pre-#377,
    /// silently truncating any disposition outside 0..=127.
    pub disposition_base: i16,
    /// Flags (from ACBS).
    pub acbs_flags: u32,
    /// True when the NPC record carries a `VMAD` sub-record (Skyrim+
    /// Papyrus VM attached-script blob). Presence flag only; full
    /// decoding deferred to scripting-as-ECS work. See #369.
    pub has_script: bool,
    /// FO4 face-morph block (FMRI/FMRS/MSDK/MSDV/QNAM/HCLF/BCLF/PNAM).
    /// `None` when the record carries no face-morph sub-records (most
    /// pre-FO4 NPCs and FO4 generic settlers). Driven by audit
    /// FO4-DIM6-06 / #591 — actual morph-target application is
    /// downstream of HDPT mesh linking + the skinning pipeline.
    ///
    /// **Parse-but-don't-consume gate (TD5-013):** gated on M41.0.5
    /// (GPU per-vertex morph runtime, Tier 5). The sibling field
    /// `runtime_facegen` IS consumed in `npc_spawn.rs:619` (M41.0
    /// Phase 3b); `face_morphs` unlocks when the `.tri`-morph weight
    /// application pass lands.
    pub face_morphs: Option<NpcFaceMorphs>,
    /// Pre-FO4 FaceGen recipe (FGGS/FGGA/FGTS slider arrays + HCLR /
    /// HNAM / LNAM / ENAM / PNAM). `None` when the record carries
    /// none of those sub-records. Mutually exclusive with
    /// [`face_morphs`] in vanilla content (no record carries both
    /// forms — they're per-game). M41.0 Phase 3b consumes the slider
    /// arrays against the race's `.egm` sidecar to deform the base
    /// head mesh per NPC.
    pub runtime_facegen: Option<NpcFaceGenRecipe>,
}

#[derive(Debug, Clone)]
pub struct RaceRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// Skill bonuses: pairs of (skill AVIF form, bonus value).
    pub skill_bonuses: Vec<(u32, i8)>,
    /// Body part model paths (head, body, hand, foot).
    pub body_models: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ClassRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// 7 attribute weights (Strength, Perception, Endurance, Charisma,
    /// Intelligence, Agility, Luck) — order varies per game.
    pub attribute_weights: [u8; 7],
    /// Tag skill form IDs from DATA.
    pub tag_skills: Vec<u32>,
}

/// Faction-to-faction relation.
#[derive(Debug, Clone, Copy)]
pub struct FactionRelation {
    pub other_faction: u32,
    /// Modifier (-100..100, larger means more friendly).
    pub modifier: i32,
    /// Combat reaction (0=neutral, 1=enemy, 2=ally, 3=friend).
    pub combat_reaction: u8,
}

#[derive(Debug, Clone)]
pub struct FactionRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Hidden flag etc. (from DATA).
    pub flags: u32,
    pub relations: Vec<FactionRelation>,
    /// Rank index → label.
    pub ranks: Vec<String>,
}

// ── Parsers ───────────────────────────────────────────────────────────

pub fn parse_npc(form_id: u32, subs: &[SubRecord], game: GameKind) -> NpcRecord {
    // The FMRI/FMRS/MSDK/MSDV/QNAM/HCLF/BCLF face-morph block was
    // introduced in FO4. FNV/FO3/Skyrim NPCs ship none of those
    // sub-records — and crucially, FNV `PNAM` carries a single
    // eyebrow HDPT FormID, NOT a head-part list, so accumulating it
    // into `face.head_parts` (the FO4 semantic) was a silent
    // mis-classification pre-fix. The two paths are mutually
    // exclusive in vanilla content; both arms key off `GameKind`
    // semantic predicates so adding new games extends the table at
    // one site.
    let captures_fo4_face = game.uses_prebaked_facegen();
    let captures_runtime_facegen = game.has_runtime_facegen_recipe();
    // EDID / FULL / MODL / VMAD shared with every named record — drain
    // them through the helper so the per-record loop below only carries
    // NPC-specific subrecords. TD3-203 / #1113.
    let common = CommonNamedFields::from_subs(subs);
    let mut record = NpcRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        model_path: common.model_path,
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
        has_script: common.has_script,
        face_morphs: None,
        runtime_facegen: None,
    };
    // FMRI and FMRS are collected separately and zipped after the walk
    // since they appear alternating on the wire and we don't want to
    // assume a strict ordering inside the sub-record list.
    let mut fmri_forms: Vec<u32> = Vec::new();
    let mut fmrs_settings: Vec<[f32; 9]> = Vec::new();
    let mut face = NpcFaceMorphs::default();
    let mut recipe = NpcFaceGenRecipe::default();

    for sub in subs {
        match &sub.sub_type {
            b"RNAM" if sub.data.len() >= 4 => {
                record.race_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            b"CNAM" if sub.data.len() >= 4 => {
                record.class_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            b"VTCK" if sub.data.len() >= 4 => {
                record.voice_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            // SNAM (FNV NPC_): faction form ID (u32) + rank (i8) + pad x3
            b"SNAM" if sub.data.len() >= 8 => {
                let faction = read_u32_at(&sub.data, 0).unwrap_or(0);
                let rank = sub.data[4] as i8;
                record.factions.push(FactionMembership {
                    faction_form_id: faction,
                    rank,
                });
            }
            // CNTO: shared with CONT
            b"CNTO" if sub.data.len() >= 8 => {
                record.inventory.push(NpcInventoryEntry {
                    item_form_id: read_u32_at(&sub.data, 0).unwrap_or(0),
                    count: i32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]),
                });
            }
            b"PKID" if sub.data.len() >= 4 => {
                record
                    .ai_packages
                    .push(read_u32_at(&sub.data, 0).unwrap_or(0));
            }
            // DOFT — Skyrim+ default outfit FormID. Pre-Skyrim games
            // don't emit DOFT (NPCs equip directly from inventory).
            // Stored as Option so the equip pipeline can dispatch on
            // presence without ambiguity vs the null-form sentinel.
            b"DOFT" if sub.data.len() >= 4 => {
                record.default_outfit = read_u32_at(&sub.data, 0);
            }
            b"INAM" if sub.data.len() >= 4 => {
                record.death_item_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            // ACBS (FNV NPC_): flags(u32), fatigue(u16), barter(u16), level(i16),
            // calc_min(u16), calc_max(u16), speed_mult(u16), karma(f32),
            // disposition_base(i16), template_flags(u16)
            b"ACBS" if sub.data.len() >= 24 => {
                record.acbs_flags = read_u32_at(&sub.data, 0).unwrap_or(0);
                record.level = i16::from_le_bytes([sub.data[8], sub.data[9]]);
                // disposition_base is i16 at offset 20 (per UESP /
                // FalloutSnip). Pre-#377 the parser read a single byte
                // here, so any value outside 0..=127 lost its high byte
                // (and signed values past -128 had the sign chopped).
                if sub.data.len() >= 22 {
                    record.disposition_base = i16::from_le_bytes([sub.data[20], sub.data[21]]);
                }
            }
            // ── M41.0 Phase 1a — Pre-FO4 FaceGen recipe ────────────────
            // FGGS (50 × f32 sym morph weights), FGGA (30 × f32 asym),
            // FGTS (50 × f32 texture morphs). Vanilla bytes are exactly
            // the documented sizes; the parser pads short payloads with
            // zeros and truncates over-long ones rather than panicking.
            b"FGGS" if captures_runtime_facegen && !sub.data.is_empty() => {
                read_f32_array_into(&sub.data, &mut recipe.fggs);
            }
            b"FGGA" if captures_runtime_facegen && !sub.data.is_empty() => {
                read_f32_array_into(&sub.data, &mut recipe.fgga);
            }
            b"FGTS" if captures_runtime_facegen && !sub.data.is_empty() => {
                read_f32_array_into(&sub.data, &mut recipe.fgts);
            }
            // HCLR carries 3-byte RGB on FNV vanilla; some records ship
            // a 4th alpha/padding byte — drop it per UESP (only the
            // first 3 are authoritative).
            b"HCLR" if captures_runtime_facegen && sub.data.len() >= 3 => {
                recipe.hair_color_rgb = Some([sub.data[0], sub.data[1], sub.data[2]]);
            }
            b"HNAM" if captures_runtime_facegen && sub.data.len() >= 4 => {
                recipe.hair_form_id = Some(read_u32_at(&sub.data, 0).unwrap_or(0));
            }
            b"LNAM" if captures_runtime_facegen && sub.data.len() >= 4 => {
                recipe.unused_lnam = Some(read_u32_at(&sub.data, 0).unwrap_or(0));
            }
            b"ENAM" if captures_runtime_facegen && sub.data.len() >= 4 => {
                recipe.eyes_form_id = Some(read_u32_at(&sub.data, 0).unwrap_or(0));
            }
            // FNV / FO3 PNAM = single eyebrow HDPT FormID. The FO4 PNAM
            // arm below carries a different semantic (head-parts list);
            // the two are guarded by `captures_runtime_facegen` vs
            // `captures_fo4_face` and never both fire on a single record.
            b"PNAM" if captures_runtime_facegen && sub.data.len() >= 4 => {
                recipe.eyebrow_form_id = Some(read_u32_at(&sub.data, 0).unwrap_or(0));
            }
            // ── #591 / FO4-DIM6-06 face-morph block ────────────────────
            // FO4+/FO76/Starfield only. Pre-fix, all of these arms ran
            // unconditionally; FNV `PNAM` (eyebrow HDPT FormID) was
            // misread as an FO4 head-parts entry. See `captures_fo4_face`.
            b"FMRI" if captures_fo4_face && sub.data.len() >= 4 => {
                fmri_forms.push(read_u32_at(&sub.data, 0).unwrap_or(0));
            }
            b"FMRS" if captures_fo4_face && sub.data.len() >= 36 => {
                let mut s = [0f32; 9];
                for (i, slot) in s.iter_mut().enumerate() {
                    let off = i * 4;
                    *slot = f32::from_le_bytes([
                        sub.data[off],
                        sub.data[off + 1],
                        sub.data[off + 2],
                        sub.data[off + 3],
                    ]);
                }
                fmrs_settings.push(s);
            }
            // MSDK / MSDV are parallel arrays of u32 / f32 entries; on
            // vanilla FO4 they're single sub-records carrying the full
            // table. Reading them as variable-length flat arrays is
            // forward-compatible with malformed records that split the
            // table across multiple sub-records (last-wins per arm
            // would silently drop earlier entries — `extend` preserves).
            b"MSDK" if captures_fo4_face && sub.data.len() >= 4 => {
                for chunk in sub.data.chunks_exact(4) {
                    face.slider_keys
                        .push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }
            }
            b"MSDV" if captures_fo4_face && sub.data.len() >= 4 => {
                for chunk in sub.data.chunks_exact(4) {
                    face.slider_values
                        .push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }
            }
            // QNAM (FO4): 4 × f32 = texture-lighting tint (RGB + alpha).
            // The `captures_fo4_face` gate replaces the previous
            // length-only `>= 16` heuristic — Skyrim WTHR-record
            // siblings sharing the QNAM tag never reach this parser.
            b"QNAM" if captures_fo4_face && sub.data.len() >= 16 => {
                let mut t = [0f32; 4];
                for (i, slot) in t.iter_mut().enumerate() {
                    let off = i * 4;
                    *slot = f32::from_le_bytes([
                        sub.data[off],
                        sub.data[off + 1],
                        sub.data[off + 2],
                        sub.data[off + 3],
                    ]);
                }
                face.texture_lighting = Some(t);
            }
            b"HCLF" if captures_fo4_face && sub.data.len() >= 4 => {
                face.hair_color = Some(read_u32_at(&sub.data, 0).unwrap_or(0));
            }
            b"BCLF" if captures_fo4_face && sub.data.len() >= 4 => {
                face.body_color = Some(read_u32_at(&sub.data, 0).unwrap_or(0));
            }
            // PNAM on FO4+ NPCs accumulates head-part FormIDs (one per
            // sub-record). FNV / FO3 PNAM is captured by the kf-era
            // arm above as a single eyebrow HDPT FormID; the two arms
            // are mutually exclusive via `captures_fo4_face` vs
            // `captures_runtime_facegen`, both keyed off `GameKind`
            // semantic predicates.
            b"PNAM" if captures_fo4_face && sub.data.len() >= 4 => {
                face.head_parts.push(read_u32_at(&sub.data, 0).unwrap_or(0));
            }
            _ => {}
        }
    }

    // Pair FMRI + FMRS positionally. Truncation to the shorter length
    // is the defensive choice — Bethesda's authoring tool emits paired
    // sub-records, but a malformed mod could ship one without the
    // other, and silently dropping the unpaired tail is preferable to
    // panicking the cell load.
    let pair_count = fmri_forms.len().min(fmrs_settings.len());
    if fmri_forms.len() != fmrs_settings.len() {
        log::debug!(
            "NPC {form_id:08X}: FMRI/FMRS count mismatch ({} vs {}); pairing first {}",
            fmri_forms.len(),
            fmrs_settings.len(),
            pair_count,
        );
    }
    // MILESTONE: M41.0.5 (per-vertex morph runtime) — see #1057.
    // FMRI + FMRS pairs decoded here populate `face_morphs.morphs`
    // (typed-morph-target form). `byroredux/src/npc_spawn.rs` ignores
    // the array today; FaceGen Phase 4 (#794 family) only consumed the
    // `runtime_facegen` recipe path. Wire when the per-vertex morph
    // GPU runtime lands.
    for i in 0..pair_count {
        face.morphs.push(NpcFaceMorph {
            form_id: fmri_forms[i],
            setting: fmrs_settings[i],
        });
    }

    if !face.is_empty() {
        record.face_morphs = Some(face);
    }
    if !recipe.is_empty() {
        record.runtime_facegen = Some(recipe);
    }

    record
}

/// Read up to `dst.len()` consecutive `f32` values out of `src` into
/// `dst`, padding with zero on under-read and silently dropping any
/// over-read tail. Used by [`parse_npc`] to land FGGS/FGGA/FGTS
/// payloads against fixed-size slider arrays.
fn read_f32_array_into(src: &[u8], dst: &mut [f32]) {
    for (i, slot) in dst.iter_mut().enumerate() {
        let off = i * 4;
        if off + 4 <= src.len() {
            *slot = f32::from_le_bytes([src[off], src[off + 1], src[off + 2], src[off + 3]]);
        } else {
            *slot = 0.0;
        }
    }
}

pub fn parse_race(form_id: u32, subs: &[SubRecord]) -> RaceRecord {
    // Helper claims a single MODL; RACE records carry multiple body
    // parts in MODL — keep that arm custom and ignore the helper's
    // last-MODL string. TD3-203 / #1113.
    let common = CommonNamedFields::from_subs(subs);
    let mut record = RaceRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        description: String::new(),
        skill_bonuses: Vec::new(),
        body_models: Vec::new(),
    };

    for sub in subs {
        match &sub.sub_type {
            b"DESC" => record.description = read_lstring_or_zstring(&sub.data),
            // DATA (FNV RACE): skill bonus pairs (u32 form + i8) ×7, then more.
            // We pull the first 7 pairs.
            b"DATA" => {
                let pair_size = 5; // u32 form_id + i8 bonus
                for i in 0..7 {
                    let off = i * pair_size;
                    if sub.data.len() < off + pair_size {
                        break;
                    }
                    let f = read_u32_at(&sub.data, off).unwrap_or(0);
                    let bonus = sub.data[off + 4] as i8;
                    if f != 0 {
                        record.skill_bonuses.push((f, bonus));
                    }
                }
            }
            // MODL appears multiple times in RACE for body parts. Collect them all.
            b"MODL" => record.body_models.push(read_zstring(&sub.data)),
            _ => {}
        }
    }

    record
}

pub fn parse_clas(form_id: u32, subs: &[SubRecord]) -> ClassRecord {
    let common = CommonNamedFields::from_subs(subs);
    let mut record = ClassRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        description: String::new(),
        attribute_weights: [0u8; 7],
        tag_skills: Vec::new(),
    };

    for sub in subs {
        match &sub.sub_type {
            b"DESC" => record.description = read_lstring_or_zstring(&sub.data),
            // DATA layout (FNV CLAS): tag1..tag4 (4 × u32 form), flags (u32),
            // services (u32), trainer skill (i8), trainer level (u8),
            // teaches level (u8), teaches max (u8), then 7 attribute weights (u8).
            // 4*4 + 4 + 4 + 4 + 7 = 35 bytes.
            b"DATA" if sub.data.len() >= 35 => {
                for i in 0..4 {
                    let off = i * 4;
                    if let Some(f) = read_u32_at(&sub.data, off) {
                        if f != 0 {
                            record.tag_skills.push(f);
                        }
                    }
                }
                // Skip flags + services + skill/level/teaches bytes (16 + 4 = 20).
                // Attribute weights start at offset 28.
                for i in 0..7 {
                    record.attribute_weights[i] = sub.data.get(28 + i).copied().unwrap_or(0);
                }
            }
            _ => {}
        }
    }

    record
}

pub fn parse_fact(form_id: u32, subs: &[SubRecord]) -> FactionRecord {
    let common = CommonNamedFields::from_subs(subs);
    let mut record = FactionRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        flags: 0,
        relations: Vec::new(),
        ranks: Vec::new(),
    };

    for sub in subs {
        match &sub.sub_type {
            // DATA (FNV FACT): flags is a single byte per UESP
            // `Mod_File_Format/FACT` (FO3 / FNV). The tail is a
            // variable-width payload (FNV adds `u8 unknown + f32 crime
            // gold multiplier`) that different vanilla records truncate
            // differently — reading 4 bytes pulled padding / neighbor
            // bytes into the high 24 bits, producing spurious bits 8+.
            // Only bits 0 (hidden from PC), 1 (evil), 2 (special
            // combat) are authoritative on FO3 / FNV.
            //
            // Skyrim and FO4 extend DATA to a full u32; if / when those
            // parse paths get added here, split per `GameKind`. See
            // #481 / FNV-2-L1.
            b"DATA" if !sub.data.is_empty() => {
                record.flags = sub.data[0] as u32;
            }
            // XNAM: relation entry — other faction (u32) + modifier (i32) + reaction (u32).
            // The reaction field is a full 4-byte u32 per UESP; pre-#482 the
            // parser read only the low byte via `sub.data[8]`, which happened
            // to be correct for vanilla values 0..=3 but would silently
            // truncate any future mod that extends the enum past 255.
            b"XNAM" if sub.data.len() >= 8 => {
                let other = read_u32_at(&sub.data, 0).unwrap_or(0);
                let modifier =
                    i32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]);
                let combat = if sub.data.len() >= 12 {
                    read_u32_at(&sub.data, 8).unwrap_or(0) as u8
                } else {
                    0
                };
                record.relations.push(FactionRelation {
                    other_faction: other,
                    modifier,
                    combat_reaction: combat,
                });
            }
            // MNAM: male rank label (string)
            b"MNAM" => record.ranks.push(read_zstring(&sub.data)),
            _ => {}
        }
    }

    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::reader::SubRecord;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    #[test]
    fn npc_extracts_race_class_factions_inventory() {
        let mut acbs = Vec::new();
        acbs.extend_from_slice(&0x100u32.to_le_bytes()); // flags
        acbs.extend_from_slice(&[0u8; 4]); // fatigue + barter
        acbs.extend_from_slice(&5i16.to_le_bytes()); // level
        acbs.extend_from_slice(&[0u8; 14]); // pad to 24 bytes total

        let mut snam = Vec::new();
        snam.extend_from_slice(&0xAAAAu32.to_le_bytes());
        snam.push(2u8);
        snam.extend_from_slice(&[0u8; 3]);

        let mut cnto = Vec::new();
        cnto.extend_from_slice(&0xBBBBu32.to_le_bytes());
        cnto.extend_from_slice(&3i32.to_le_bytes());

        let subs = vec![
            sub(b"EDID", b"NpcTest\0"),
            sub(b"FULL", b"Test NPC\0"),
            sub(b"RNAM", &0xCCCCu32.to_le_bytes()),
            sub(b"CNAM", &0xDDDDu32.to_le_bytes()),
            sub(b"ACBS", &acbs),
            sub(b"SNAM", &snam),
            sub(b"CNTO", &cnto),
            sub(b"PKID", &0xEEEEu32.to_le_bytes()),
        ];
        let n = parse_npc(0x500, &subs, GameKind::Fallout3NV);
        assert_eq!(n.editor_id, "NpcTest");
        assert_eq!(n.race_form_id, 0xCCCC);
        assert_eq!(n.class_form_id, 0xDDDD);
        assert_eq!(n.factions.len(), 1);
        assert_eq!(n.factions[0].faction_form_id, 0xAAAA);
        assert_eq!(n.factions[0].rank, 2);
        assert_eq!(n.inventory.len(), 1);
        assert_eq!(n.inventory[0].item_form_id, 0xBBBB);
        assert_eq!(n.inventory[0].count, 3);
        assert_eq!(n.ai_packages, vec![0xEEEE]);
        assert_eq!(n.acbs_flags, 0x100);
        assert_eq!(n.level, 5);
    }

    /// Regression for #377 (FNV F2-03): ACBS `disposition_base` is an
    /// i16 at offset 20, not a u8. Pre-fix the parser pulled
    /// `sub.data[20]` (one byte), so values outside 0..=127 got their
    /// high byte dropped and the sign destroyed. Verify both a negative
    /// disposition (Raider-tier) and a positive value > 127 round-trip.
    #[test]
    fn npc_acbs_disposition_base_reads_signed_i16() {
        // ACBS layout (FNV NPC_, 24 bytes): flags u32, fatigue u16,
        // barter u16, level i16, calc_min u16, calc_max u16, speed_mult
        // u16, karma f32, disposition_base i16, template_flags u16.
        fn acbs_with_disposition(d: i16) -> Vec<u8> {
            let mut a = Vec::with_capacity(24);
            a.extend_from_slice(&0u32.to_le_bytes()); // flags
            a.extend_from_slice(&[0u8; 4]); // fatigue + barter
            a.extend_from_slice(&1i16.to_le_bytes()); // level
            a.extend_from_slice(&[0u8; 10]); // calc_min + calc_max + speed_mult + karma
            a.extend_from_slice(&d.to_le_bytes()); // disposition_base
            a.extend_from_slice(&0u16.to_le_bytes()); // template_flags
            a
        }

        let neg = parse_npc(
            0x700,
            &[
                sub(b"EDID", b"Raider\0"),
                sub(b"ACBS", &acbs_with_disposition(-40)),
            ],
            GameKind::Fallout3NV,
        );
        assert_eq!(
            neg.disposition_base, -40,
            "negative disposition must keep its sign"
        );

        let high = parse_npc(
            0x701,
            &[
                sub(b"EDID", b"Friendly\0"),
                sub(b"ACBS", &acbs_with_disposition(200)),
            ],
            GameKind::Fallout3NV,
        );
        assert_eq!(
            high.disposition_base, 200,
            "values > 127 must not lose the high byte"
        );
    }

    #[test]
    fn npc_vmad_flips_has_script() {
        // Regression: #369 — Skyrim NPCs with attached Papyrus scripts
        // were not discoverable. The presence-only `has_script` flag
        // is the audit's minimum-viable signal.
        let subs = vec![
            sub(b"EDID", b"ScriptedActor\0"),
            sub(b"VMAD", b"\x05\x00\x02\x00\x00\x00"),
        ];
        let n = parse_npc(0x501, &subs, GameKind::Skyrim);
        assert!(n.has_script);
    }

    #[test]
    fn npc_without_vmad_has_script_false() {
        // Sibling check — bare NPC must keep has_script at default.
        let subs = vec![sub(b"EDID", b"PlainActor\0")];
        let n = parse_npc(0x502, &subs, GameKind::Fallout3NV);
        assert!(!n.has_script);
    }

    #[test]
    fn fact_extracts_relations_and_ranks() {
        let mut xnam = Vec::new();
        xnam.extend_from_slice(&0x123u32.to_le_bytes());
        xnam.extend_from_slice(&(-50i32).to_le_bytes());
        xnam.extend_from_slice(&1u32.to_le_bytes()); // combat reaction = enemy

        let subs = vec![
            sub(b"EDID", b"NCR\0"),
            sub(b"FULL", b"NCR\0"),
            sub(b"DATA", &0x01u32.to_le_bytes()),
            sub(b"XNAM", &xnam),
            sub(b"MNAM", b"Recruit\0"),
            sub(b"MNAM", b"Trooper\0"),
            sub(b"MNAM", b"Veteran\0"),
        ];
        let f = parse_fact(0x42, &subs);
        assert_eq!(f.editor_id, "NCR");
        assert_eq!(f.flags, 0x01);
        assert_eq!(f.relations.len(), 1);
        assert_eq!(f.relations[0].other_faction, 0x123);
        assert_eq!(f.relations[0].modifier, -50);
        assert_eq!(f.relations[0].combat_reaction, 1);
        assert_eq!(f.ranks, vec!["Recruit", "Trooper", "Veteran"]);
    }

    /// Regression for #482: the reaction field is a 4-byte u32 per
    /// UESP spec, not a single byte. A typical u32 like `0x00000002`
    /// (ally) must round-trip through the parser correctly — this is
    /// the minimal "parser reads the right field width" check.
    ///
    /// Pre-fix the parser read only `sub.data[8]` (the low byte). For
    /// vanilla values 0..=3 the low byte happens to equal the full
    /// value, so the test passes with the old code too — its job is
    /// to document the spec and catch a future regression that goes
    /// back to byte access.
    #[test]
    fn fact_xnam_combat_reaction_reads_full_u32() {
        let mut xnam = Vec::new();
        xnam.extend_from_slice(&0x999u32.to_le_bytes()); // other faction
        xnam.extend_from_slice(&0i32.to_le_bytes()); // modifier
        xnam.extend_from_slice(&2u32.to_le_bytes()); // combat reaction = ally (full 4 bytes)

        let subs = vec![
            sub(b"EDID", b"AllyFaction\0"),
            sub(b"DATA", &0x00u32.to_le_bytes()),
            sub(b"XNAM", &xnam),
        ];
        let f = parse_fact(0x77, &subs);
        assert_eq!(f.relations.len(), 1);
        assert_eq!(
            f.relations[0].combat_reaction, 2,
            "ally (combat_reaction=2) must round-trip — parser must read 4 bytes"
        );
    }

    /// Regression for #481 (FNV-2-L1): FACT DATA is a single-byte
    /// flags field on FO3 / FNV per UESP. Pre-fix the parser read 4
    /// bytes, so any garbage in bytes 1..=3 of the DATA payload
    /// (variable tail, neighbour padding) leaked into the high 24
    /// bits. Only bits 0–2 are authoritative; verify the fix rejects
    /// the high bytes.
    #[test]
    fn fact_data_reads_only_low_byte() {
        // Simulate a DATA sub-record whose first byte holds the real
        // flags (bit 0 = hidden) and whose remaining bytes are the
        // FNV tail (e.g. `unknown: u8 + crime_gold_multiplier: f32`)
        // or just padding. Pre-fix the parser treated all 4 bytes as
        // flags and reported `0x0EFF_FF01`; post-fix it reports `0x01`.
        let data = [
            0x01u8, // real flags — bit 0 = hidden
            0xFFu8, 0xFFu8, 0xEFu8, // tail / padding bytes; must NOT become flags
        ];
        let subs = vec![sub(b"EDID", b"SpookyFaction\0"), sub(b"DATA", &data)];
        let f = parse_fact(0x88, &subs);
        assert_eq!(
            f.flags, 0x01,
            "only byte 0 of DATA carries flag bits on FO3 / FNV (#481)"
        );
    }

    /// Edge case: a zero-length DATA sub-record must not crash and
    /// must leave flags at the default (0).
    #[test]
    fn fact_data_empty_leaves_flags_default() {
        let subs = vec![sub(b"EDID", b"PlaceholderFaction\0"), sub(b"DATA", &[])];
        let f = parse_fact(0x89, &subs);
        assert_eq!(
            f.flags, 0,
            "empty DATA must not override the FactionRecord default"
        );
    }

    // ── #591 / FO4-DIM6-06 face-morph capture ──────────────────────────

    /// Build a 36-byte FMRS payload from 9 floats.
    fn fmrs_bytes(values: [f32; 9]) -> Vec<u8> {
        let mut out = Vec::with_capacity(36);
        for v in values {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    /// FMRI / FMRS appear in alternating order on the wire and pair
    /// 1-to-1 inside the parsed record. Shape verified against vanilla
    /// `Fallout4.esm` named-NPC sub-records (Hancock has 6 paired
    /// FMRI/FMRS; MQ101KelloggScene player duplicate has 30).
    #[test]
    fn npc_pairs_fmri_with_fmrs_in_order() {
        let s0 = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let s1 = [-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0];
        let subs = vec![
            sub(b"EDID", b"NamedNpc\0"),
            sub(b"FMRI", &0xDEADu32.to_le_bytes()),
            sub(b"FMRS", &fmrs_bytes(s0)),
            sub(b"FMRI", &0xBEEFu32.to_le_bytes()),
            sub(b"FMRS", &fmrs_bytes(s1)),
        ];
        let n = parse_npc(0x600, &subs, GameKind::Fallout4);
        let face = n
            .face_morphs
            .as_ref()
            .expect("face_morphs must be Some when FMRI/FMRS present");
        assert_eq!(face.morphs.len(), 2);
        assert_eq!(face.morphs[0].form_id, 0xDEAD);
        assert_eq!(face.morphs[0].setting, s0);
        assert_eq!(face.morphs[1].form_id, 0xBEEF);
        assert_eq!(face.morphs[1].setting, s1);
    }

    /// MSDK / MSDV are parallel arrays: u32 keys + matching f32 values.
    /// One sub-record carries the full table on vanilla FO4 NPCs;
    /// `chunks_exact` walks every entry without dropping a tail.
    #[test]
    fn npc_msdk_msdv_walk_full_table() {
        let mut msdk = Vec::new();
        msdk.extend_from_slice(&0x10u32.to_le_bytes());
        msdk.extend_from_slice(&0x20u32.to_le_bytes());
        msdk.extend_from_slice(&0x30u32.to_le_bytes());
        let mut msdv = Vec::new();
        msdv.extend_from_slice(&0.25f32.to_le_bytes());
        msdv.extend_from_slice(&0.5f32.to_le_bytes());
        msdv.extend_from_slice(&0.75f32.to_le_bytes());
        let subs = vec![
            sub(b"EDID", b"Slidered\0"),
            sub(b"MSDK", &msdk),
            sub(b"MSDV", &msdv),
        ];
        let n = parse_npc(0x601, &subs, GameKind::Fallout4);
        let face = n.face_morphs.as_ref().unwrap();
        assert_eq!(face.slider_keys, vec![0x10, 0x20, 0x30]);
        assert_eq!(face.slider_values, vec![0.25, 0.5, 0.75]);
    }

    /// QNAM is 4 × f32 on FO4 NPCs (texture-lighting tint). HCLF / BCLF
    /// each are u32 FormIDs; multiple PNAM head-part FormIDs accumulate.
    #[test]
    fn npc_captures_qnam_hclf_bclf_pnam() {
        let mut qnam = Vec::new();
        for v in [0.6f32, 0.7, 0.8, 1.0] {
            qnam.extend_from_slice(&v.to_le_bytes());
        }
        let subs = vec![
            sub(b"EDID", b"FullFace\0"),
            sub(b"QNAM", &qnam),
            sub(b"HCLF", &0x1111u32.to_le_bytes()),
            sub(b"BCLF", &0x2222u32.to_le_bytes()),
            sub(b"PNAM", &0xAAAAu32.to_le_bytes()),
            sub(b"PNAM", &0xBBBBu32.to_le_bytes()),
            sub(b"PNAM", &0xCCCCu32.to_le_bytes()),
        ];
        let n = parse_npc(0x602, &subs, GameKind::Fallout4);
        let face = n.face_morphs.as_ref().unwrap();
        assert_eq!(face.texture_lighting, Some([0.6, 0.7, 0.8, 1.0]));
        assert_eq!(face.hair_color, Some(0x1111));
        assert_eq!(face.body_color, Some(0x2222));
        assert_eq!(face.head_parts, vec![0xAAAA, 0xBBBB, 0xCCCC]);
    }

    /// Face-morph block stays `None` for NPCs that ship none of the
    /// covered sub-records — pre-FO4 NPCs and FO4 generic settlers
    /// land in this branch. Regression pin so the
    /// `if !face.is_empty()` gate doesn't drift to `Some(Default)`.
    #[test]
    fn npc_without_face_subs_leaves_face_morphs_none() {
        let subs = vec![sub(b"EDID", b"PlainSettler\0")];
        let n = parse_npc(0x603, &subs, GameKind::Fallout4);
        assert!(n.face_morphs.is_none());
    }

    /// Mismatched FMRI/FMRS counts truncate to the shorter array
    /// instead of panicking. Defensive against malformed mod records;
    /// vanilla Bethesda content always pairs them 1-to-1.
    #[test]
    fn npc_mismatched_fmri_fmrs_truncates_to_shorter() {
        let s = [1.0; 9];
        // 3 FMRI but only 2 FMRS — should yield 2 paired entries.
        let subs = vec![
            sub(b"EDID", b"Malformed\0"),
            sub(b"FMRI", &0xA1u32.to_le_bytes()),
            sub(b"FMRI", &0xA2u32.to_le_bytes()),
            sub(b"FMRI", &0xA3u32.to_le_bytes()),
            sub(b"FMRS", &fmrs_bytes(s)),
            sub(b"FMRS", &fmrs_bytes(s)),
        ];
        let n = parse_npc(0x604, &subs, GameKind::Fallout4);
        let face = n.face_morphs.as_ref().unwrap();
        assert_eq!(face.morphs.len(), 2);
        assert_eq!(face.morphs[0].form_id, 0xA1);
        assert_eq!(face.morphs[1].form_id, 0xA2);
    }

    /// FNV NPC `PNAM` carries a single eyebrow HDPT FormID, NOT an
    /// FO4-style head-parts list. The `game`-aware gate keeps FNV
    /// PNAMs out of `face_morphs.head_parts`; M41.0 Phase 1a now
    /// captures them into `runtime_facegen.eyebrow_form_id` instead
    /// of dropping them on the floor.
    #[test]
    fn npc_fnv_pnam_lands_in_runtime_facegen_eyebrow() {
        let subs = vec![
            sub(b"EDID", b"FnvNpc\0"),
            // FNV-style PNAM: a single 4-byte eyebrow HDPT FormID.
            sub(b"PNAM", &0xDEADu32.to_le_bytes()),
        ];
        let n = parse_npc(0x606, &subs, GameKind::Fallout3NV);
        assert!(
            n.face_morphs.is_none(),
            "FNV PNAM must not populate face_morphs.head_parts (FO4 semantic)"
        );
        let recipe = n
            .runtime_facegen
            .as_ref()
            .expect("FNV PNAM must produce runtime_facegen");
        assert_eq!(recipe.eyebrow_form_id, Some(0xDEAD));
    }

    /// FGGS / FGGA / FGTS slider arrays land in fixed-size float
    /// arrays. Pre-Phase-3b the parser is the only consumer; the
    /// spawn-side morph evaluator picks them up from
    /// `runtime_facegen.fggs` directly.
    #[test]
    fn npc_fnv_fggs_fgga_fgts_populate_runtime_facegen() {
        let mut fggs = Vec::with_capacity(50 * 4);
        for i in 0..50 {
            fggs.extend_from_slice(&(i as f32 * 0.1).to_le_bytes());
        }
        let mut fgga = Vec::with_capacity(30 * 4);
        for i in 0..30 {
            fgga.extend_from_slice(&(i as f32 * -0.05).to_le_bytes());
        }
        let mut fgts = Vec::with_capacity(50 * 4);
        for i in 0..50 {
            fgts.extend_from_slice(&(i as f32 * 0.02).to_le_bytes());
        }
        let subs = vec![
            sub(b"EDID", b"SunnyMockup\0"),
            sub(b"FGGS", &fggs),
            sub(b"FGGA", &fgga),
            sub(b"FGTS", &fgts),
        ];
        let n = parse_npc(0x607, &subs, GameKind::Fallout3NV);
        let recipe = n
            .runtime_facegen
            .as_ref()
            .expect("FGGS/FGGA/FGTS must produce runtime_facegen");
        assert!((recipe.fggs[7] - 0.7).abs() < 1e-6);
        assert!((recipe.fgga[5] - -0.25).abs() < 1e-6);
        assert!((recipe.fgts[3] - 0.06).abs() < 1e-6);
        // Slot beyond the table stays at the default 0.0.
        assert_eq!(recipe.fggs[49], 4.9_f32);
        assert_eq!(recipe.fgga[29], -1.45_f32);
    }

    /// Short FGGS payload pads with zeros — the parser must not
    /// over-read or panic on truncated mod records.
    #[test]
    fn npc_fnv_short_fggs_pads_with_zero() {
        // 5 × f32 = 20 bytes; far short of the canonical 200.
        let mut fggs = Vec::with_capacity(5 * 4);
        for v in [1.0f32, 2.0, 3.0, 4.0, 5.0] {
            fggs.extend_from_slice(&v.to_le_bytes());
        }
        let subs = vec![sub(b"EDID", b"TruncMod\0"), sub(b"FGGS", &fggs)];
        let n = parse_npc(0x608, &subs, GameKind::Fallout3NV);
        let recipe = n.runtime_facegen.as_ref().unwrap();
        assert_eq!(recipe.fggs[0], 1.0);
        assert_eq!(recipe.fggs[4], 5.0);
        for v in &recipe.fggs[5..] {
            assert_eq!(*v, 0.0);
        }
    }

    /// HCLR / HNAM / LNAM / ENAM all land in `runtime_facegen` on
    /// kf-era games. HCLR's optional 4th byte is dropped per UESP.
    #[test]
    fn npc_fnv_hclr_hnam_lnam_enam_populate_runtime_facegen() {
        let subs = vec![
            sub(b"EDID", b"FullRecipe\0"),
            sub(b"HCLR", &[0x33, 0x55, 0x77, 0xFF]), // 4-byte; alpha dropped
            sub(b"HNAM", &0xCAFEu32.to_le_bytes()),
            sub(b"LNAM", &0xBEEFu32.to_le_bytes()),
            sub(b"ENAM", &0xF00Du32.to_le_bytes()),
        ];
        let n = parse_npc(0x609, &subs, GameKind::Fallout3NV);
        let recipe = n.runtime_facegen.as_ref().unwrap();
        assert_eq!(recipe.hair_color_rgb, Some([0x33, 0x55, 0x77]));
        assert_eq!(recipe.hair_form_id, Some(0xCAFE));
        assert_eq!(recipe.unused_lnam, Some(0xBEEF));
        assert_eq!(recipe.eyes_form_id, Some(0xF00D));
    }

    /// FO4 NPCs ship none of the kf-era recipe sub-records — and
    /// even if a malformed mod adds an FGGS payload to an FO4 NPC,
    /// the gate keeps `runtime_facegen` at `None`. Mirror property:
    /// kf-era NPCs with FO4-shaped FMRI/FMRS don't populate
    /// `face_morphs`. Both are pinned to keep the predicates honest.
    #[test]
    fn npc_runtime_facegen_and_face_morphs_are_mutually_exclusive() {
        let fggs = vec![0u8; 200];
        let subs_fo4 = vec![sub(b"EDID", b"Fo4Stray\0"), sub(b"FGGS", &fggs)];
        let n = parse_npc(0x60A, &subs_fo4, GameKind::Fallout4);
        assert!(n.runtime_facegen.is_none(), "FO4 must not parse FGGS");

        let mut fmrs = Vec::with_capacity(36);
        for v in [0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9] {
            fmrs.extend_from_slice(&v.to_le_bytes());
        }
        let subs_fnv = vec![
            sub(b"EDID", b"FnvStray\0"),
            sub(b"FMRI", &0xDEADu32.to_le_bytes()),
            sub(b"FMRS", &fmrs),
        ];
        let n = parse_npc(0x60B, &subs_fnv, GameKind::Fallout3NV);
        assert!(n.face_morphs.is_none(), "FNV must not parse FMRI/FMRS");
    }

    /// Wrong-sized FMRS (e.g. a Skyrim record that ships a smaller
    /// payload, or a corrupt mod) is dropped silently — the length
    /// gate `>= 36` keeps malformed bytes from being re-interpreted as
    /// a partial setting array. The matched FMRI then becomes an
    /// orphan and the truncation rule above drops it too.
    #[test]
    fn npc_undersized_fmrs_is_dropped() {
        let subs = vec![
            sub(b"EDID", b"BadBytes\0"),
            sub(b"FMRI", &0xF00Du32.to_le_bytes()),
            sub(b"FMRS", &[0u8; 16]), // < 36 bytes
        ];
        let n = parse_npc(0x605, &subs, GameKind::Fallout4);
        // FMRI captured but FMRS dropped → mismatched (1 vs 0) →
        // truncate to 0 → no morphs → block is empty → None.
        assert!(n.face_morphs.is_none());
    }
}
