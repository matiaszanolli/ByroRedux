//! Low-level binary reader for the TES4 record format.
//!
//! ESM/ESP files are sequences of records and groups. Each record has a
//! 4-char type code, data size, flags, and form ID. Records contain
//! sub-records (type + size + data). Groups contain other records/groups.
//!
//! **Per-game header layout.** Oblivion (TES4) uses a 20-byte record
//! header and 20-byte group header, ending after `vc_info`. Every later
//! game (Fallout 3, New Vegas, Skyrim, FO4, etc.) extends both to 24
//! bytes with a trailing version + unknown field. The first 16 bytes are
//! identical in either layout, so we only need to branch on the
//! additional skip at the end.

use anyhow::{ensure, Result};
use flate2::read::ZlibDecoder;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::rc::Rc;

/// Record flag: data is zlib-compressed.
const FLAG_COMPRESSED: u32 = 0x00040000;
/// Record flag: this override deletes the inherited authored record.
const FLAG_DELETED: u32 = 0x0000_0020;

/// Largest inflation multiple a compressed record may claim over its own
/// compressed byte count (#3399).
///
/// Census over every compressed record in four shipped masters — 33 179
/// in `FalloutNV.esm`, 44 153 in `Skyrim.esm`, 56 399 in `Fallout4.esm`,
/// 0 in `Oblivion.esm`, which compresses nothing — puts the worst real
/// ratio at **102.1:1** (an FNV `LAND` inflating 75 bytes to 7 658). 512
/// is 5× that, so no vanilla or vanilla-shaped mod record is anywhere
/// near it, while DEFLATE's practical ~1000:1 bomb ratio is cut off.
const MAX_RECORD_INFLATION_RATIO: usize = 512;

/// Floor under [`MAX_RECORD_INFLATION_RATIO`], so a tiny compressed
/// payload is not held to a tiny ceiling (#3399). The worst observed
/// ratios all come from ~60–85 byte `LAND` records, where a pure
/// multiple would be the binding constraint; 64 KiB clears the largest
/// of those (7 658 bytes) by 8×.
const MIN_RECORD_INFLATED_CEILING: usize = 64 * 1024;

/// Absolute ceiling on one record's inflated size, whatever its
/// compressed length (#3399). The ratio bound alone scales with the
/// attacker's input — a 100 MB compressed record could still claim
/// 51 GB under it — so the two are combined. The largest real inflated
/// record across the same four masters is 225 433 bytes (`Fallout4.esm`),
/// which this clears by ~290×. Sibling of `byroredux_bsa::safety`'s
/// `MAX_CHUNK_BYTES`, set lower because one ESM *record* has far tighter
/// realistic bounds than one archive chunk.
const MAX_RECORD_INFLATED_BYTES: usize = 64 * 1024 * 1024;

/// The inflated-size ceiling for a record whose compressed payload is
/// `compressed_len` bytes. See the three constants above.
fn record_inflation_ceiling(compressed_len: usize) -> usize {
    MAX_RECORD_INFLATED_BYTES.min(
        MIN_RECORD_INFLATED_CEILING.max(compressed_len.saturating_mul(MAX_RECORD_INFLATION_RATIO)),
    )
}

/// Record flag: "Visible When Distant" / "Has Distant LOD" (#1731 /
/// LC-D7-02). Not to be confused with the deleted-REFR tombstone flag
/// `0x20` (SKY-D4-01 / #1660) — a different bit on the same `flags` field.
/// `pub` (unlike [`FLAG_COMPRESSED`]) so downstream LOD-spawn consumers
/// (`docs/engine/exal.md` §5.4, LC-D7-01) can reference it by name instead
/// of a magic number.
pub const FLAG_VISIBLE_WHEN_DISTANT: u32 = 0x00010000;

/// Maximum number of nested GRUP bodies any recursive ESM walker may enter.
/// Vanilla files stay below five levels; 64 leaves generous headroom while
/// preventing adversarial plugins from exhausting the native stack (#3237).
pub(crate) const MAX_GRUP_NESTING_DEPTH: u32 = 64;

/// ESM format variant — determines record / group header size.
///
/// The two surviving layouts across the Bethesda lineage:
/// - [`Oblivion`](Self::Oblivion) — 20-byte headers (TES4, Oblivion.esm)
/// - [`Tes5Plus`](Self::Tes5Plus) — 24-byte headers (FO3 / FNV / Skyrim /
///   FO4 / FO76 / Starfield)
///
/// Morrowind's TES3 format is entirely different and not supported here;
/// it would need its own reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EsmVariant {
    /// Oblivion — 20-byte record and group headers.
    Oblivion,
    /// FO3 / FNV / Skyrim LE+SE / FO4 / FO76 / Starfield — 24-byte headers.
    Tes5Plus,
}

impl EsmVariant {
    /// Auto-detect the ESM variant from a file buffer.
    ///
    /// The heuristic looks at byte offset 20 in the file. Every Bethesda
    /// ESM begins with a `TES4` record, and the first sub-record inside
    /// its data area is always `HEDR`. In Oblivion, the record header is
    /// 20 bytes, so bytes 20-23 spell out `"HEDR"`. In every later game
    /// the header is 24 bytes, so bytes 20-23 are the version u16 +
    /// unknown u16 (small integers, never ASCII). Test the four ASCII
    /// bytes and you have a deterministic, one-shot detector.
    pub fn detect(data: &[u8]) -> Self {
        if data.len() >= 24 && &data[20..24] == b"HEDR" {
            Self::Oblivion
        } else {
            Self::Tes5Plus
        }
    }

    /// Record header size in bytes (`type + data_size + flags + form_id`
    /// plus trailing metadata).
    pub fn record_header_size(self) -> usize {
        match self {
            Self::Oblivion => 20,
            Self::Tes5Plus => 24,
        }
    }

    /// Group header size in bytes (`GRUP + size + label + group_type`
    /// plus trailing metadata).
    pub fn group_header_size(self) -> usize {
        match self {
            Self::Oblivion => 20,
            Self::Tes5Plus => 24,
        }
    }
}

/// Fine-grained game identity for sub-record layout dispatch.
///
/// [`EsmVariant`] only splits "Oblivion (20-byte headers)" from "everything
/// else (24-byte headers)" because that's what the low-level walker needs.
/// Per-record layouts diverge within the Tes5Plus family: FO3/FNV share one
/// schema for ARMO/WEAP/AMMO DATA, Skyrim uses a different one (no health
/// field, BOD2 instead of BMDT, DNAM as packed armor rating), and FO4 adds
/// its own variants again. Callers that parse body data need this finer
/// distinction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameKind {
    /// Oblivion (TES4, HEDR 1.0).
    Oblivion,
    /// Fallout 3 (HEDR 0.94 on the GOTY master, 0.85 pre-GOTY — the
    /// discriminating table in `from_hedr` below has this right; this line
    /// used to name only the pre-GOTY value, #3756) and Fallout: New Vegas
    /// (HEDR 1.34). These two
    /// share their DATA/DNAM layouts everywhere the current parser cares
    /// about, so they collapse to one game kind.
    #[default]
    Fallout3NV,
    /// Skyrim LE + SE (HEDR 1.7). New ARMO/WEAP/AMMO sub-record schemas.
    Skyrim,
    /// Fallout 4 (HEDR 0.95). SCOL/PKIN/TXST and yet another item schema.
    Fallout4,
    /// Fallout 76 (HEDR 266.0 — unusually large; see the sampled-values
    /// table in [`GameKind::from_header`]).
    Fallout76,
    /// Starfield (HEDR 0.96).
    Starfield,
}

impl GameKind {
    /// Derive the game kind from the ESM variant, HEDR `Version` f32, and
    /// TES4 record-header version word. The latter disambiguates FO4 DLCs
    /// whose HEDR 0.95 overlaps FO3's band. Callers without either value may
    /// pass zero, which falls back to [`GameKind::Fallout3NV`].
    pub fn from_header(variant: EsmVariant, hedr_version: f32, record_version: u16) -> Self {
        match variant {
            EsmVariant::Oblivion => Self::Oblivion,
            EsmVariant::Tes5Plus => {
                // HEDR versions sampled from real vanilla masters at
                // 2026-04-19, FO76 re-read from disk 2026-08-28 (#3405):
                //   FO3 (GOTY) = 0.94    (bytes d7 a3 70 3f)
                //   FO4        = 1.0     (bytes 00 00 80 3f)
                //   Starfield  = 0.96    (bytes 8f c2 75 3f)
                //   FNV        = 1.34    (bytes 1f 85 ab 3f)
                //   Skyrim SE  = 1.71    (bytes 48 e1 da 3f)
                //   FO76       = 266.0   (bytes 00 00 85 43)
                // Exact float equality is unsafe — match on small bands
                // that leave clear gaps between the known values.
                //
                // #3405 — this table read `FO76 = 68.0` until 2026-08-28.
                // Both installed FO76 masters (`SeventySix.esm`, `NW.esm`)
                // ship 266.0 / TES4 record version 209. The band below is
                // unaffected — `>= 60.0` is a deliberately low floor, not a
                // tight fit around the sampled value — but this table is the
                // evidence every band gap is reasoned from, so a wrong entry
                // here is what misplaces the *next* band.
                //
                // Pre-fix the FO3 band (0.94..=0.955) routed every FO3
                // master to Fallout4 — and FO4's real 1.0 fell through
                // to Fallout3NV — so the FO3↔FO4 classification was
                // inverted. A mis-band today is no longer latent — three
                // dedicated FO4 arms in items.rs now depend on it: `ARMO
                // DATA` (items.rs, swaps the value/weight/health field
                // order vs FO3NV/Oblivion at the same 12-byte length),
                // `WEAP DATA` (empty FO4 arm — FO4 ships none), and `BOOK`
                // (its own 8-byte FO4 arm). A FO3↔FO4 mis-band would
                // silently swap armor weight and health and zero every
                // weapon's value/weight/damage. See #439 / audit FO3-3-01.
                if hedr_version >= 60.0 {
                    // Floor deliberately far below the real 266.0 (#3405):
                    // nothing else in the lineage exceeds 1.71, so the whole
                    // decade above 60 is free for FO76's outlier scheme.
                    Self::Fallout76
                } else if (1.6..=1.8).contains(&hedr_version) {
                    Self::Skyrim
                } else if (0.98..=1.04).contains(&hedr_version) {
                    Self::Fallout4
                } else if (0.945..=0.955).contains(&hedr_version) && record_version >= 100 {
                    // FO4's CK and five of the seven shipped DLC masters
                    // stamp HEDR 0.95, which overlaps the old FO3 band.
                    // Their TES4 record version is 131 (FO3/FNV use 2),
                    // giving us an independent on-disk discriminator.
                    Self::Fallout4
                } else if (0.955..=0.97).contains(&hedr_version) {
                    Self::Starfield
                } else if (0.93..=0.95).contains(&hedr_version) {
                    // FO3 GOTY (0.94). Pre-GOTY FO3 shipped 0.85 which
                    // falls through to the Fallout3NV tail branch — same
                    // parsing family, so the distinction is purely
                    // cosmetic.
                    Self::Fallout3NV
                } else {
                    // FNV (1.34), pre-GOTY FO3 (0.85), or unknown →
                    // treat as the legacy "Fallout" family.
                    Self::Fallout3NV
                }
            }
        }
    }

