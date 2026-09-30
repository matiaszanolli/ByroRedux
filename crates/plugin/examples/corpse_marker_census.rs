//! #5005 census: which marker makes a placed FO3/FNV actor a corpse?
//!
//! The sourced engine rule (xNVSE `nvse/nvse/GameObjects.h:99`, the
//! reverse-engineered `TESObjectREFR` vtable):
//!
//! ```text
//! Unk_8B  // IsDead = HasNoHealth (baseForm health <= 0 or Flags bit23 set)
//! ```
//!
//! i.e. a reference is dead when its BASE form carries health <= 0 (the
//! `TESHealthForm` both `TESNPC` and `TESCreature` embed) or the base's
//! ACBS flags bit 23 is set — the marker is on the base, not the
//! placement. This census cross-tabs that rule against the competing
//! hypothesis (issue #5005's `XRGD` presence on the ACHR/ACRE) over the
//! vanilla masters:
//!
//!   * `XRGD & rule-dead` — posed corpses (the havok pose comes from the
//!     CS-era authoring workflow in the Oblivion CS wiki's "Creating Dead
//!     Actors", which makes 0-health actors first and poses them second).
//!   * `XRGD & rule-alive` — XRGD is pose-only data (e.g. deactivated
//!     robots); these are NOT corpses under the engine rule.
//!   * `rule-dead & !XRGD` — corpses the XRGD hypothesis would leave
//!     alive (unposed 0-health placements).
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example corpse_marker_census -- <file.esm> [...]

use std::collections::HashMap;

use byroredux_plugin::esm::reader::EsmReader;

/// xNVSE `TESActorBase` ACBS flags bit 23 (`kFlags_CreatureImmobile` is the
/// CREA-side alias; the IsDead comment names the bit generically).
const ACBS_BIT23: u32 = 1 << 23;

struct Base {
    editor_id: String,
    health: Option<i32>,
    acbs_flags: u32,
    is_creature: bool,
}

#[derive(Default)]
struct Totals {
    placed: usize,
    xrgd: usize,
    rule_dead: usize,
    xrgd_and_dead: usize,
    health_dead: usize,
    either: usize,
    xrgd_alive: Vec<String>,
    dead_unposed_named: Vec<String>,
    bit23_bases: usize,
    bit23_only: Vec<String>,
    health_nonpositive_bases: usize,
}

fn main() -> anyhow::Result<()> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let mut reader = EsmReader::new(&bytes);
        let _ = reader.read_file_header()?;
        let mut bases: HashMap<u32, Base> = HashMap::new();
        let mut refs: Vec<(u32, bool)> = Vec::new(); // (base form id, has XRGD)
        walk(&mut reader, bytes.len(), &mut bases, &mut refs)?;

        let mut t = Totals::default();
        for (base_fid, xrgd) in &refs {
            let Some(base) = bases.get(base_fid) else {
                continue;
            };
            t.placed += 1;
            let no_health = base.health.is_some_and(|h| h <= 0);
            let dead = no_health || base.acbs_flags & ACBS_BIT23 != 0;
            if *xrgd {
                t.xrgd += 1;
            }
            if dead {
                t.rule_dead += 1;
            }
            if no_health {
                t.health_dead += 1;
            }
            if *xrgd && dead {
                t.xrgd_and_dead += 1;
            }
            if *xrgd || dead {
                t.either += 1;
            }
            if *xrgd && !dead {
                t.xrgd_alive.push(base.editor_id.clone());
            }
            if dead && !*xrgd {
                let lower = base.editor_id.to_ascii_lowercase();
                if lower.contains("dead") || lower.contains("corpse") || lower.contains("loot") {
                    t.dead_unposed_named.push(base.editor_id.clone());
                }
            }
        }
        for base in bases.values() {
            if base.acbs_flags & ACBS_BIT23 != 0 {
                t.bit23_bases += 1;
            }
            if base.health.is_some_and(|h| h <= 0) {
                t.health_nonpositive_bases += 1;
            }
            if base.acbs_flags & ACBS_BIT23 != 0 && !base.health.is_some_and(|h| h <= 0) {
                t.bit23_only.push(if base.is_creature {
                    format!("{} [CREA]", base.editor_id)
                } else {
                    format!("{} [NPC_]", base.editor_id)
                });
            }
        }

        let game = path.rsplit('/').next().unwrap_or(&path);
        println!("== {game}");
        println!(
            "  placed actors w/ base: {}   bases: NPC_+CREA {} (health<=0: {}, ACBS bit23: {})",
            t.placed,
            bases.len(),
            t.health_nonpositive_bases,
            t.bit23_bases
        );
        println!(
            "  XRGD refs: {}   rule-dead refs: {}   health<=0 refs: {}   XRGD & rule-dead: {}   either: {}",
            t.xrgd, t.rule_dead, t.health_dead, t.xrgd_and_dead, t.either
        );
        println!(
            "  rule-dead but NOT XRGD: {}   (dead/corpse/loot-named among them: {})",
            t.either - t.xrgd_and_dead,
            t.dead_unposed_named.len()
        );
        println!("  XRGD but rule-ALIVE (pose-only, first 12):");
        for name in t.xrgd_alive.iter().take(12) {
            println!("    {name}");
        }
        println!("  rule-dead, dead-named, NOT XRGD (first 12):");
        for name in t.dead_unposed_named.iter().take(12) {
            println!("    {name}");
        }
        println!("  bit23-only bases, healthy (first 16):");
        for name in t.bit23_only.iter().take(16) {
            println!("    {name}");
        }
    }
    Ok(())
}

