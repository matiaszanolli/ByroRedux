//! Equipment / crafting / generic-record records.

use super::super::common::{read_f32_at, read_lstring_or_zstring, read_u32_at, read_zstring};
use crate::esm::reader::{GameKind, SubRecord};

/// ARMA — armor addon record. Race-specific biped slot variants for
/// armor. The ARMO → ARMA → race-specific MODL chain resolves armor
/// rendering on non-default-race NPCs (Vipers, Ghouls, Super Mutants,
/// Centaurs, Deathclaws, etc.).
///
/// Stub captures EDID + biped flags + DT/DR + the male/female biped
/// model paths the Skyrim+ equip pipeline dispatches against. See
/// audits `FNV-D2-NEW-01` / #808 (initial stub) and #896 (Phase B
/// extension for Skyrim+/FO4 actor-worn meshes).
///
/// Per-game sub-record meanings sourced from the xEdit project (by
/// ElminsterAU and the xEdit team, MPL-2.0,
/// <https://github.com/TES5Edit/TES5Edit>) at tag `dev-4.1.6`
/// (2026-05-07):
///
/// | sub  | FNV / FO3 ARMA          | Skyrim+ ARMA              |
/// |------|-------------------------|---------------------------|
/// | BMDT | biped_flags + general   | (replaced by BOD2)        |
/// | BOD2 | n/a                     | biped_flags + armor_type  |
/// | RNAM | n/a                     | race FormID               |
/// | MODL | male **biped** path     | additional-races FormID[] |
/// | MOD2 | male world model path   | male **biped** path       |
/// | MOD3 | female biped path       | female biped path         |
/// | MOD4 | female world model path | male 1st-person biped     |
///
/// The `male_biped_model` / `female_biped_model` fields below
/// normalise across games: they always hold the *worn* mesh paths,
/// regardless of which sub-record code carried them on disk.
#[derive(Debug, Clone, Default)]
pub struct ArmaRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `BMDT` (FNV/FO3) or `BOD2` (Skyrim+) offset 0..4 — biped slot
    /// bitfield. Drives the ARMO → ARMA biped-slot routing.
    pub biped_flags: u32,
    /// `BMDT` offset 4..8 — general flags (Heavy / Medium / Light,
    /// power armor, etc.). Decoded lazily per-game. 0 on Skyrim+
    /// (BOD2 carries `armor_type` instead, captured below).
    pub general_flags: u32,
    /// `DNAM` offset 0..2 — Damage Threshold (i16, FNV-specific).
    pub dt: i16,
    /// `DNAM` offset 2..4 — Damage Resistance (i16, FNV-specific).
    pub dr: i16,
    /// `RNAM` — race FormID this addon is for (Skyrim+ only).
    /// 0 on Oblivion/FO3/FNV — race linkage there flows through
    /// ARMO records or per-race world models, not ARMA.
    pub race_form_id: u32,
    /// `MODL` (FNV/FO3 — male biped) or `MOD2` (Skyrim+ — male
    /// biped). Normalised: always the male worn mesh path.
    pub male_biped_model: String,
    /// `MOD3` on every supported game — female worn mesh path.
    pub female_biped_model: String,
    /// Skyrim+ only: additional race FormIDs from the `Additional
    /// Races` MODL FormID array. Empty on FNV/FO3 (MODL there is the
    /// male biped path, captured into `male_biped_model`).
    pub additional_races: Vec<u32>,
}