    // ── Semantic predicates ────────────────────────────────────────
    //
    // These are the canonical version-feature gates consumed by NPC
    // spawn code (M41.0) and any future face / animation / tint
    // pipeline. The convention mirrors `NifVariant`'s
    // `has_shader_alpha_refs()` / `has_material_crc()` style: each
    // predicate names a *capability*, not a game; new games extend
    // the match arms in one place instead of bumping a dozen
    // `match game { ... }` blocks scattered through the parsers.

    /// True when the NPC face shape is a runtime-evaluated recipe
    /// (FGGS / FGGA / FGTS slider arrays + `.egm` / `.egt` / `.tri`
    /// sidecar deltas applied at spawn time on top of a race-shared
    /// base head NIF). Oblivion / FO3 / FNV use this model.
    ///
    /// **Mutually exclusive** with [`uses_prebaked_facegen`].
    pub fn has_runtime_facegen_recipe(self) -> bool {
        matches!(self, Self::Oblivion | Self::Fallout3NV)
    }

    /// True when the NPC face shape is a per-NPC pre-baked NIF
    /// shipped under `meshes\actors\character\facegendata\facegeom\
    /// <plugin>\<formid:08x>.nif` with a matching face-tint DDS at
    /// `textures\actors\character\facegendata\facetint\...`.
    /// Skyrim / FO4 / FO76 / Starfield use this model.
    ///
    /// **Mutually exclusive** with [`has_runtime_facegen_recipe`].
    pub fn uses_prebaked_facegen(self) -> bool {
        matches!(
            self,
            Self::Skyrim | Self::Fallout4 | Self::Fallout76 | Self::Starfield,
        )
    }

    /// True when the engine ships `.kf` keyframe animation clips that
    /// the existing [`crates::nif::anim::import_kf`] importer can
    /// decode directly. FNV vanilla ships ~962 idle clips under
    /// `meshes\characters\_male\idleanims\`. Skyrim+ vanilla ships
    /// **zero** `.kf` files (Havok `.hkx` only).
    pub fn has_kf_animations(self) -> bool {
        matches!(self, Self::Oblivion | Self::Fallout3NV)
    }

    /// True when the engine animates actors via Havok Behavior Format
    /// (`.hkx` files + behaviour graphs). Skyrim onwards. The current runtime
    /// decodes Skyrim 64-bit spline-compressed IDLE clips; later games remain
    /// explicit compatibility work.
    pub fn has_havok_animations(self) -> bool {
        matches!(
            self,
            Self::Skyrim | Self::Fallout4 | Self::Fallout76 | Self::Starfield,
        )
    }

    /// True when an actor stores its actor values (SPECIAL + overrides)
    /// as a `PRPS` "Properties" array of `(AVIF FormID, f32)` pairs and
    /// ships baked derived stats (`Calculated Health` / `Calculated
    /// Action Points`) in an 8-byte `DNAM` — the Creation-Engine actor
    /// model introduced in FO4 and carried forward to FO76 / Starfield.
    ///
    /// **Distinct from [`uses_prebaked_facegen`]**, which also matches
    /// Skyrim: Skyrim predates the AVIF-FormID property model and stores
    /// NPC stats differently, so this predicate excludes it. The `PRPS`
    /// `(AVIF, value)` wire format is verified against the xEdit FO4
    /// `NPC_` definition; FO76 / Starfield share the model by lineage
    /// (the leading `Calculated Health`/`AP` u16 prefix of `DNAM` is the
    /// stable head — confirm the tail when those games are exercised).
    pub fn uses_actor_value_properties(self) -> bool {
        matches!(self, Self::Fallout4 | Self::Fallout76 | Self::Starfield)
    }

    /// True when an actor's `NPC_` carries `PRKZ`/`PRKR` perk entries.
    ///
    /// **Deliberately wider than [`uses_actor_value_properties`]** (#3158).
    /// Perks arrived in Skyrim, one release before the AVIF-FormID property
    /// model, so gating the `PRKR` parse on the actor-value predicate made
    /// `Perks` unreachable for every Skyrim NPC — and with it every
    /// `HasPerk` CTDA on the project's reference title.
    ///
    /// Censused against the shipped masters rather than inferred from
    /// lineage (`cargo run -p byroredux-plugin --example probe_npc_perks`):
    ///
    /// | master | `NPC_` | with `PRKR` | entries | `PRKR` size |
    /// |---|---:|---:|---:|---:|
    /// | `Skyrim.esm` | 5118 | 1620 | 7993 | 8 B |
    /// | `Fallout4.esm` | 3015 | 1361 | 2771 | 5 B |
    /// | `FalloutNV.esm` | 3816 | 0 | 0 | — |
    /// | `Fallout3.esm` | 1647 | 0 | 0 | — |
    /// | `Oblivion.esm` | 2482 | 0 | 0 | — |
    ///
    /// FO3/FNV/Oblivion ship none, so their `HasPerk` zero is data-correct
    /// for NPCs — the remaining gap there is the player, who has no perk
    /// source of any kind yet.
    ///
    /// The two payload widths need no separate decode: both start with the
    /// PERK FormID `u32` and carry the rank at byte 4 (Skyrim's extra three
    /// bytes are unused padding), which is exactly what the `PRKR` arm
    /// reads. FO76/Starfield are included by lineage from FO4 — confirm the
    /// width when those games are exercised.
    pub fn uses_npc_perk_entries(self) -> bool {
        matches!(
            self,
            Self::Skyrim | Self::Fallout4 | Self::Fallout76 | Self::Starfield
        )
    }
}

/// Binary reader for ESM/ESP files.
pub struct EsmReader<'a> {
    data: &'a [u8],
    pos: usize,
    variant: EsmVariant,
    /// Per-plugin FormID mod-index remap. `None` = identity (single
    /// plugin with no masters). `Some` = remap record FormID top bytes
    /// according to this plugin's position in the global load order.
    /// See [`FormIdRemap`] for the mapping rules.
    form_id_remap: Option<FormIdRemap>,
    record_type_trace: Option<Rc<RefCell<RecordTypeTrace>>>,
}

/// Optional generic record-header trace used to build read-only metadata.
#[derive(Debug, Default)]
pub struct RecordTypeTrace {
    pub record_types: HashMap<u32, [u8; 4]>,
    pub deleted_records: HashSet<u32>,
}

/// FormID mod-index remap rule for a single plugin in a load order.
///
/// A plugin-local FormID has its top byte pointing into the plugin's
/// MASTERS array (or to itself if the top byte equals the master
/// count). At load time every FormID needs to be rewritten to point
/// into the GLOBAL load order so references and map keys stay
/// collision-free across plugins. See `FormIdPair` in
/// `crates/plugin/src/legacy/mod.rs` for the dual-index form.
///
/// Single-plugin load (the default) uses `plugin_index = 0` and an
/// empty `master_indices`, which makes the remap a no-op (a file's
/// own records have top byte 0, and `0 == master_indices.len()` →
/// self-reference → stays 0). Pre-#445 the reader had no remap at
/// all; every record's raw u32 landed directly in `EsmIndex` maps
/// and multi-plugin loads silently collided on the shared
/// base-game-index 0x01 (Anchorage / BrokenSteel / PointLookout /
/// Pitt / Zeta all use 0x01 for their own new forms).
#[derive(Debug, Clone)]
pub struct FormIdRemap {
    /// This plugin's slot in the global load order.
    pub plugin_slot: GlobalSlot,
    /// For each entry in this plugin's MASTERS list, the global slot of
    /// that master. `master_slots[N]` is where a FormID with mod-index
    /// `N` actually lives.
    pub master_slots: Vec<GlobalSlot>,
}

/// A plugin's resolved position in the global load order.
///
/// Regular plugins occupy a full top-byte slot (`0x00`–`0xFD`) with a
/// 24-bit object-id space. ESL / light-master plugins (TES4 flag `0x0200`)
/// all share the `0xFE` top byte and are distinguished by a 12-bit
/// load-order sub-index, leaving only a 12-bit (`0x000`–`0xFFF`) object-id
/// space. Modelling the slot as a sum type lets [`FormIdRemap::remap`]
/// decode both kinds — and references *to* an ESL master — uniformly.
/// See the Creation Engine light-master spec / SK-D4-03 / #1554.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalSlot {
    /// Full-byte load-order index (`0x00`–`0xFD`).
    Regular(u8),
    /// `0xFE` light-master space; the `u16` is the 12-bit sub-index.
    Light(u16),
}

impl GlobalSlot {
    /// Compose this slot with a raw plugin-local FormID into a global
    /// FormID.
    ///
    /// `Regular` keeps the bottom 24 bits as the object id and writes the
    /// byte index into the top byte. `Light` packs the 12-bit sub-index
    /// into the `0xFE` space and keeps only the bottom 12 bits — the ESL
    /// object-id range — because an ESL's object space is 12 bits, not 24.
    pub fn compose(self, raw: u32) -> u32 {
        match self {
            GlobalSlot::Regular(byte) => ((byte as u32) << 24) | (raw & 0x00FF_FFFF),
            GlobalSlot::Light(sub) => {
                0xFE00_0000 | (((sub as u32) & 0x0FFF) << 12) | (raw & 0x0000_0FFF)
            }
        }
    }
}

impl FormIdRemap {
    /// All-regular convenience constructor (no ESL in the load order) —
    /// every slot is a full-byte index. Used by the multi-plugin tests.
    #[cfg(test)]
    pub fn regular(plugin_index: u8, master_indices: Vec<u8>) -> Self {
        Self {
            plugin_slot: GlobalSlot::Regular(plugin_index),
            master_slots: master_indices
                .into_iter()
                .map(GlobalSlot::Regular)
                .collect(),
        }
    }

    /// Apply this remap to a raw plugin-local FormID.
    ///
    /// The object-id bits (bottom 24 for regular, bottom 12 for ESL) are
    /// preserved; the load-order portion is rewritten to the global slot
    /// of whichever plugin owns this form. See [`GlobalSlot::compose`].
    pub fn remap(&self, raw: u32) -> u32 {
        // FormID zero is the cross-record NULL sentinel, not a form owned by
        // the plugin at local object index zero. This matters for ESLs: their
        // self slot composes into 0xFE... and would otherwise turn an absent
        // optional reference into a live-looking light-space FormID.
        if raw == 0 {
            return 0;
        }
        let mod_index = (raw >> 24) as usize;
        let slot = if mod_index == self.master_slots.len() {
            // Self-reference — top byte equals master count per the
            // Bethesda ESM spec.
            self.plugin_slot
        } else if mod_index < self.master_slots.len() {
            self.master_slots[mod_index]
        } else if self.master_slots.is_empty() {
            // Standalone / single-plugin load (no masters) with a top byte
            // past index 0 — the known Bethesda authoring artifact: vanilla
            // Oblivion.esm (and a few FO3/FNV masters) ship a handful of
            // forms prefixed 0x01 despite having no master at that index.
            // There is no plugin at that global slot, so the form cannot be
            // remapped; pass it through unchanged and log at debug. (Prior
            // code logged this at warn per-form — spam on every verbose
            // Oblivion load — and its comment wrongly claimed single-plugin
            // loads never reach here. #1308 / OBL-D6-NEW-04.)
            //
            // Clamping to self is deliberately NOT done: whether a 0x01
            // local-id aliases a real 0x00 form in the file is unverified,
            // and a wrong clamp would collide two distinct forms onto one
            // global FormID in EsmIndex — strictly worse than the harmless
            // (no rendering impact) dangling pass-through.
            log::debug!(
                "standalone FormID {raw:08x} (mod_index {mod_index}, no masters) \
                 passed through — Bethesda index-{mod_index:02x} artifact"
            );
            return raw;
        } else {
            // Multi-plugin load with an out-of-range index — genuinely
            // suspicious (malformed file or an in-memory injected form).
            // Keep this at warn; it is rare and worth surfacing.
            log::warn!(
                "FormID {raw:08x} has mod_index {mod_index} but plugin has {} masters",
                self.master_slots.len()
            );
            return raw;
        };
        slot.compose(raw)
    }
}

