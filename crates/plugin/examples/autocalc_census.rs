//! Census the FO3/FNV ACBS "Auto-calculate stats" bit (0x0010) over every
//! `NPC_` record in an ESM — the discriminator between the class auto-calc
//! path `derive_autocalc_actor_values` takes and the stored SPECIAL/skill
//! values it cannot yet read (#2957).
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example autocalc_census -- <file.esm> [...]

use byroredux_plugin::esm::reader::EsmReader;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::actor::parse_npc;

const ACBS_AUTO_CALC_STATS: u32 = 0x0010;

fn main() -> anyhow::Result<()> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let mut reader = EsmReader::new(&bytes);
        let _header = reader.read_file_header()?;
        let mut flags: Vec<u32> = Vec::new();
        walk_groups(&mut reader, bytes.len(), &mut flags)?;
        let total = flags.len();
        let on = flags
            .iter()
            .filter(|f| *f & ACBS_AUTO_CALC_STATS != 0)
            .count();
        let off = total - on;
        let pct = |n: usize| {
            if total == 0 {
                0.0
            } else {
                n as f64 * 100.0 / total as f64
            }
        };
        println!(
            "{path}\n  NPC_ records      {total}\n  auto-calc ON      {on} ({:.1} %)\n  auto-calc OFF     {off} ({:.1} %)",
            pct(on),
            pct(off)
        );
    }
    Ok(())
}

fn walk_groups(reader: &mut EsmReader<'_>, end: usize, flags: &mut Vec<u32>) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let group = reader.read_group_header()?;
            let inner_end = reader.group_content_end(&group);
            walk_groups(reader, inner_end, flags)?;
            continue;
        }
        let header = reader.read_record_header()?;
        if header.record_type != *b"NPC_" {
            reader.skip_record(&header);
            continue;
        }
        let subs = reader.read_sub_records(&header)?;
        let npc = parse_npc(header.form_id, &subs, GameKind::Fallout3NV, &None);
        flags.push(npc.acbs_flags);
    }
    Ok(())
}