/// Parse an ARMA record. Game-aware because MODL has different
/// meanings (string path vs FormID list) on Skyrim+ vs FNV/FO3.
pub fn parse_arma(form_id: u32, subs: &[SubRecord], game: GameKind) -> ArmaRecord {
    let mut out = ArmaRecord {
        form_id,
        ..Default::default()
    };
    let is_skyrim_or_later = matches!(
        game,
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield
    );

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"BMDT" if sub.data.len() >= 8 => {
                out.biped_flags = read_u32_at(&sub.data, 0).unwrap_or(0);
                out.general_flags = read_u32_at(&sub.data, 4).unwrap_or(0);
            }
            b"BOD2" if is_skyrim_or_later && sub.data.len() >= 4 => {
                out.biped_flags = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            b"DNAM" if !is_skyrim_or_later && sub.data.len() >= 4 => {
                out.dt = i16::from_le_bytes([sub.data[0], sub.data[1]]);
                out.dr = i16::from_le_bytes([sub.data[2], sub.data[3]]);
            }
            b"RNAM" if is_skyrim_or_later && sub.data.len() >= 4 => {
                out.race_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            b"MODL" => {
                if is_skyrim_or_later {
                    // Skyrim+ ARMA additional-races: 4-byte FormID per
                    // entry. Multiple MODL sub-records can appear.
                    if let Some(id) = read_u32_at(&sub.data, 0) {
                        out.additional_races.push(id);
                    }
                } else if out.male_biped_model.is_empty() {
                    // FNV/FO3 ARMA male biped — first MODL only;
                    // ignore subsequent MODT (texture variant) refs.
                    out.male_biped_model = read_zstring(&sub.data);
                }
            }
            b"MOD2" if is_skyrim_or_later && out.male_biped_model.is_empty() => {
                // Skyrim+ male biped model lives at MOD2 (FNV's MOD2
                // is the world/drop model — uninteresting for the
                // worn-mesh consumer).
                out.male_biped_model = read_zstring(&sub.data);
            }
            b"MOD3" if out.female_biped_model.is_empty() => {
                out.female_biped_model = read_zstring(&sub.data);
            }
            _ => {}
        }
    }
    out
}

/// BPTD — body part data record. Per-NPC dismemberment routing
/// (head, torso, limbs) + biped slot count. Used by VATS targeting
/// and gore effects.
///
/// Each part is described by a quartet of sub-records (`BPTN` name +
/// `BPNN` node + `BPNT` target + `BPND` data). The stub captures the
/// total part count and the first part name as a sanity check; the
/// full per-part array decode lands with the dismemberment consumer.
/// See audit `FNV-D2-NEW-01` / #808.
#[derive(Debug, Clone, Default)]
pub struct BptdRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Number of body parts (count of `BPTN` sub-records).
    pub part_count: u32,
    /// `BPTN` of the first body part (often "Head"). Sanity-check
    /// field; downstream code that needs every part will re-walk
    /// the sub-records.
    pub first_part_name: String,
}

// MILESTONE: Tier-7 ragdoll / dismemberment — see #1057.
// `BptdRecord` (part_count + first_part_name) is extracted today but
// no consumer touches it. The dismemberment routing + biped-slot
// catalogue from BPTD never reaches the physics or render layer.
// When ragdoll lands, swap the count-only stub for a full per-part
// array (BPTN + BPNN + BPNT + BPND quartets) and wire to the physics
// crate's actor body builder.
pub fn parse_bptd(form_id: u32, subs: &[SubRecord]) -> BptdRecord {
    let mut out = BptdRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"BPTN" => {
                if out.part_count == 0 {
                    out.first_part_name = read_lstring_or_zstring(&sub.data);
                }
                out.part_count += 1;
            }
            _ => {}
        }
    }
    out
}

// ── #809 / FNV-D2-NEW-02 — supporting record stubs ─────────────────
//
// Seven records that gate FNV NPC AI / crafting / impact-effect /
// faction-reputation subsystems. All stub-form (EDID + a handful of
// key scalar / form-ref fields per record); full sub-record decoding
// lands when the consuming subsystem arrives. Pattern matches #808.

/// COBJ — constructible object record (FNV crafting). Workbench /
/// reloading bench / campfire recipes. Stub captures EDID + created
/// form (CNAM) + workbench filter (BNAM). The CNTO component list
/// is deferred to the crafting consumer. See audit
/// `FNV-D2-NEW-02` / #809.
#[derive(Debug, Clone, Default)]
pub struct CobjRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `CNAM` — created form ID (the recipe's output item).
    pub created_form: u32,
    /// `BNAM` — workbench filter form ID (which workbench category
    /// the recipe shows up under).
    pub workbench_form: u32,
}