/// Parsed record header (CELL, REFR, STAT, TES4, etc.).
#[derive(Debug, Clone)]
pub struct RecordHeader {
    pub record_type: [u8; 4],
    pub data_size: u32,
    pub flags: u32,
    pub form_id: u32,
}

impl RecordHeader {
    /// "Visible When Distant" / "Has Distant LOD" (#1731 / LC-D7-02).
    /// Signals the base record should cull its full-resolution model once
    /// a distant-LOD stand-in (Skyrim+/FO4 `.bto`, or Oblivion/FO3/FNV
    /// `DistantLOD` placement) is shown for it, and marks it LOD-eligible
    /// in the first place. See `crates/plugin/src/esm/reader.rs::FLAG_VISIBLE_WHEN_DISTANT`
    /// and `docs/engine/exal.md` §5.4.
    pub fn is_visible_when_distant(&self) -> bool {
        self.flags & FLAG_VISIBLE_WHEN_DISTANT != 0
    }
}

/// Parsed group header (GRUP).
#[derive(Debug, Clone)]
pub struct GroupHeader {
    pub label: [u8; 4],
    pub group_type: u32,
    /// Total size including this header.
    pub total_size: u32,
}

/// A sub-record within a record.
#[derive(Debug, Clone)]
pub struct SubRecord {
    pub sub_type: [u8; 4],
    pub data: Vec<u8>,
}

/// File header data from the TES4 record.
#[derive(Debug)]
pub struct FileHeader {
    pub master_files: Vec<String>,
    pub record_count: u32,
    /// HEDR `Version` f32 (sub-record offset 0). 0.0 when absent (synthetic
    /// test fixtures often omit HEDR). Feed into [`GameKind::from_header`].
    pub hedr_version: f32,
    /// TES4 record-header version word (24-byte headers, offsets 20..22).
    /// FO4 masters/plugins use 131 while FO3/FNV use 2, which disambiguates
    /// FO4's common HEDR 0.95 from FO3's nearby 0.94.
    pub record_version: u16,
    /// TES4 record flag bit `0x80` (Localized). When set, Skyrim+
    /// plugins encode FULL / DESC / RNAM / etc. as a 4-byte u32
    /// reference into companion `Strings/*.{STRINGS,DLSTRINGS,ILSTRINGS}`
    /// files rather than inline z-strings. Downstream record parsers
    /// consult the thread-local `CURRENT_PLUGIN_LOCALIZED` flag
    /// (`records/common.rs`) when decoding those sub-records so a
    /// 4-byte payload becomes a `"<lstring 0xNNNNNNNN>"` placeholder
    /// instead of 3-character UTF-8 garbage. See audit S6-03 / #348.
    pub localized: bool,
    /// TES4 record flag bit `0x0200` (Light Master / ESL). When set, this
    /// plugin shares the `0xFE` top-byte load-order space with every other
    /// ESL: its forms are addressed as `0xFE` + a 12-bit load-order
    /// sub-index + a 12-bit object id, rather than a full-byte mod-index +
    /// 24-bit object id. The load-order builder turns this into a
    /// [`GlobalSlot::Light`] so [`FormIdRemap`] decodes the plugin's forms
    /// (and references to ESL masters) correctly. No vanilla Skyrim SE /
    /// FO4 / Starfield **master** (`.esm`) is ESL-flagged — but the decode
    /// path is exercised on stock content anyway: a stock Anniversary
    /// Edition install ships three ESL-flagged plugins, one of them
    /// (`_ResourcePack.esl`, 374 records, 3-entry MAST list) base-game
    /// content rather than third-party or Creation Club (measured
    /// 2026-08-30, SK-D4-02). This is not a mod-only code path.
    /// See SK-D4-03 / #1554.
    pub light_master: bool,
}

