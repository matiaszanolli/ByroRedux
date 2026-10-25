//! Alignment probe (#6350 1b): line the repo-parsed `WEAP` fields up
//! against the raw `DNAM` float columns so the FO3 AP-cost and Base
//! VATS To-Hit slots are named from evidence, not memory.
//!
//! Prints, for a watchlist plus every VATS-AP-authoring weapon:
//! parsed damage / clip / crit-mult, the raw DNAM f32 columns at
//! @20/@24/@28/@44/@64/@68/@88/@92/@96, and the FNV VATS sub-record
//! (skill @4, damage mult @8, AP @12).
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example probe_weap_align -- <ESM>

use std::collections::BTreeMap;

use byroredux_plugin::esm::parse_esm;
use byroredux_plugin::esm::reader::EsmReader;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: probe_weap_align <ESM>"))?;
    let bytes = std::fs::read(&path)?;
    let index = parse_esm(&bytes).expect("parse");

    // Raw DNAM/VATS by (remapped) form id — same walk the parser used,
    // but keeping the bytes.
    let mut dnam: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    let mut vats: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    let mut edid: BTreeMap<u32, String> = BTreeMap::new();
    {
        let mut reader = EsmReader::new(&bytes);
        let _ = reader.read_file_header()?;
        reader.for_each_record(bytes.len(), Some(b"WEAP"), |header, subs| {
            for sub in subs {
                match &sub.sub_type {
                    b"EDID" => {
                        edid.insert(
                            header.form_id,
                            String::from_utf8_lossy(&sub.data)
                                .trim_end_matches('\0')
                                .to_string(),
                        );
                    }
                    b"DNAM" => {
                        dnam.insert(header.form_id, sub.data.clone());
                    }
                    b"VATS" => {
                        vats.insert(header.form_id, sub.data.clone());
                    }
                    _ => {}
                }
            }
        })?;
    }

    let f32_at = |d: &[u8], o: usize| {
        d.get(o..o + 4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "-".to_string())
    };
    let watchlist = [
        "Weap10mmPistol",
        "WeapLaserRifle",
        "WeapLaserPistol",
        "WeapPlasmaRifle",
        "WeapHuntingRifle",
        "WeapSniperRifle",
        "WeapAssaultRifle",
        "WeapChineseAssaultRifle",
        "WeapChinesePistol",
        "Weap10mmSMG",
        "WeapCombatShotgun",
        "WeapSawedOffShotgun",
        "WeapMinigun",
        "WeapGatlingLaser",
        "WeapMissileLauncher",
        "WeapFatMan",
        "WeapFlamer",
        "WeapCombatKnife",
        "WeapBaseballBat",
        "WeapSuperSledge",
        "WeapPowerFist",
        "WeapShishkebab",
        "WeapNVHuntingRevolver",
        "WeapNVMachete",
        "WeapNVLeadPipe",
        "WeapNV9iron",
    ];

    println!("  edid | parsed(dmg,clip,crit) | dnam @20 @24 @28 @44 @64 @68 @88 @92 @96 | vats(skill,dmg,ap)");
    for (fid, item) in index.items.iter() {
        let Some(name) = edid.get(fid) else { continue };
        if !watchlist.contains(&name.as_str())
            && !vats
                .get(fid)
                .and_then(|v| v.get(12..16))
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .is_some_and(|v| v > 0.0)
        {
            continue;
        }
        let byroredux_plugin::esm::records::ItemRecord {
            kind: byroredux_plugin::esm::records::ItemKind::Weapon { damage, clip_size, crit_mult, ap_cost, .. },
            ..
        } = item
        else {
            continue;
        };
        let d = dnam.get(fid).cloned().unwrap_or_default();
        let v = vats.get(fid).cloned().unwrap_or_default();
        println!(
            "  {name:<26} dmg={damage:.0} clip={clip_size:.0} crit={crit_mult:.2} ap={ap_cost:.0} | {} {} {} {} {} {} {} {} {} | {} {} {}",
            f32_at(&d, 20),
            f32_at(&d, 24),
            f32_at(&d, 28),
            f32_at(&d, 44),
            f32_at(&d, 64),
            f32_at(&d, 68),
            f32_at(&d, 88),
            f32_at(&d, 92),
            f32_at(&d, 96),
            f32_at(&v, 4),
            f32_at(&v, 8),
            f32_at(&v, 12),
        );
    }
    Ok(())
}