fn walk(
    reader: &mut EsmReader<'_>,
    end: usize,
    bases: &mut HashMap<u32, Base>,
    refs: &mut Vec<(u32, bool)>,
) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let group = reader.read_group_header()?;
            let inner_end = reader.group_content_end(&group);
            walk(reader, inner_end, bases, refs)?;
            continue;
        }
        let header = reader.read_record_header()?;
        let kind @ (b"NPC_" | b"CREA" | b"ACHR" | b"ACRE") = &header.record_type else {
            reader.skip_record(&header);
            continue;
        };
        let subs = reader.read_sub_records(&header)?;
        match kind {
            b"NPC_" | b"CREA" => {
                let mut editor_id = String::new();
                let mut health = None;
                let mut acbs_flags = 0u32;
                for sub in &subs {
                    match &sub.sub_type {
                        b"EDID" => {
                            editor_id = sub
                                .data
                                .split(|b| *b == 0)
                                .next()
                                .map(|s| String::from_utf8_lossy(s).into_owned())
                                .unwrap_or_default();
                        }
                        b"ACBS" if sub.data.len() >= 4 => {
                            acbs_flags = u32::from_le_bytes(sub.data[0..4].try_into()?);
                        }
                        // NPC_ DATA (11 or 25 B): i32 Base Health @ 0.
                        b"DATA" if *kind == *b"NPC_" && sub.data.len() >= 4 => {
                            health = Some(i32::from_le_bytes(sub.data[0..4].try_into()?));
                        }
                        // CREA DATA (17 B): i16 health @ 4 (CreatureStats).
                        b"DATA" if *kind == *b"CREA" && sub.data.len() >= 6 => {
                            health = Some(i16::from_le_bytes(sub.data[4..6].try_into()?) as i32);
                        }
                        _ => {}
                    }
                }
                bases.insert(
                    header.form_id,
                    Base {
                        editor_id,
                        health,
                        acbs_flags,
                        is_creature: *kind == *b"CREA",
                    },
                );
            }
            _ => {
                let mut base_fid = None;
                let mut xrgd = false;
                for sub in &subs {
                    match &sub.sub_type {
                        b"NAME" if sub.data.len() >= 4 => {
                            base_fid = Some(u32::from_le_bytes(sub.data[0..4].try_into()?));
                        }
                        b"XRGD" => xrgd = true,
                        _ => {}
                    }
                }
                if let Some(base_fid) = base_fid {
                    refs.push((base_fid, xrgd));
                }
            }
        }
    }
    Ok(())
}
