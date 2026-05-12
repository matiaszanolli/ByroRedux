//! Magic / perks records.

use super::super::common::{read_lstring_or_zstring, read_u32_at, read_zstring};
use crate::esm::reader::SubRecord;

/// `PERK` perk / trait record. Holds the condition list + entry-point
/// tree that drives the `perk_system.md` / `perk_entry_points.md`
/// memos' ~120 catalog. Entry-point decoding (PRKE) is deferred —
/// lands with the condition pipeline. Stub captures identity + flags
/// so the perk catalog can be enumerated at load time.
#[derive(Debug, Clone, Default)]
pub struct PerkRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// Flags byte from DATA (playable / hidden / leveled / trait).
    pub perk_flags: u8,
}

pub fn parse_perk(form_id: u32, subs: &[SubRecord]) -> PerkRecord {
    let mut out = PerkRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"DESC" => out.description = read_lstring_or_zstring(&sub.data),
            b"DATA" if !sub.data.is_empty() => {
                out.perk_flags = sub.data[0];
            }
            _ => {}
        }
    }
    out
}

/// `SPEL` spell / ability / power record. FO3/FNV also covers passive
/// abilities and radiation-poisoning style auto-cast effects. SPIT
/// carries cost + level requirement + flags; effect list (EFID/EFIT)
/// is deferred — lands with MGEF application.
#[derive(Debug, Clone, Default)]
pub struct SpelRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Flags from SPIT offset 12 (or 8 on some pre-FNV variants).
    /// Bit 0 = `Manual Cost`, bit 2 = `Touch Explodes`.
    pub spell_flags: u32,
    /// Magicka cost from SPIT offset 0.
    pub cost: u32,
}