impl<'a> EsmReader<'a> {
    /// Create a reader, auto-detecting the game variant from the file
    /// header. Oblivion gets 20-byte record/group headers; everything
    /// else gets 24. See [`EsmVariant::detect`].
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            variant: EsmVariant::detect(data),
            form_id_remap: None,
            record_type_trace: None,
        }
    }

    /// Create a reader with an explicit variant — used by the unit
    /// tests which build synthetic 24-byte records regardless of game.
    pub fn with_variant(data: &'a [u8], variant: EsmVariant) -> Self {
        Self {
            data,
            pos: 0,
            variant,
            form_id_remap: None,
            record_type_trace: None,
        }
    }

    /// Install a FormID mod-index remap — call once, before walking
    /// records, when this reader is loading a plugin in a multi-plugin
    /// load order. See [`FormIdRemap`] for the rules. See #445.
    pub fn set_form_id_remap(&mut self, remap: FormIdRemap) {
        self.form_id_remap = Some(remap);
    }

    /// Trace remapped record identities as existing parsers consume headers.
    pub fn enable_record_type_trace(&mut self) -> Rc<RefCell<RecordTypeTrace>> {
        let trace = Rc::new(RefCell::new(RecordTypeTrace::default()));
        self.record_type_trace = Some(Rc::clone(&trace));
        trace
    }

    /// Get a clone of the installed FormID remap (if any).
    /// Used by parsers to remap cross-record FormID references.
    pub fn get_form_id_remap(&self) -> Option<FormIdRemap> {
        self.form_id_remap.clone()
    }

    /// Apply the installed FormID remap (if any) to a raw plugin-local
    /// u32. Callsite-agnostic: use this anywhere a sub-record field
    /// carries a cross-record FormID reference (REFR.NAME, XOWN, XLOC,
    /// etc.) so cross-plugin references stay valid. Identity when no
    /// remap is set. See #445.
    pub fn remap_form_id(&self, raw: u32) -> u32 {
        self.form_id_remap
            .as_ref()
            .map_or(raw, |remap| remap.remap(raw))
    }

    pub fn variant(&self) -> EsmVariant {
        self.variant
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    /// Peek at the next 4 bytes without advancing.
    pub fn peek_type(&self) -> Option<[u8; 4]> {
        if self.pos + 4 <= self.data.len() {
            Some([
                self.data[self.pos],
                self.data[self.pos + 1],
                self.data[self.pos + 2],
                self.data[self.pos + 3],
            ])
        } else {
            None
        }
    }

    /// Check if the next record is a GRUP.
    pub fn is_group(&self) -> bool {
        self.peek_type() == Some(*b"GRUP")
    }

    /// Read a record header (20 bytes on Oblivion, 24 on FO3+).
    pub fn read_record_header(&mut self) -> Result<RecordHeader> {
        let header_size = self.variant.record_header_size();
        ensure!(self.remaining() >= header_size, "Truncated record header");
        let record_type = self.read_bytes_4();
        let data_size = self.read_u32();
        let flags = self.read_u32();
        let form_id_raw = self.read_u32();
        // Trailing metadata: Oblivion = 4 bytes (vc_info); FO3+ = 8 bytes
        // (vc_info + unknown + version + unknown). We don't consume any
        // of it today, just skip past.
        self.skip(header_size - 16);
        // #445 — remap the record's own FormID through the installed
        // load-order so multi-plugin maps stay collision-free. No-op
        // when no remap is set (single-plugin load).
        let form_id = self.remap_form_id(form_id_raw);
        if form_id != 0 {
            if let Some(trace) = &self.record_type_trace {
                let mut trace = trace.borrow_mut();
                if flags & FLAG_DELETED != 0 {
                    trace.record_types.remove(&form_id);
                    trace.deleted_records.insert(form_id);
                } else {
                    trace.deleted_records.remove(&form_id);
                    trace.record_types.insert(form_id, record_type);
                }
            }
        }
        Ok(RecordHeader {
            record_type,
            data_size,
            flags,
            form_id,
        })
    }

    /// Read a group header (20 bytes on Oblivion, 24 on FO3+). Caller
    /// must verify `peek_type() == "GRUP"` first.
    pub fn read_group_header(&mut self) -> Result<GroupHeader> {
        let header_size = self.variant.group_header_size();
        ensure!(self.remaining() >= header_size, "Truncated group header");
        let typ = self.read_bytes_4();
        ensure!(
            &typ == b"GRUP",
            "Expected GRUP, got {:?}",
            std::str::from_utf8(&typ)
        );
        let total_size = self.read_u32();
        let label = self.read_bytes_4();
        let group_type = self.read_u32();
        // Trailing metadata: Oblivion = 4 bytes (stamp); FO3+ = 8 bytes
        // (stamp + unknown + version + unknown).
        self.skip(header_size - 16);
        Ok(GroupHeader {
            label,
            group_type,
            total_size,
        })
    }

    /// Read the sub-records within a record's data section.
    ///
    /// If the record is compressed (FLAG_COMPRESSED), decompresses first.
    pub fn read_sub_records(&mut self, header: &RecordHeader) -> Result<Vec<SubRecord>> {
        let data_start = self.pos;
        let raw_data = if header.flags & FLAG_COMPRESSED != 0 {
            // First 4 bytes = uncompressed size, rest is zlib.
            ensure!(header.data_size >= 4, "Compressed record too small");
            let decompressed_size = self.read_u32() as usize;
            let compressed_len = header.data_size as usize - 4;
            ensure!(
                self.remaining() >= compressed_len,
                "Truncated compressed data"
            );
            let compressed = &self.data[self.pos..self.pos + compressed_len];
            self.pos += compressed_len;

            // #3399 — `decompressed_size` is the record's own 4-byte
            // prefix: attacker-controlled over the full u32 range, and
            // previously fed straight to `Vec::with_capacity` (a lone
            // `0xFFFFFFFF` bought a 4 GiB reservation) while
            // `read_to_end` inflated with no output limit at all (a
            // 100 MB record at DEFLATE's ~1000:1 reaches ~100 GB and
            // takes the process down on allocation failure — not an
            // `Err` any caller can handle). Bound both against the one
            // quantity the file cannot inflate for free: its own
            // compressed length.
            let ceiling = record_inflation_ceiling(compressed_len);
            ensure!(
                decompressed_size <= ceiling,
                "Compressed record {:08X} declares {} uncompressed bytes from {}                  compressed — over the {} ceiling; plugin is corrupt or hostile",
                header.form_id,
                decompressed_size,
                compressed_len,
                ceiling,
            );
            let mut decompressed = Vec::with_capacity(decompressed_size);
            // Hold the decoder to the (now validated) declared size.
            // `+ 1`: landing on the extra byte proves the stream had
            // more to give, so a prefix that lies *downward* is
            // rejected rather than silently truncating the record.
            // Cannot overflow — `decompressed_size <= ceiling`, which is
            // `MAX_RECORD_INFLATED_BYTES` at most. A *short* decode
            // stays `Ok`, matching `byroredux_bsa::safety::inflate_bounded`
            // (#3410), whose contract this mirrors.
            if let Err(zlib_err) = ZlibDecoder::new(compressed)
                .take(decompressed_size as u64 + 1)
                .read_to_end(&mut decompressed)
            {
                // Some shipped records (e.g. FalloutNV.esm LAND 0x00150FC0,
                // The Strip exterior (-6, 26)) carry a well-formed DEFLATE
                // stream behind a corrupt Adler-32 trailer — the checksum
                // is wrong, not the data. flate2's `ZlibDecoder` validates
                // that trailer and errors even though every byte inflated
                // cleanly. Retry as raw DEFLATE (skip the 2-byte zlib
                // header, no trailer to validate) and accept the recovery
                // ONLY when its length exactly matches the validated
                // declared size — a length mismatch means the stream
                // itself is bad, not just its checksum, so the original
                // zlib error should surface instead. Bethesda's own loader
                // evidently tolerates this exact defect, since the tile
                // renders in the shipped game. (#3720 / D8-01)
                decompressed.clear();
                ensure!(
                    compressed.len() >= 2,
                    "Compressed record {:08X}: zlib decompression failed ({}) and the \
                     stream is too short for a raw-DEFLATE retry",
                    header.form_id,
                    zlib_err,
                );
                let raw_ok = flate2::read::DeflateDecoder::new(&compressed[2..])
                    .take(decompressed_size as u64 + 1)
                    .read_to_end(&mut decompressed)
                    .is_ok()
                    && decompressed.len() == decompressed_size;
                ensure!(
                    raw_ok,
                    "Compressed record {:08X}: zlib decompression failed ({}), and the \
                     raw-DEFLATE Adler-32-mismatch recovery did not reproduce the \
                     declared {} bytes exactly",
                    header.form_id,
                    zlib_err,
                    decompressed_size,
                );
                log::warn!(
                    "ESM record {:08X}: zlib Adler-32 trailer mismatch on an otherwise \
                     well-formed stream, recovered via raw DEFLATE ({decompressed_size} bytes, exact match)",
                    header.form_id,
                );
            }
            ensure!(
                decompressed.len() <= decompressed_size,
                "Compressed record {:08X} inflated past its declared {} bytes                  — plugin is corrupt or hostile (decompression bomb)",
                header.form_id,
                decompressed_size,
            );
            decompressed
        } else {
            let size = header.data_size as usize;
            ensure!(self.remaining() >= size, "Truncated record data");
            let slice = self.data[self.pos..self.pos + size].to_vec();
            self.pos += size;
            slice
        };

        // Parse sub-records from the (possibly decompressed) data.
        let mut sub_pos = 0;
        let mut subs = Vec::new();
        while sub_pos + 6 <= raw_data.len() {
            let sub_type = [
                raw_data[sub_pos],
                raw_data[sub_pos + 1],
                raw_data[sub_pos + 2],
                raw_data[sub_pos + 3],
            ];
            let sub_size =
                u16::from_le_bytes([raw_data[sub_pos + 4], raw_data[sub_pos + 5]]) as usize;
            sub_pos += 6;

            // XXXX extended-size override — used when a sub-record's data
            // exceeds 0xFFFF bytes (e.g. WRLD OFST in FalloutNV.esm). The
            // XXXX sub-record itself carries a 4-byte u32 payload whose
            // value is the TRUE data-byte count for the immediately
            // following sub-record; that record's own 2-byte size field is
            // meaningless and ignored.
            if &sub_type == b"XXXX" {
                if sub_size != 4 || sub_pos + 4 > raw_data.len() {
                    break; // malformed — XXXX payload must be exactly 4 bytes
                }
                let extended_size = u32::from_le_bytes([
                    raw_data[sub_pos],
                    raw_data[sub_pos + 1],
                    raw_data[sub_pos + 2],
                    raw_data[sub_pos + 3],
                ]) as usize;
                sub_pos += 4; // consume the XXXX payload

                // Read the oversized sub-record header (4-byte type + 2-byte
                // size field — the size field is ignored; we use extended_size).
                if sub_pos + 6 > raw_data.len() {
                    break;
                }
                let next_type = [
                    raw_data[sub_pos],
                    raw_data[sub_pos + 1],
                    raw_data[sub_pos + 2],
                    raw_data[sub_pos + 3],
                ];
                sub_pos += 6; // skip type (4 B) + ignored 2-byte size field

                if sub_pos + extended_size > raw_data.len() {
                    break; // truncated oversized sub-record
                }
                let data = raw_data[sub_pos..sub_pos + extended_size].to_vec();
                sub_pos += extended_size;
                subs.push(SubRecord {
                    sub_type: next_type,
                    data,
                });
                continue;
            }

            if sub_pos + sub_size > raw_data.len() {
                // Tolerate truncated final sub-record.
                break;
            }
            let data = raw_data[sub_pos..sub_pos + sub_size].to_vec();
            sub_pos += sub_size;
            subs.push(SubRecord { sub_type, data });
        }

        // Ensure we consumed exactly data_size from the outer stream.
        let consumed = self.pos - data_start;
        if consumed != header.data_size as usize {
            // Adjust position if we over/under-read (shouldn't happen, but defensive).
            self.pos = data_start + header.data_size as usize;
        }

        Ok(subs)
    }

    /// Skip a record's data section without parsing.
    pub fn skip_record(&mut self, header: &RecordHeader) {
        self.pos += header.data_size as usize;
    }

    /// Skip a group's remaining content. `total_size` in the group
    /// header includes the (20- or 24-byte) header that the caller has
    /// already read, so subtract the variant's header size to get the
    /// remaining content length.
    pub fn skip_group(&mut self, header: &GroupHeader) {
        self.pos += self.group_content_len(header);
    }

    /// Remaining content length for a group the caller has just read.
    /// Equivalent to `total_size - group_header_size`, variant-aware.
    pub fn group_content_len(&self, header: &GroupHeader) -> usize {
        (header.total_size as usize).saturating_sub(self.variant.group_header_size())
    }

    /// Absolute byte offset of the end of a group's content. The caller
    /// must have already consumed the group header — `position()` is
    /// expected to sit at the first byte of the content. Replaces the
    /// error-prone `reader.position() + total_size - 24` pattern, which
    /// bakes in a Tes5Plus header size and breaks on Oblivion's 20-byte
    /// group header (#391).
    ///
    /// **A self-recursive walker must not call this** — it has no nesting
    /// bound, so a crafted plugin of nested minimal GRUPs drives the
    /// recursion until the native stack aborts the process (an uncatchable
    /// crash, not a `Result`). Use [`Self::bounded_group_content_end`] and
    /// thread a `depth` parameter, the `_inner(…, depth: u32)` shape every
    /// recursive walker in `esm/` follows. This stays available for the
    /// non-recursive top-level dispatch loops (`records/mod.rs`,
    /// `parse_wrld_group`) and for tests, which enter a group exactly once.
    /// #3237 established the guard; #3503 is what it cost to have eight
    /// walkers keep calling this one.
    pub fn group_content_end(&self, header: &GroupHeader) -> usize {
        self.pos + self.group_content_len(header)
    }

    /// Return the end offset for a group a recursive walker may enter, or
    /// skip its body when the shared nesting ceiling has been reached.
    ///
    /// `depth` is the caller's current recursive depth; the just-read group
    /// would occupy the next level. Centralising the guard keeps every GRUP
    /// walker on the same boundary and makes future recursive walkers harder
    /// to add without noticing the safety contract.
    ///
    /// `parent_end` is the caller's own content-end bound (the `end` every
    /// walker already loops against) — the returned end is clamped to it, so
    /// a corrupt or hostile child GRUP declaring a `total_size` larger than
    /// its parent's remaining content can never make the recursive call walk
    /// past the parent's own boundary into a sibling group or the next
    /// top-level record. The nesting-depth guard above bounds *how deep* a
    /// walker recurses; this bounds *how far* each level can read. (#3721)
    pub(crate) fn bounded_group_content_end(
        &mut self,
        header: &GroupHeader,
        depth: u32,
        parent_end: usize,
        walker: &str,
    ) -> Option<usize> {
        if depth >= MAX_GRUP_NESTING_DEPTH {
            log::warn!(
                "{walker}: skipping GRUP type {} at nesting depth {} (limit {})",
                header.group_type,
                depth.saturating_add(1),
                MAX_GRUP_NESTING_DEPTH,
            );
            self.skip_group(header);
            None
        } else {
            Some(self.group_content_end(header).min(parent_end))
        }
    }

    /// Parse the TES4 file header record.
    pub fn read_file_header(&mut self) -> Result<FileHeader> {
        let header_start = self.position();
        let header = self.read_record_header()?;
        ensure!(
            &header.record_type == b"TES4",
            "ESM file must start with TES4 record, got {:?}",
            std::str::from_utf8(&header.record_type),
        );

        // Bit 0x80 in the TES4 record flags is the "Localized" flag
        // Skyrim+ sets on plugins whose FULL/DESC/etc. sub-records
        // carry 4-byte lstring-table u32 indices instead of inline
        // z-strings. Capture once so downstream record parsers can
        // route string decoding through the lstring helper. See
        // audit S6-03 / #348.
        let localized = header.flags & 0x80 != 0;
        // Bit 0x0200 is the Light Master (ESL) flag — see
        // `FileHeader::light_master`. Captured here so the load-order
        // builder can place the plugin in the 0xFE light space (#1554).
        let light_master = header.flags & 0x0200 != 0;
        let record_version = if self.variant == EsmVariant::Tes5Plus {
            u16::from_le_bytes([self.data[header_start + 20], self.data[header_start + 21]])
        } else {
            0
        };

        let subs = self.read_sub_records(&header)?;
        let mut masters = Vec::new();
        let mut record_count = 0;
        let mut hedr_version = 0.0f32;

        for sub in &subs {
            match &sub.sub_type {
                b"HEDR" if sub.data.len() >= 12 => {
                    hedr_version =
                        f32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                    record_count =
                        u32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]);
                }
                b"MAST" => {
                    // Null-terminated string.
                    let name = sub.data.split(|&b| b == 0).next().unwrap_or(&sub.data);
                    masters.push(String::from_utf8_lossy(name).to_string());
                }
                _ => {}
            }
        }

        Ok(FileHeader {
            master_files: masters,
            record_count,
            hedr_version,
            record_version,
            localized,
            light_master,
        })
    }

    // ── Primitives ──────────────────────────────────────────────────

    fn read_u32(&mut self) -> u32 {
        let v = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        v
    }

    fn read_bytes_4(&mut self) -> [u8; 4] {
        let v = [
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ];
        self.pos += 4;
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic Tes5Plus (24-byte header) record.
    fn build_record(typ: &[u8; 4], form_id: u32, sub_records: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        build_record_for(EsmVariant::Tes5Plus, typ, form_id, sub_records)
    }

    /// Build a synthetic record with explicit variant — Oblivion's
    /// 20-byte header has 4 bytes of vc_info padding where the Tes5Plus
    /// layout has 8.
    fn build_record_for(
        variant: EsmVariant,
        typ: &[u8; 4],
        form_id: u32,
        sub_records: &[(&[u8; 4], &[u8])],
    ) -> Vec<u8> {
        // Build sub-record data first.
        let mut sub_data = Vec::new();
        for (sub_type, data) in sub_records {
            sub_data.extend_from_slice(*sub_type);
            sub_data.extend_from_slice(&(data.len() as u16).to_le_bytes());
            sub_data.extend_from_slice(data);
        }

        let mut buf = Vec::new();
        buf.extend_from_slice(typ);
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes()); // data_size
        buf.extend_from_slice(&0u32.to_le_bytes()); // flags
        buf.extend_from_slice(&form_id.to_le_bytes());
        // Trailing metadata: 4 bytes for Oblivion, 8 for Tes5Plus.
        buf.resize(buf.len() + (variant.record_header_size() - 16), 0);
        buf.extend_from_slice(&sub_data);
        buf
    }

    /// Build a synthetic Tes5Plus (24-byte header) group.
    fn build_group(label: &[u8; 4], group_type: u32, content: &[u8]) -> Vec<u8> {
        build_group_for(EsmVariant::Tes5Plus, label, group_type, content)
    }

    fn build_group_for(
        variant: EsmVariant,
        label: &[u8; 4],
        group_type: u32,
        content: &[u8],
    ) -> Vec<u8> {
        let total_size = variant.group_header_size() + content.len();
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GRUP");
        buf.extend_from_slice(&(total_size as u32).to_le_bytes());
        buf.extend_from_slice(label);
        buf.extend_from_slice(&group_type.to_le_bytes());
        buf.resize(buf.len() + (variant.group_header_size() - 16), 0);
        buf.extend_from_slice(content);
        buf
    }

    /// Regression: #439 / FO3-3-01. Pin the HEDR → GameKind mapping
    /// against real vanilla master values sampled from disk on
    /// 2026-04-19. Pre-fix FO3's 0.94 routed to Fallout4 and FO4's 1.0
    /// fell through to Fallout3NV, inverting the FO3↔FO4
    /// classification.
    /// Regression: #445 — single-plugin loads (no masters, self-index 0)
    /// must be an exact identity remap so the existing CLI behaviour
    /// and every existing consumer keep seeing the same u32 FormIDs.
    #[test]
    fn form_id_remap_single_plugin_is_identity() {
        let remap = FormIdRemap::regular(0, Vec::new());
        // Self-references (top byte = 0 = master_count) pass through.
        assert_eq!(remap.remap(0x0001_2345), 0x0001_2345);
        assert_eq!(remap.remap(0x00CA_FEBA), 0x00CA_FEBA);
    }

    #[test]
    fn form_id_remap_preserves_null_for_light_plugins() {
        let remap = FormIdRemap {
            plugin_slot: GlobalSlot::Light(7),
            master_slots: Vec::new(),
        };
        assert_eq!(remap.remap(0), 0, "NULL must not become 0xFE00_7000");
    }

    /// #1308 / OBL-D6-NEW-04 — vanilla `Oblivion.esm` loaded standalone
    /// (no masters) ships a handful of forms prefixed `0x01`, a Bethesda
    /// authoring artifact. The remap can't resolve them (no plugin at that
    /// global slot), so it passes them through unchanged — NOT clamped to
    /// self, which could collide distinct forms in `EsmIndex` — and logs at
    /// debug rather than spamming warn per form. This pins the pass-through
    /// contract so a future "clamp" refactor can't land silently.
    #[test]
    fn form_id_remap_standalone_out_of_range_passes_through() {
        let remap = FormIdRemap::regular(0, Vec::new());
        // mod_index 0x01 with no masters — the Oblivion artifact. Verbatim
        // pass-through, not clamped to 0x00.
        assert_eq!(remap.remap(0x0100_0ABC), 0x0100_0ABC);
        // A higher out-of-range index behaves identically.
        assert_eq!(remap.remap(0x0512_3456), 0x0512_3456);
        // The in-range self-reference (mod_index 0 == master count) is
        // unaffected by the out-of-range arm.
        assert_eq!(remap.remap(0x0000_0001), 0x0000_0001);
    }

    /// Two-plugin load: Anchorage.esm depends on Fallout3.esm.
    /// Anchorage's own new forms land in the global slot for Anchorage
    /// (plugin_index=1), and its references to Fallout3 statics pass
    /// through unchanged (mod_index 0 → master_indices[0] = 0).
    #[test]
    fn form_id_remap_dlc_on_base_routes_self_and_master_correctly() {
        // Anchorage.esm loaded at plugin_index=1 with Fallout3.esm as master 0.
        let remap = FormIdRemap::regular(1, vec![0]);
        // Reference to Fallout3 (mod_index=0, master_indices[0]=0) → unchanged.
        assert_eq!(remap.remap(0x0001_2345), 0x0001_2345);
        // Self-reference (mod_index=1 == master count) → plugin_index=1.
        assert_eq!(remap.remap(0x0101_2345), 0x0101_2345);
    }

    /// Three-plugin collision scenario from the issue: Anchorage and
    /// BrokenSteel both ship form 0x01_012345 in-file. Remapping by
    /// load-order prevents the collision in shared HashMaps.
    #[test]
    fn form_id_remap_two_dlcs_resolve_collision() {
        // Anchorage.esm loaded at plugin_index=1, masters=[0 (Fallout3)].
        let anchorage = FormIdRemap::regular(1, vec![0]);
        // BrokenSteel.esm loaded at plugin_index=2, masters=[0 (Fallout3)].
        let broken_steel = FormIdRemap::regular(2, vec![0]);

        // Both files ship their own 0x01_012345 — the audit's canonical
        // example. Under remap they land in distinct global slots.
        let anchorage_form = anchorage.remap(0x0101_2345);
        let broken_steel_form = broken_steel.remap(0x0101_2345);
        assert_eq!(anchorage_form, 0x0101_2345);
        assert_eq!(broken_steel_form, 0x0201_2345);
        assert_ne!(
            anchorage_form, broken_steel_form,
            "DLC self-refs must not collide after remap — this is the #445 regression"
        );

        // Both files' references to Fallout3 base forms remain unchanged.
        assert_eq!(anchorage.remap(0x00CA_FEBA), 0x00CA_FEBA);
        assert_eq!(broken_steel.remap(0x00CA_FEBA), 0x00CA_FEBA);
    }

    /// A plugin loaded at plugin_index=3 that only declares Fallout3
    /// as master (master_indices=[0]) — self-refs use mod_index=1
    /// in-file but must remap to the plugin's actual load-order slot 3.
    /// Catches the case where the file's MAST count is smaller than
    /// the load-order index.
    #[test]
    fn form_id_remap_rewrites_self_ref_to_load_order_index() {
        let remap = FormIdRemap::regular(3, vec![0]);
        // mod_index 1 == num_masters → self → plugin_index=3.
        assert_eq!(remap.remap(0x0112_3456), 0x0312_3456);
    }

    /// Record header read returns the remapped FormID on any installed
    /// remap — the integration point where the fix actually lands.
    #[test]
    fn read_record_header_applies_installed_remap() {
        let data = build_record(b"STAT", 0x0112_3456, &[]);
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        reader.set_form_id_remap(FormIdRemap::regular(2, vec![0]));
        let header = reader.read_record_header().unwrap();
        assert_eq!(
            header.form_id, 0x0212_3456,
            "read_record_header must route the top byte through set_form_id_remap (#445)"
        );
    }

    /// Bottom 24 bits of every FormID must be preserved verbatim — only
    /// the mod-index byte changes.
    #[test]
    fn form_id_remap_preserves_local_24_bits() {
        let remap = FormIdRemap::regular(5, vec![0, 1]);
        let local = 0x00AB_CDEF;
        assert_eq!(remap.remap(local) & 0x00FF_FFFF, local);
        assert_eq!(remap.remap(0x01CD_EF12) & 0x00FF_FFFF, 0x00CD_EF12);
        assert_eq!(remap.remap(0x02CD_EF12) & 0x00FF_FFFF, 0x00CD_EF12);
    }

    /// #1554 — `GlobalSlot::compose` decode. Regular slots keep the
    /// 24-bit object id; Light (ESL) slots pack a 12-bit sub-index into
    /// the 0xFE space and keep only the 12-bit ESL object id.
    #[test]
    fn global_slot_compose_decodes_regular_and_light() {
        // Regular: byte index in the top byte, 24-bit object id preserved.
        assert_eq!(GlobalSlot::Regular(3).compose(0x00AB_CDEF), 0x03AB_CDEF);
        // Light: 0xFE | (sub << 12) | (object & 0xFFF).
        assert_eq!(GlobalSlot::Light(5).compose(0x0000_0ABC), 0xFE00_5ABC);
        // ESL object space is 12 bits — anything above 0xFFF is masked off
        // (the higher bits belong to the 12-bit sub-index window).
        assert_eq!(GlobalSlot::Light(0).compose(0x0000_1FFF), 0xFE00_0FFF);
        // Max sub-index (0xFFF) and max object id (0xFFF) fill the space.
        assert_eq!(GlobalSlot::Light(0xFFF).compose(0x0000_0FFF), 0xFEFF_FFFF);
    }

    /// #1554 / SK-D4-03 — an ESL plugin's OWN forms (self-reference,
    /// mod_index == master count) decode into the 0xFE light space at the
    /// plugin's 12-bit sub-index, NOT a flat top-byte index. References to
    /// the ESL's regular masters are unaffected.
    #[test]
    fn form_id_remap_esl_plugin_own_forms_decode_into_light_space() {
        // An ESL at light sub-index 2 with one regular master (byte 0).
        let remap = FormIdRemap {
            plugin_slot: GlobalSlot::Light(2),
            master_slots: vec![GlobalSlot::Regular(0)],
        };
        // Self-ref (mod_index 1 == master count): raw 0x0100_0800 →
        // 0xFE | sub 2 | object 0x800.
        assert_eq!(remap.remap(0x0100_0800), 0xFE00_2800);
        // Reference to the regular master (mod_index 0) stays a byte-0 form.
        assert_eq!(remap.remap(0x0001_2345), 0x0001_2345);
    }

    /// #1554 / SK-D4-03 — a reference FROM any plugin TO an ESL master
    /// decodes into that master's 0xFE light sub-index. This is the
    /// per-master half the flat-top-byte remap got wrong.
    #[test]
    fn form_id_remap_reference_to_esl_master_decodes_into_light_space() {
        // A regular plugin (byte 1) whose master[0] is an ESL at sub 5.
        let remap = FormIdRemap {
            plugin_slot: GlobalSlot::Regular(1),
            master_slots: vec![GlobalSlot::Light(5)],
        };
        // Reference to the ESL master (mod_index 0) → 0xFE | sub 5 | 0xABC.
        assert_eq!(remap.remap(0x0000_0ABC), 0xFE00_5ABC);
        // Self-ref (mod_index 1 == master count) → regular byte 1.
        assert_eq!(remap.remap(0x0112_3456), 0x0112_3456);
    }

    /// #1554 — `read_file_header` decodes both the Localized (0x80) and
    /// Light-Master (0x0200) TES4 flags independently.
    #[test]
    fn read_file_header_reads_localized_and_light_master_flags() {
        // Minimal TES4 (Tes5Plus 24-byte header) carrying `flags` + HEDR.
        fn tes4_with_flags(flags: u32) -> Vec<u8> {
            let mut hedr = Vec::new();
            hedr.extend_from_slice(&1.7f32.to_le_bytes()); // version
            hedr.extend_from_slice(&0u32.to_le_bytes()); // record_count
            hedr.extend_from_slice(&0u32.to_le_bytes()); // next_object_id
            let mut subs = Vec::new();
            subs.extend_from_slice(b"HEDR");
            subs.extend_from_slice(&(hedr.len() as u16).to_le_bytes());
            subs.extend_from_slice(&hedr);
            let mut buf = Vec::new();
            buf.extend_from_slice(b"TES4");
            buf.extend_from_slice(&(subs.len() as u32).to_le_bytes());
            buf.extend_from_slice(&flags.to_le_bytes());
            buf.extend_from_slice(&0u32.to_le_bytes()); // form_id
            buf.extend_from_slice(&[0u8; 8]); // trailer
            buf.extend_from_slice(&subs);
            buf
        }
        let read = |flags: u32| {
            EsmReader::with_variant(&tes4_with_flags(flags), EsmVariant::Tes5Plus)
                .read_file_header()
                .unwrap()
        };
        let plain = read(0x0000);
        assert!(!plain.localized && !plain.light_master);
        let localized = read(0x0080);
        assert!(localized.localized && !localized.light_master);
        let esl = read(0x0200);
        assert!(!esl.localized && esl.light_master);
        let both = read(0x0280);
        assert!(both.localized && both.light_master);
    }

    #[test]
    fn game_kind_from_header_maps_real_master_hedr_values() {
        // FO3 GOTY — bytes d7 a3 70 3f → f32 0.94.
        assert_eq!(
            GameKind::from_header(EsmVariant::Tes5Plus, 0.94, 2),
            GameKind::Fallout3NV,
            "FO3 GOTY (HEDR=0.94) must classify as Fallout3NV",
        );
        // FNV — bytes 1f 85 ab 3f → f32 ≈ 1.34.
        assert_eq!(
            GameKind::from_header(EsmVariant::Tes5Plus, 1.34, 2),
            GameKind::Fallout3NV,
            "FNV (HEDR=1.34) must classify as Fallout3NV",
        );
        // FO4 — bytes 00 00 80 3f → f32 1.0.
        assert_eq!(
            GameKind::from_header(EsmVariant::Tes5Plus, 1.0, 131),
            GameKind::Fallout4,
            "FO4 (HEDR=1.0) must classify as Fallout4",
        );
        // Skyrim SE — bytes 48 e1 da 3f → f32 ≈ 1.71.
        assert_eq!(
            GameKind::from_header(EsmVariant::Tes5Plus, 1.71, 44),
            GameKind::Skyrim,
            "Skyrim SE (HEDR=1.71) must classify as Skyrim",
        );
        // Starfield — bytes 8f c2 75 3f → f32 ≈ 0.96.
        assert_eq!(
            GameKind::from_header(EsmVariant::Tes5Plus, 0.96, 581),
            GameKind::Starfield,
            "Starfield (HEDR=0.96) must classify as Starfield",
        );
        // FO76 — SeventySix.esm and NW.esm both ship HEDR bytes
        // 00 00 85 43 (266.0) with TES4 record version 209, read off disk
        // 2026-08-28. The old assertion used 68.0, a value no installed
        // master carries; keep a second probe at that height so the band
        // floor stays pinned well below the real value (#3405).
        assert_eq!(
            GameKind::from_header(EsmVariant::Tes5Plus, 266.0, 209),
            GameKind::Fallout76,
            "FO76 (real HEDR=266.0, rec_ver=209) must classify as Fallout76",
        );
        assert_eq!(
            GameKind::from_header(EsmVariant::Tes5Plus, 68.0, 155),
            GameKind::Fallout76,
            "the >=60.0 FO76 floor must stay far below the real 266.0",
        );
        // Oblivion — variant-dispatched regardless of HEDR.
        assert_eq!(
            GameKind::from_header(EsmVariant::Oblivion, 1.0, 0),
            GameKind::Oblivion,
        );
        // DLCCoast.esm ships these exact header discriminators:
        // HEDR bytes 33 33 73 3f (0.95), TES4 version 0x0083 (131).
        assert_eq!(
            GameKind::from_header(EsmVariant::Tes5Plus, 0.95, 131),
            GameKind::Fallout4,
            "FO4 CK/DLC HEDR=0.95 must not fall into the FO3/FNV family",
        );
    }

    #[test]
    fn read_record_header_basic() {
        let data = build_record(b"STAT", 0x12345, &[]);
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        assert_eq!(&header.record_type, b"STAT");
        assert_eq!(header.form_id, 0x12345);
        assert_eq!(header.data_size, 0);
    }

    #[test]
    fn read_sub_records() {
        let data = build_record(
            b"STAT",
            0x100,
            &[(b"EDID", b"TestStatic\0"), (b"MODL", b"meshes\\test.nif\0")],
        );
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        let subs = reader.read_sub_records(&header).unwrap();

        assert_eq!(subs.len(), 2);
        assert_eq!(&subs[0].sub_type, b"EDID");
        assert_eq!(&subs[0].data, b"TestStatic\0");
        assert_eq!(&subs[1].sub_type, b"MODL");
        assert_eq!(&subs[1].data, b"meshes\\test.nif\0");
    }

    /// Regression for #1347 / D2-02 — the XXXX extended-size override must
    /// be handled: when a sub-record's data exceeds 0xFFFF bytes (e.g. the
    /// WRLD OFST in FalloutNV.esm at 177 KB), a preceding `XXXX` sub-record
    /// carries the true u32 size; the following record's 2-byte size field is
    /// ignored. The subsequent sub-records after the oversized one must still
    /// parse correctly (stream position must be exact).
    #[test]
    fn read_sub_records_handles_xxxx_extended_size() {
        // Build the sub-record payload manually so we control the exact bytes.
        let extended_size: u32 = 70_000; // > 0xFFFF, the interesting case
        let mut sub_data: Vec<u8> = Vec::new();

        // 1. Normal EDID sub-record before the XXXX.
        let edid_bytes = b"Test\0";
        sub_data.extend_from_slice(b"EDID");
        sub_data.extend_from_slice(&(edid_bytes.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(edid_bytes);

        // 2. XXXX sub-record: type=XXXX, 2-byte size=4, payload=extended_size.
        sub_data.extend_from_slice(b"XXXX");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&extended_size.to_le_bytes());

        // 3. Oversized OFST sub-record: type=OFST, 2-byte size=ignored,
        //    data=extended_size bytes (zeros).
        sub_data.extend_from_slice(b"OFST");
        sub_data.extend_from_slice(&0u16.to_le_bytes()); // ignored
        sub_data.extend(std::iter::repeat_n(0u8, extended_size as usize));

        // 4. Normal CNAM sub-record after the oversized one — verifies stream
        //    position is correct so this can be read cleanly.
        sub_data.extend_from_slice(b"CNAM");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDDu8]);

        // Wrap in a record header (Tes5Plus = 24-byte header).
        let mut record: Vec<u8> = Vec::new();
        record.extend_from_slice(b"WRLD");
        record.extend_from_slice(&(sub_data.len() as u32).to_le_bytes()); // data_size
        record.extend_from_slice(&0u32.to_le_bytes()); // flags
        record.extend_from_slice(&1u32.to_le_bytes()); // form_id
        record.extend(std::iter::repeat_n(0u8, 8)); // 8-byte Tes5Plus trailer
        record.extend_from_slice(&sub_data);

        let mut reader = EsmReader::with_variant(&record, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        let subs = reader.read_sub_records(&header).unwrap();

        assert_eq!(subs.len(), 3, "EDID + OFST (via XXXX) + CNAM");
        assert_eq!(&subs[0].sub_type, b"EDID");
        assert_eq!(subs[0].data, edid_bytes);
        assert_eq!(&subs[1].sub_type, b"OFST");
        assert_eq!(
            subs[1].data.len(),
            extended_size as usize,
            "OFST data must be exactly extended_size bytes"
        );
        assert_eq!(&subs[2].sub_type, b"CNAM");
        assert_eq!(subs[2].data, [0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn read_group_header_basic() {
        let group = build_group(b"CELL", 0, &[]);
        let mut reader = EsmReader::with_variant(&group, EsmVariant::Tes5Plus);
        let header = reader.read_group_header().unwrap();
        assert_eq!(&header.label, b"CELL");
        assert_eq!(header.group_type, 0);
        assert_eq!(header.total_size, 24);
    }

    #[test]
    fn is_group_detects_grup() {
        let group = build_group(b"CELL", 0, &[]);
        let reader = EsmReader::with_variant(&group, EsmVariant::Tes5Plus);
        assert!(reader.is_group());

        let record = build_record(b"STAT", 0, &[]);
        let reader = EsmReader::with_variant(&record, EsmVariant::Tes5Plus);
        assert!(!reader.is_group());
    }

    #[test]
    fn skip_record_advances_position() {
        let data = build_record(b"STAT", 0, &[(b"EDID", b"Test\0")]);
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        reader.skip_record(&header);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn file_header_parses_tes4() {
        let tes4 = build_record(
            b"TES4",
            0,
            &[
                // HEDR: version(f32) + record_count(u32) + next_object_id(u32)
                (b"HEDR", &{
                    let mut d = Vec::new();
                    d.extend_from_slice(&1.0f32.to_le_bytes()); // version
                    d.extend_from_slice(&42u32.to_le_bytes()); // record count
                    d.extend_from_slice(&0u32.to_le_bytes()); // next object id
                    d
                }),
                (b"MAST", b"FalloutNV.esm\0"),
                (b"DATA", &0u64.to_le_bytes()),
            ],
        );
        let mut reader = EsmReader::with_variant(&tes4, EsmVariant::Tes5Plus);
        let fh = reader.read_file_header().unwrap();
        assert_eq!(fh.record_count, 42);
        assert_eq!(fh.master_files, vec!["FalloutNV.esm"]);
    }

    // ── Oblivion (20-byte header) tests ────────────────────────────────

    #[test]
    fn variant_detect_oblivion() {
        // Build a real Oblivion TES4 record: 20-byte header + HEDR subrecord.
        let tes4 = build_record_for(
            EsmVariant::Oblivion,
            b"TES4",
            0,
            &[(b"HEDR", &{
                let mut d = Vec::new();
                d.extend_from_slice(&1.0f32.to_le_bytes()); // Oblivion version = 1.0
                d.extend_from_slice(&0u32.to_le_bytes()); // record count
                d.extend_from_slice(&0u32.to_le_bytes()); // next object id
                d
            })],
        );
        // At offset 20 we should see "HEDR" — the sub-record type.
        assert_eq!(&tes4[20..24], b"HEDR");
        assert_eq!(EsmVariant::detect(&tes4), EsmVariant::Oblivion);
    }

    #[test]
    fn variant_detect_tes5_plus() {
        // FNV-style TES4 — HEDR lands at offset 24.
        let tes4 = build_record_for(
            EsmVariant::Tes5Plus,
            b"TES4",
            0,
            &[(b"HEDR", b"placeholder\0")],
        );
        assert_eq!(&tes4[24..28], b"HEDR");
        assert_eq!(EsmVariant::detect(&tes4), EsmVariant::Tes5Plus);
    }

    #[test]
    fn read_oblivion_record_header_has_20_byte_layout() {
        let data = build_record_for(EsmVariant::Oblivion, b"STAT", 0xAB, &[]);
        assert_eq!(data.len(), 20); // no sub-records → 20 header bytes total
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Oblivion);
        let header = reader.read_record_header().unwrap();
        assert_eq!(&header.record_type, b"STAT");
        assert_eq!(header.form_id, 0xAB);
        assert_eq!(header.data_size, 0);
        assert_eq!(reader.position(), 20);
    }

    #[test]
    fn read_oblivion_group_header_has_20_byte_layout() {
        let group = build_group_for(EsmVariant::Oblivion, b"CELL", 0, &[]);
        assert_eq!(group.len(), 20);
        let mut reader = EsmReader::with_variant(&group, EsmVariant::Oblivion);
        let header = reader.read_group_header().unwrap();
        assert_eq!(&header.label, b"CELL");
        assert_eq!(header.total_size, 20);
        assert_eq!(reader.position(), 20);
    }

    #[test]
    fn read_oblivion_sub_records() {
        let data = build_record_for(
            EsmVariant::Oblivion,
            b"STAT",
            0x100,
            &[
                (b"EDID", b"TestOblivion\0"),
                (b"MODL", b"meshes\\stat.nif\0"),
            ],
        );
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Oblivion);
        let header = reader.read_record_header().unwrap();
        let subs = reader.read_sub_records(&header).unwrap();
        assert_eq!(subs.len(), 2);
        assert_eq!(&subs[0].sub_type, b"EDID");
        assert_eq!(subs[0].data, b"TestOblivion\0");
        assert_eq!(&subs[1].sub_type, b"MODL");
    }

    #[test]
    fn oblivion_skip_group_uses_20_byte_header() {
        // Group containing one STAT record. skip_group should land exactly
        // at end-of-buffer — off-by-4 bugs show up here.
        let inner = build_record_for(EsmVariant::Oblivion, b"STAT", 1, &[]);
        let group = build_group_for(EsmVariant::Oblivion, b"STAT", 0, &inner);
        let mut reader = EsmReader::with_variant(&group, EsmVariant::Oblivion);
        let header = reader.read_group_header().unwrap();
        reader.skip_group(&header);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn group_content_end_is_variant_aware() {
        // Regression: #391 — the walker used to hardcode `-24` when
        // deriving a group's content end, which over-read by 4 bytes on
        // Oblivion (20-byte group header). The helper on the reader
        // must produce different absolute positions for the two
        // variants given identical `total_size` payload.
        let inner_tes5 = build_record_for(EsmVariant::Tes5Plus, b"STAT", 1, &[]);
        let group_tes5 = build_group_for(EsmVariant::Tes5Plus, b"STAT", 0, &inner_tes5);
        let mut r5 = EsmReader::with_variant(&group_tes5, EsmVariant::Tes5Plus);
        let gh5 = r5.read_group_header().unwrap();
        assert_eq!(
            r5.group_content_end(&gh5),
            group_tes5.len(),
            "Tes5Plus: end of content must equal buffer length"
        );
        assert_eq!(r5.group_content_len(&gh5), group_tes5.len() - 24);

        let inner_obl = build_record_for(EsmVariant::Oblivion, b"STAT", 1, &[]);
        let group_obl = build_group_for(EsmVariant::Oblivion, b"STAT", 0, &inner_obl);
        let mut ro = EsmReader::with_variant(&group_obl, EsmVariant::Oblivion);
        let gho = ro.read_group_header().unwrap();
        assert_eq!(
            ro.group_content_end(&gho),
            group_obl.len(),
            "Oblivion: end of content must equal buffer length (20-byte header)"
        );
        assert_eq!(ro.group_content_len(&gho), group_obl.len() - 20);

        // Same payload, different header sizes — the variant decides.
        let fake_header = GroupHeader {
            label: *b"STAT",
            group_type: 0,
            total_size: 100,
        };
        let r5 = EsmReader::with_variant(&[], EsmVariant::Tes5Plus);
        let ro = EsmReader::with_variant(&[], EsmVariant::Oblivion);
        assert_eq!(r5.group_content_len(&fake_header), 76); // 100 - 24
        assert_eq!(ro.group_content_len(&fake_header), 80); // 100 - 20
    }

    /// #3721 — a nested group declaring a `total_size` larger than its
    /// parent's remaining content must be clamped to the parent's own end,
    /// not allowed to walk past it into a sibling group or the next
    /// top-level record.
    #[test]
    fn bounded_group_content_end_clamps_to_parent_end() {
        let mut reader = EsmReader::with_variant(&[], EsmVariant::Tes5Plus);
        reader.skip(100); // simulate having just read a nested group header at offset 100

        // Declares far more content than the parent has left: natural end
        // would be 100 + (10000 - 24) = 10076, but the parent only extends
        // to 200.
        let overrunning = GroupHeader {
            label: *b"CELL",
            group_type: 0,
            total_size: 10000,
        };
        let parent_end = 200;
        assert_eq!(
            reader.bounded_group_content_end(&overrunning, 0, parent_end, "test"),
            Some(parent_end),
            "an overrunning child GRUP must be clamped to the parent's own end"
        );

        // A well-formed child that fits comfortably inside its parent must
        // be unaffected by the clamp — its own natural end is returned.
        let well_formed = GroupHeader {
            label: *b"CELL",
            group_type: 0,
            total_size: 50,
        };
        let natural_end = reader.group_content_end(&well_formed);
        assert!(
            natural_end < parent_end,
            "test fixture sanity: well-formed group must not already overrun"
        );
        assert_eq!(
            reader.bounded_group_content_end(&well_formed, 0, parent_end, "test"),
            Some(natural_end),
            "a group that fits inside its parent must not be shrunk by the clamp"
        );
    }

    #[test]
    fn oblivion_file_header_parses() {
        let tes4 = build_record_for(
            EsmVariant::Oblivion,
            b"TES4",
            0,
            &[
                (b"HEDR", &{
                    let mut d = Vec::new();
                    d.extend_from_slice(&1.0f32.to_le_bytes()); // Oblivion v1.0
                    d.extend_from_slice(&123u32.to_le_bytes()); // record count
                    d.extend_from_slice(&0u32.to_le_bytes());
                    d
                }),
                (b"MAST", b"Oblivion.esm\0"),
            ],
        );
        let mut reader = EsmReader::new(&tes4); // auto-detect
        assert_eq!(reader.variant(), EsmVariant::Oblivion);
        let fh = reader.read_file_header().unwrap();
        assert_eq!(fh.record_count, 123);
        assert_eq!(fh.master_files, vec!["Oblivion.esm"]);
    }

    /// M41.0 Phase 1a — face/animation predicates are exhaustive
    /// across the 6 supported `GameKind` variants and partition them
    /// cleanly: every game uses **either** a runtime FaceGen recipe
    /// **or** a pre-baked NIF (no double-counting); same for `.kf` vs
    /// `.hkx`. A new game variant must extend both `match` arms in
    /// one place to satisfy this test.
    #[test]
    fn game_kind_face_animation_predicates_partition_cleanly() {
        let all = [
            GameKind::Oblivion,
            GameKind::Fallout3NV,
            GameKind::Skyrim,
            GameKind::Fallout4,
            GameKind::Fallout76,
            GameKind::Starfield,
        ];
        for g in all {
            assert!(
                g.has_runtime_facegen_recipe() ^ g.uses_prebaked_facegen(),
                "{g:?}: must satisfy exactly one of runtime / prebaked FaceGen",
            );
            assert!(
                g.has_kf_animations() ^ g.has_havok_animations(),
                "{g:?}: must satisfy exactly one of kf / hkx animation",
            );
        }

        // Spot-check the membership of each set so a future
        // misclassification fails loudly here, not at spawn time.
        assert!(GameKind::Fallout3NV.has_runtime_facegen_recipe());
        assert!(GameKind::Oblivion.has_runtime_facegen_recipe());
        assert!(GameKind::Skyrim.uses_prebaked_facegen());
        assert!(GameKind::Fallout4.uses_prebaked_facegen());
        assert!(GameKind::Starfield.uses_prebaked_facegen());

        assert!(GameKind::Fallout3NV.has_kf_animations());
        assert!(GameKind::Skyrim.has_havok_animations());
        assert!(GameKind::Starfield.has_havok_animations());
    }

    // ── Compressed record tests (#990 / SK-D6-NEW-02) ──────────────────
    //
    // `read_sub_records` had zero unit test coverage for the FLAG_COMPRESSED
    // branch. These tests guard against: backend swaps (flate2 → miniz_oxide),
    // decompressed-size prefix off-by-one, the data_size < 4 panic path, and
    // silent byte-order changes in read_u32.

    /// Build a raw sub-record payload byte-vector (same layout `read_sub_records`
    /// parses) without going through a full record header.
    fn build_sub_record_payload(sub_records: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut payload = Vec::new();
        for (sub_type, data) in sub_records {
            payload.extend_from_slice(*sub_type);
            payload.extend_from_slice(&(data.len() as u16).to_le_bytes());
            payload.extend_from_slice(data);
        }
        payload
    }

    /// Build a compressed record: zlib-encode the sub-record payload, prepend
    /// the 4-byte decompressed-size header, and wrap in a Tes5Plus record with
    /// FLAG_COMPRESSED set.
    fn build_compressed_record(
        typ: &[u8; 4],
        form_id: u32,
        sub_records: &[(&[u8; 4], &[u8])],
    ) -> Vec<u8> {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let payload = build_sub_record_payload(sub_records);
        let decompressed_size = payload.len() as u32;

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&payload).expect("zlib encode");
        let compressed = encoder.finish().expect("zlib finish");

        // data_size = 4 (decompressed-size prefix) + compressed_len.
        let data_size = (4 + compressed.len()) as u32;

        let mut buf = Vec::new();
        buf.extend_from_slice(typ); // record type
        buf.extend_from_slice(&data_size.to_le_bytes()); // data_size
        buf.extend_from_slice(&FLAG_COMPRESSED.to_le_bytes()); // flags
        buf.extend_from_slice(&form_id.to_le_bytes()); // form_id
                                                       // Tes5Plus trailing 8 bytes (vc_info + revision + version + unknown).
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&decompressed_size.to_le_bytes()); // 4-byte prefix
        buf.extend_from_slice(&compressed);
        buf
    }

    /// Happy path: a compressed STAT record with two sub-records round-trips
    /// correctly through `read_sub_records`.
    #[test]
    fn compressed_record_round_trips_sub_records() {
        let data = build_compressed_record(
            b"STAT",
            0x200,
            &[
                (b"EDID", b"TreeLOD\0"),
                (b"MODL", b"meshes\\tree_lod.nif\0"),
            ],
        );
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();

        // FLAG_COMPRESSED must be visible on the header.
        assert_ne!(
            header.flags & FLAG_COMPRESSED,
            0,
            "FLAG_COMPRESSED must be set on the header"
        );

        let subs = reader.read_sub_records(&header).unwrap();

        assert_eq!(subs.len(), 2);
        assert_eq!(&subs[0].sub_type, b"EDID");
        assert_eq!(subs[0].data, b"TreeLOD\0");
        assert_eq!(&subs[1].sub_type, b"MODL");
        assert_eq!(subs[1].data, b"meshes\\tree_lod.nif\0");
    }

    /// Decompressed size embedded in the 4-byte prefix must match the actual
    /// decompressed content length. This test verifies that the capacity hint
    /// (`Vec::with_capacity(decompressed_size)`) is correct — a mismatch here
    /// would panic or silently truncate on strict allocators.
    #[test]
    fn compressed_record_prefix_matches_payload_length() {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let payload = build_sub_record_payload(&[(b"FULL", b"Hello world\0")]);
        let expected_len = payload.len();
        let decompressed_size = payload.len() as u32;

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let data_size = (4 + compressed.len()) as u32;
        let mut buf = Vec::new();
        buf.extend_from_slice(b"MISC");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.extend_from_slice(&FLAG_COMPRESSED.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes()); // form_id
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&decompressed_size.to_le_bytes());
        buf.extend_from_slice(&compressed);

        let mut reader = EsmReader::with_variant(&buf, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        let subs = reader.read_sub_records(&header).unwrap();

        assert_eq!(subs.len(), 1);
        assert_eq!(
            subs[0].data.len() + 6,
            expected_len, // +6 for sub-type + length prefix
            "decompressed payload length must match the 4-byte prefix"
        );
    }

    /// #3720 — a well-formed zlib stream whose 4-byte Adler-32 trailer has
    /// been corrupted (the DEFLATE payload itself is untouched) must still
    /// decode: `ZlibDecoder` rejects the checksum, but the raw-DEFLATE
    /// retry recovers the exact declared byte count. Regression fixture
    /// for `FalloutNV.esm` LAND `0x00150FC0` (The Strip, exterior -6,26),
    /// whose real trailer is corrupt in exactly this way.
    #[test]
    fn compressed_record_with_corrupt_adler32_trailer_recovers_via_raw_deflate() {
        let mut data = build_compressed_record(
            b"LAND",
            0x00150FC0,
            &[(b"DATA", &[0u8; 4]), (b"VHGT", &[0u8; 8])],
        );
        // Flip the last 4 bytes of the compressed stream — the zlib
        // Adler-32 trailer sits at the very end, after the DEFLATE data.
        let len = data.len();
        for byte in &mut data[len - 4..] {
            *byte ^= 0xFF;
        }

        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        let subs = reader
            .read_sub_records(&header)
            .expect("a checksum-only failure must still recover via raw DEFLATE");

        assert_eq!(subs.len(), 2);
        assert_eq!(&subs[0].sub_type, b"DATA");
        assert_eq!(subs[0].data, [0u8; 4]);
        assert_eq!(&subs[1].sub_type, b"VHGT");
        assert_eq!(subs[1].data, [0u8; 8]);
    }

    /// #3720 sibling: corrupting the DEFLATE *body* (not just the trailer)
    /// must still be a hard error — the raw-DEFLATE retry only masks a bad
    /// checksum on an otherwise-correct stream, never a genuinely corrupt
    /// one. A body-level corruption changes the decoded length (or fails
    /// outright), so the retry's exact-length check rejects it.
    #[test]
    fn compressed_record_with_corrupt_deflate_body_still_errors() {
        let mut data = build_compressed_record(
            b"LAND",
            0x00150FC1,
            &[(b"DATA", &[0u8; 4]), (b"VHGT", &[0u8; 8])],
        );
        // Corrupt a byte well inside the DEFLATE stream, not the trailing
        // Adler-32 — the compressed payload starts after the 24-byte
        // Tes5Plus header + 4-byte decompressed-size prefix.
        let compressed_start = 24 + 4;
        data[compressed_start + 2] ^= 0xFF;

        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        assert!(
            reader.read_sub_records(&header).is_err(),
            "a genuinely corrupt DEFLATE body must not be silently recovered"
        );
    }

    /// Build a compressed record whose 4-byte prefix is a lie: `declared`
    /// replaces the true payload length, everything else stays well-formed.
    fn build_compressed_record_with_prefix(
        typ: &[u8; 4],
        form_id: u32,
        declared: u32,
        sub_records: &[(&[u8; 4], &[u8])],
    ) -> Vec<u8> {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let payload = build_sub_record_payload(sub_records);
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&payload).expect("zlib encode");
        let compressed = encoder.finish().expect("zlib finish");

        let mut buf = Vec::new();
        buf.extend_from_slice(typ);
        buf.extend_from_slice(&((4 + compressed.len()) as u32).to_le_bytes());
        buf.extend_from_slice(&FLAG_COMPRESSED.to_le_bytes());
        buf.extend_from_slice(&form_id.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&declared.to_le_bytes());
        buf.extend_from_slice(&compressed);
        buf
    }

    /// #3399 (a) — a prefix that lies *upward* must be rejected before it
    /// reaches `Vec::with_capacity`.
    ///
    /// The prefix is entirely file-controlled over the full `u32` range and
    /// used to go straight into the reservation, so a lone `0xFFFFFFFF`
    /// bought a 4 GiB allocation off a 24-byte header. The ceiling is
    /// derived from the compressed length, the one quantity the file
    /// cannot inflate for free.
    #[test]
    fn compressed_record_with_oversized_prefix_is_rejected() {
        let data =
            build_compressed_record_with_prefix(b"STAT", 0x300, u32::MAX, &[(b"EDID", b"Bomb\0")]);
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        let err = reader
            .read_sub_records(&header)
            .expect_err("a u32::MAX prefix must not reach Vec::with_capacity");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("ceiling"),
            "error must name the ceiling it tripped: {msg}",
        );
    }

    /// #3399 (b) — `read_to_end` had no output limit *independent* of the
    /// prefix, so a prefix that lies *downward* let the decoder run past
    /// it. A record declaring a handful of bytes while its payload
    /// inflates to megabytes is the decompression-bomb shape; it must be
    /// rejected, not silently truncated.
    #[test]
    fn compressed_record_inflating_past_its_prefix_is_rejected() {
        // ~4 MiB of zeros compresses to a few KiB — well inside the
        // ratio ceiling, so this exercises the *output* bound, not the
        // prefix bound.
        let big = vec![0u8; 4 * 1024 * 1024];
        let data = build_compressed_record_with_prefix(b"STAT", 0x301, 16, &[(b"EDID", &big)]);
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        let err = reader
            .read_sub_records(&header)
            .expect_err("inflating past the declared size must be an error");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("decompression bomb"),
            "error must name the bomb it caught: {msg}",
        );
    }

    /// The ceiling is a function of the compressed length, and must clear
    /// every shape vanilla actually ships. Census over the four masters
    /// that compress anything: 133 731 records, worst real ratio 102.1:1
    /// (an FNV `LAND`, 75 compressed bytes to 7 658), largest real
    /// inflated record 225 433 bytes (FO4). None is rejected.
    #[test]
    fn inflation_ceiling_clears_every_observed_vanilla_shape() {
        // The worst observed ratio, at the compressed length it occurred at.
        assert!(record_inflation_ceiling(75) >= 7_658);
        // The largest observed inflated record — even if it had been the
        // worst-ratio record too, which it is not.
        assert!(record_inflation_ceiling(225_433 / 102) >= 225_433);
        // And the bound still bites on the two attack shapes.
        assert!(record_inflation_ceiling(4) < u32::MAX as usize);
        assert_eq!(
            record_inflation_ceiling(usize::MAX),
            MAX_RECORD_INFLATED_BYTES,
            "the absolute cap must survive a saturating multiply",
        );
    }

    /// Error path: data_size < 4 must be rejected with an error, not a panic.
    #[test]
    fn compressed_record_too_small_returns_error() {
        // Build a record with FLAG_COMPRESSED but data_size = 3 (too small for prefix).
        let mut buf = Vec::new();
        buf.extend_from_slice(b"WEAP");
        buf.extend_from_slice(&3u32.to_le_bytes()); // data_size = 3 — too small
        buf.extend_from_slice(&FLAG_COMPRESSED.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // form_id
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&[0u8; 3]); // 3 bytes of body

        let mut reader = EsmReader::with_variant(&buf, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        let result = reader.read_sub_records(&header);
        assert!(result.is_err(), "data_size < 4 must return Err, got Ok");
    }

    // ── VWD / "Has Distant LOD" flag (#1731 / LC-D7-02) ─────────────────
    //
    // The record-header flag decoder only ever masked FLAG_COMPRESSED; the
    // 0x00010000 "Visible When Distant" bit was stored in `header.flags`
    // (never dropped) but had no named constant or accessor. These tests
    // pin that the flag now surfaces via `RecordHeader::is_visible_when_distant`.

    /// Build a bare synthetic record header (no sub-records) with the given
    /// raw `flags` value — enough to exercise `read_record_header` without
    /// a full record body.
    fn build_record_header_with_flags(typ: &[u8; 4], flags: u32) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(typ);
        buf.extend_from_slice(&0u32.to_le_bytes()); // data_size
        buf.extend_from_slice(&flags.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // form_id
        buf.extend_from_slice(&[0u8; 8]); // Tes5Plus trailing metadata
        buf
    }

    #[test]
    fn vwd_flag_is_surfaced_when_set() {
        let data = build_record_header_with_flags(b"STAT", FLAG_VISIBLE_WHEN_DISTANT);
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        assert!(
            header.is_visible_when_distant(),
            "FLAG_VISIBLE_WHEN_DISTANT (0x00010000) must be surfaced on the header"
        );
    }

    #[test]
    fn vwd_flag_is_false_when_unset() {
        let data = build_record_header_with_flags(b"STAT", 0);
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        assert!(!header.is_visible_when_distant());
    }

    /// The deleted-REFR tombstone bit (`0x20`, SKY-D4-01 / #1660) is a
    /// different bit on the same `flags` field — must not be confused with
    /// VWD (`0x00010000`).
    #[test]
    fn vwd_flag_is_distinct_from_deleted_refr_flag() {
        let data = build_record_header_with_flags(b"REFR", 0x20);
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        assert!(
            !header.is_visible_when_distant(),
            "the deleted-REFR flag (0x20) must not be mistaken for VWD (0x00010000)"
        );
    }

    /// Both flags can be set simultaneously without interfering with each
    /// other — they occupy distinct bits.
    #[test]
    fn vwd_flag_coexists_with_compressed_flag() {
        let data =
            build_record_header_with_flags(b"STAT", FLAG_VISIBLE_WHEN_DISTANT | FLAG_COMPRESSED);
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        assert!(header.is_visible_when_distant());
        assert_ne!(header.flags & FLAG_COMPRESSED, 0);
    }
}