pub fn parse_cobj(form_id: u32, subs: &[SubRecord]) -> CobjRecord {
    let mut out = CobjRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"CNAM" if sub.data.len() >= 4 => {
                out.created_form = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            b"BNAM" if sub.data.len() >= 4 => {
                out.workbench_form = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

// ── #810 / FNV-D2-NEW-03 — long-tail catch-all stubs ───────────────
//
// 31 record types in the FNV catch-all-skip long tail. None had a
// concrete consumer driving a per-record parser (the audit's defer-
// until-consumer recommendation). The user opted to bulk-dispatch
// anyway so the catch-all skip approaches parity with FalloutNV.esm's
// authored content set.
//
// All 31 share a single minimal-stub form: EDID + optional FULL.
// Records that gain a real consumer later can grow per-record fields
// via the established #808 / #809 pattern (each record gets its own
// dedicated struct). Until then, this is the cheapest dispatch shape
// that takes them off the catch-all skip without 31× boilerplate.
//
// Records covered (5 clusters):
//   Audio metadata (11): ALOC ANIO ASPC CAMS CPTH DOBJ MICN MSET MUSC SOUN VTYP
//   Visual / world (8):  AMEF DEBR GRAS IMAD LSCR LSCT PWAT RGDL
//   Hardcore mode (4):   DEHY HUNG RADS SLPD
//   Caravan + Casino (6): CCRD CDCK CHAL CHIP CMNY CSNO
//   Recipe residuals (2): RCCT RCPE

/// Minimal-stub record for the long-tail dispatch coverage. EDID +
/// optional FULL captured; all other sub-records skipped. When a
/// consumer for a specific record type arrives, replace its
/// `HashMap<u32, MinimalEsmRecord>` field on `EsmIndex` with a
/// dedicated struct + parser pair following the `#808` / `#809`
/// established pattern. See audit `FNV-D2-NEW-03` / #810.
#[derive(Debug, Clone, Default)]
pub struct MinimalEsmRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
}

pub fn parse_minimal_esm_record(form_id: u32, subs: &[SubRecord]) -> MinimalEsmRecord {
    let mut out = MinimalEsmRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            _ => {}
        }
    }
    out
}

/// `SLGM` — soul-gem record. Oblivion ships ~15 vanilla SLGMs; FO3
/// drops the record type (no soul magic in the Wasteland), Skyrim
/// keeps it. Referenced by `ENCH` for the enchantment charge model
/// and by quest scripts for the Azura's Star / Soul Trap chain.
///
/// Wire layout per UESP / xEdit (Oblivion / Skyrim TES4 schema):
/// - `EDID` — editor ID
/// - `FULL` — display name
/// - `MODL` — model path (dual-target — also lands in `cells.statics`
///   via [`extract_records_with_modl`] so REFR → SLGM placements
///   render the right mesh)
/// - `ICON` — inventory icon path
/// - `SCRI` — attached script FormID (rare on SLGM)
/// - `DATA` — value (i32) + weight (f32), 8 bytes
/// - `SOUL` — single-byte enum: 0 None, 1 Petty, 2 Lesser, 3 Common,
///   4 Greater, 5 Grand. The soul *currently contained*; pre-loaded
///   soul gems carry a non-zero value.
/// - `SLCP` — single-byte enum (same scale as SOUL): the gem's
///   *capacity*. A Grand Soul Gem can hold a Lesser soul, etc.
///
/// The audit text suggested decoding "DATA byte 0 soul capacity" —
/// that's incorrect (DATA byte 0 is the LSB of `value: i32`). The
/// authoritative capacity field is `SLCP`. See OBL-D3-NEW-02 / #966.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlgmRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// MODL path — the inventory / world-placement mesh.
    pub model_path: String,
    /// Item value in caps / gold.
    pub value: i32,
    /// Item weight in encumbrance units.
    pub weight: f32,
    /// `SOUL` byte: currently-contained soul magnitude.
    /// 0 None, 1 Petty, 2 Lesser, 3 Common, 4 Greater, 5 Grand.
    pub current_soul: u8,
    /// `SLCP` byte: the gem's capacity (same enum). The audit
    /// originally called this "DATA byte 0"; that was a misread.
    pub soul_capacity: u8,
}