pub fn parse_spel(form_id: u32, subs: &[SubRecord]) -> SpelRecord {
    let mut out = SpelRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"SPIT" if sub.data.len() >= 16 => {
                out.cost = read_u32_at(&sub.data, 0).unwrap_or(0);
                out.spell_flags = read_u32_at(&sub.data, 12).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// `MGEF` magic effect record. Universal bridge for Actor Value
/// modifications — every perk entry point, spell effect, and
/// ingredient effect routes through here. Full effect decoding is
/// deferred; the stub captures identity + flags so references from
/// SPEL / ALCH / INGR resolve at load time.
#[derive(Debug, Clone, Default)]
pub struct MgefRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// Flags from DATA offset 0 (hostile / recover / detrimental / ...).
    pub effect_flags: u32,
}

pub fn parse_mgef(form_id: u32, subs: &[SubRecord]) -> MgefRecord {
    let mut out = MgefRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"DESC" => out.description = read_lstring_or_zstring(&sub.data),
            b"DATA" if sub.data.len() >= 4 => {
                out.effect_flags = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// `ENCH` enchantment record (Oblivion / FO3 / FNV / Skyrim). Carries
/// the effect chain a `WEAP.eitm` / `AMMO.eitm` / `ARMO.eitm` reference
/// resolves to: Pulse Gun's "Pulse" enchantment, This Machine's charge
/// effect, Holorifle's energy splash, and the entire vanilla-Skyrim
/// weapon-enchantment table. ENIT carries type/charge/cost/flags;
/// EFID/EFIT effect blocks mirror SPEL — full effect decoding is
/// deferred (lands with MGEF effect application), so the stub captures
/// identity + ENIT scalars so dangling EITM cross-refs resolve at
/// lookup time. See #629 / FNV-D2-01.
#[derive(Debug, Clone, Default)]
pub struct EnchRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// ENIT offset 0 (u32). Spell-school target: `0` = spell, `1` =
    /// staff, `2` = weapon, `3` = apparel. Vanilla weapon-EITM refs
    /// resolve to `2` exclusively.
    pub enchantment_type: u32,
    /// ENIT offset 4 (u32). Magicka / charge pool — interpreted per
    /// `enchantment_type`. Weapon enchantments use this as the per-hit
    /// charge cost.
    pub charge_amount: u32,
    /// ENIT offset 8 (u32). Pre-calculated enchant cost (auto-generated
    /// at compile time from the EFIT chain). Used by the auto-calc UI
    /// in the editor; runtime consumers re-derive from the effect chain
    /// if `flags & NoAutoCalculate` is set.
    pub enchant_cost: u32,
    /// ENIT offset 12 (u32). Bit 0 = `NoAutoCalculate` (manual
    /// override of `enchant_cost`); other bits unused on FO3/FNV.
    pub enchant_flags: u32,
}

pub fn parse_ench(form_id: u32, subs: &[SubRecord]) -> EnchRecord {
    let mut out = EnchRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            // ENIT is fixed 16 bytes on FO3 / FNV / Oblivion; Skyrim
            // appended a `cast_type` u32 making it 20 — guard `>= 16`
            // so both layouts decode the shared prefix safely.
            b"ENIT" if sub.data.len() >= 16 => {
                out.enchantment_type = read_u32_at(&sub.data, 0).unwrap_or(0);
                out.charge_amount = read_u32_at(&sub.data, 4).unwrap_or(0);
                out.enchant_cost = read_u32_at(&sub.data, 8).unwrap_or(0);
                out.enchant_flags = read_u32_at(&sub.data, 12).unwrap_or(0);
            }
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
    fn parse_perk_picks_data_flags() {
        let subs = vec![
            sub(b"EDID", b"IntenseTraining\0"),
            sub(b"FULL", b"Intense Training\0"),
            sub(b"DESC", b"Increase any one S.P.E.C.I.A.L. by 1.\0"),
            sub(b"DATA", &[0x01]), // playable
        ];
        let p = parse_perk(0xE5E5, &subs);
        assert_eq!(p.editor_id, "IntenseTraining");
        assert_eq!(p.perk_flags, 0x01);
    }

    #[test]
    fn parse_spel_picks_spit_cost_and_flags() {
        let mut spit = Vec::new();
        spit.extend_from_slice(&42u32.to_le_bytes()); // cost
        spit.extend_from_slice(&[0u8; 8]); // padding to flags offset
        spit.extend_from_slice(&0x0000_0004u32.to_le_bytes()); // flags
        let subs = vec![sub(b"EDID", b"Fireball\0"), sub(b"SPIT", &spit)];
        let s = parse_spel(0xF6F6, &subs);
        assert_eq!(s.cost, 42);
        assert_eq!(s.spell_flags, 0x0000_0004);
    }

    #[test]
    fn parse_ench_picks_enit_scalars() {
        // Synthesize ENIT for FNV's Pulse Gun-style weapon enchant:
        //   type    = 2 (weapon)
        //   charge  = 25 (per-hit charge cost)
        //   cost    = 100 (auto-calc cost)
        //   flags   = 0x01 (NoAutoCalculate)
        let mut enit = Vec::new();
        enit.extend_from_slice(&2u32.to_le_bytes());
        enit.extend_from_slice(&25u32.to_le_bytes());
        enit.extend_from_slice(&100u32.to_le_bytes());
        enit.extend_from_slice(&0x0000_0001u32.to_le_bytes());
        let subs = vec![
            sub(b"EDID", b"PulseEnchant\0"),
            sub(b"FULL", b"Pulse\0"),
            sub(b"ENIT", &enit),
        ];
        let e = parse_ench(0x000E_5C77, &subs);
        assert_eq!(e.editor_id, "PulseEnchant");
        assert_eq!(e.full_name, "Pulse");
        assert_eq!(e.enchantment_type, 2);
        assert_eq!(e.charge_amount, 25);
        assert_eq!(e.enchant_cost, 100);
        assert_eq!(e.enchant_flags, 0x01);
    }

    #[test]
    fn parse_ench_tolerates_skyrim_20_byte_enit() {
        // Skyrim appended a `cast_type` u32 to ENIT (20 bytes total).
        // The shared 16-byte prefix must still decode safely; the
        // trailing field is ignored. #629 / FNV-D2-01 must not regress
        // future Skyrim parses that route through the same arm.
        let mut enit = Vec::new();
        enit.extend_from_slice(&2u32.to_le_bytes());
        enit.extend_from_slice(&50u32.to_le_bytes());
        enit.extend_from_slice(&200u32.to_le_bytes());
        enit.extend_from_slice(&0x0000_0000u32.to_le_bytes());
        enit.extend_from_slice(&3u32.to_le_bytes()); // Skyrim cast_type
        assert_eq!(enit.len(), 20);
        let subs = vec![sub(b"EDID", b"FireDmg\0"), sub(b"ENIT", &enit)];
        let e = parse_ench(0x0001_F25D, &subs);
        assert_eq!(e.charge_amount, 50);
        assert_eq!(e.enchant_cost, 200);
    }

    #[test]
    fn parse_ench_short_enit_keeps_defaults() {
        // Author-malformed ENIT (< 16 bytes) must not panic and must
        // leave scalars at their defaults so the surrounding records
        // still load.
        let subs = vec![sub(b"EDID", b"BrokenEnchant\0"), sub(b"ENIT", &[0u8; 8])];
        let e = parse_ench(0xDEAD_BEEF, &subs);
        assert_eq!(e.editor_id, "BrokenEnchant");
        assert_eq!(e.enchantment_type, 0);
        assert_eq!(e.charge_amount, 0);
        assert_eq!(e.enchant_cost, 0);
        assert_eq!(e.enchant_flags, 0);
    }

    #[test]
    fn parse_mgef_picks_data_effect_flags() {
        let subs = vec![
            sub(b"EDID", b"RadiationPoisoning\0"),
            sub(b"FULL", b"Radiation Poisoning\0"),
            sub(b"DESC", b"Contaminated by radiation.\0"),
            sub(b"DATA", &0x0000_0009u32.to_le_bytes()),
        ];
        let e = parse_mgef(0xA7A7, &subs);
        assert_eq!(e.effect_flags, 0x0000_0009);
    }
}