pub fn parse_slgm(form_id: u32, subs: &[SubRecord]) -> SlgmRecord {
    let mut out = SlgmRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"MODL" => out.model_path = read_zstring(&sub.data),
            b"DATA" if sub.data.len() >= 8 => {
                out.value = i32::from_le_bytes([
                    sub.data[0],
                    sub.data[1],
                    sub.data[2],
                    sub.data[3],
                ]);
                out.weight = read_f32_at(&sub.data, 4).unwrap_or(0.0);
            }
            b"SOUL" if !sub.data.is_empty() => out.current_soul = sub.data[0],
            b"SLCP" if !sub.data.is_empty() => out.soul_capacity = sub.data[0],
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    #[test]
    fn parse_arma_picks_edid_biped_flags_dt_dr() {
        // `MetalArmor` shape: BMDT = (i32 biped_flags, i32 general_flags),
        // DNAM = (i16 dt, i16 dr). Stub captures the biped flags so the
        // ARMO → ARMA → biped-slot routing has the data it needs.
        let mut bmdt = [0u8; 8];
        bmdt[0..4].copy_from_slice(&0x0000_000C_u32.to_le_bytes()); // body+legs slot
        bmdt[4..8].copy_from_slice(&0x0000_0001_u32.to_le_bytes()); // metal flag
        let mut dnam = [0u8; 4];
        dnam[0..2].copy_from_slice(&15_i16.to_le_bytes()); // DT
        dnam[2..4].copy_from_slice(&30_i16.to_le_bytes()); // DR
        let subs = vec![
            sub(b"EDID", b"MetalArmor\0"),
            sub(b"BMDT", &bmdt),
            sub(b"DNAM", &dnam),
        ];
        let a = parse_arma(0x0006_2103, &subs, GameKind::Fallout3NV);
        assert_eq!(a.editor_id, "MetalArmor");
        assert_eq!(a.biped_flags, 0x0000_000C);
        assert_eq!(a.general_flags, 0x0000_0001);
        assert_eq!(a.dt, 15);
        assert_eq!(a.dr, 30);
    }

    #[test]
    fn parse_bptd_picks_edid_first_bptn() {
        // `HumanRace` shape: BPTN labels each body-part name; stub
        // captures the first one for sanity-check round-tripping.
        let subs = vec![
            sub(b"EDID", b"HumanRace\0"),
            sub(b"BPTN", b"Head\0"),
            sub(b"BPTN", b"Torso\0"),
            sub(b"BPTN", b"Left Arm\0"),
        ];
        let b = parse_bptd(0x0009_29DC, &subs);
        assert_eq!(b.editor_id, "HumanRace");
        assert_eq!(b.part_count, 3);
        assert_eq!(b.first_part_name, "Head");
    }

    // ── #809 / FNV-D2-NEW-02 stubs ─────────────────────────────────
    #[test]
    fn parse_cobj_picks_edid_created_workbench() {
        // `RecipeStimpak` shape: EDID + CNAM (created stimpak form) +
        // BNAM (workbench filter form).
        let subs = vec![
            sub(b"EDID", b"RecipeStimpak\0"),
            sub(b"CNAM", &0x0014_4F10_u32.to_le_bytes()),
            sub(b"BNAM", &0x000A_6_001_u32.to_le_bytes()),
        ];
        let c = parse_cobj(0x0014_F800, &subs);
        assert_eq!(c.editor_id, "RecipeStimpak");
        assert_eq!(c.created_form, 0x0014_4F10);
        assert_eq!(c.workbench_form, 0x000A_6_001);
    }
    #[test]
    fn parse_minimal_record_picks_edid_full() {
        let subs = vec![
            sub(b"EDID", b"MUSCMainTitle\0"),
            sub(b"FULL", b"Main Title Theme\0"),
            // Other sub-records ignored:
            sub(b"FNAM", b"music\\base\\maintitle.mp3\0"),
        ];
        let m = parse_minimal_esm_record(0x0001_5C8C, &subs);
        assert_eq!(m.form_id, 0x0001_5C8C);
        assert_eq!(m.editor_id, "MUSCMainTitle");
        assert_eq!(m.full_name, "Main Title Theme");
    }

    #[test]
    fn parse_minimal_record_handles_edid_only() {
        // Many long-tail records ship EDID only (no FULL). Stub must
        // tolerate the missing FULL without panic; full_name stays
        // empty.
        let subs = vec![sub(b"EDID", b"DOBJDefaultObject\0")];
        let m = parse_minimal_esm_record(0xDEAD_BEEF, &subs);
        assert_eq!(m.editor_id, "DOBJDefaultObject");
        assert!(m.full_name.is_empty());
    }

    // ── #966 / OBL-D3-NEW-02 — SLGM dedicated decode ────────────────

    #[test]
    fn parse_slgm_decodes_full_oblivion_layout() {
        // Synthetic Grand Soul Gem (pre-loaded with a Grand soul) —
        // exercises every sub-record arm the parser is supposed to
        // consume. DATA = value (i32) + weight (f32). SOUL = current
        // soul magnitude (0..=5). SLCP = capacity (0..=5).
        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&750_i32.to_le_bytes());
        data.extend_from_slice(&1.0_f32.to_le_bytes());

        let subs = vec![
            sub(b"EDID", b"SoulGemGrandFilled\0"),
            sub(b"FULL", b"Grand Soul Gem\0"),
            sub(b"MODL", b"Clutter\\SoulGems\\Grand.NIF\0"),
            sub(b"ICON", b"Clutter\\SoulGems\\Grand.dds\0"),
            sub(b"DATA", &data),
            sub(b"SOUL", &[5]), // current = Grand
            sub(b"SLCP", &[5]), // capacity = Grand
        ];
        let g = parse_slgm(0x0002_3F1B, &subs);
        assert_eq!(g.form_id, 0x0002_3F1B);
        assert_eq!(g.editor_id, "SoulGemGrandFilled");
        assert_eq!(g.full_name, "Grand Soul Gem");
        assert_eq!(g.model_path, "Clutter\\SoulGems\\Grand.NIF");
        assert_eq!(g.value, 750);
        assert!((g.weight - 1.0).abs() < f32::EPSILON);
        assert_eq!(g.current_soul, 5);
        assert_eq!(g.soul_capacity, 5);
    }

    #[test]
    fn parse_slgm_handles_empty_petty_gem_no_soul() {
        // Empty petty gem — no SOUL sub-record at all. Capacity comes
        // from SLCP. Pre-fix the audit text suggested reading capacity
        // from DATA byte 0; this test pins that the correct field is
        // SLCP and that absence of SOUL leaves `current_soul` at 0.
        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&25_i32.to_le_bytes());
        data.extend_from_slice(&0.1_f32.to_le_bytes());

        let subs = vec![
            sub(b"EDID", b"SoulGemPettyEmpty\0"),
            sub(b"DATA", &data),
            sub(b"SLCP", &[1]), // petty capacity, no SOUL = empty gem
        ];
        let g = parse_slgm(0x0002_3F00, &subs);
        assert_eq!(g.value, 25);
        assert_eq!(g.current_soul, 0, "missing SOUL means an empty gem");
        assert_eq!(g.soul_capacity, 1);
    }

    #[test]
    fn parse_slgm_truncated_data_does_not_panic() {
        // 5-byte DATA (gated by the `>= 8` arm) → value/weight stay at
        // default. Empty SOUL / SLCP also tolerated.
        let subs = vec![
            sub(b"EDID", b"SoulGemMalformed\0"),
            sub(b"DATA", &[1u8; 5]),
            sub(b"SOUL", &[]),
            sub(b"SLCP", &[3]),
        ];
        let g = parse_slgm(0x0002_3F02, &subs);
        assert_eq!(g.value, 0, "short DATA must not bleed bytes into value");
        assert_eq!(g.current_soul, 0, "empty SOUL stays at default");
        assert_eq!(g.soul_capacity, 3);
    }
}
