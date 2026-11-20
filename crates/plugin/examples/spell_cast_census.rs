//! Census of an Oblivion `SPEL` set for the player-spellcasting PRD's Phase 0 (PSC-CST-003/004,
//! PSC-TGT-002/005, PSC-USE-003): the authored `SPIT` header (type, cost, mastery level, flags), each
//! effect's range, area and school, and how the stored cost compares to the UESP effect-cost equation.
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example spell_cast_census -- <Oblivion.esm>
//!
//! Layouts are xEdit's `wbDefinitionsTES4.pas`: `SPIT` = type u8, 3 unused, cost u32, level u8, 3 unused,
//! flags u8, 3 unused; `EFIT` = effect code u32, magnitude u32, area u32, duration u32, type u32 (0 Self,
//! 1 Touch, 2 Target), actor value i32; `SCIT` = script formid u32, magic school u32, visual effect u32,
//! hostile u8; `MGEF` `DATA` = flags u32, base cost f32, assoc item u32, magic school u32, resist value i32.

use byroredux_plugin::esm::reader::{EsmReader, SubRecord};
use std::collections::BTreeMap;

const SCHOOLS: [&str; 7] = [
    "Alteration",
    "Conjuration",
    "Destruction",
    "Illusion",
    "Mysticism",
    "Restoration",
    "None",
];
const TYPES: [&str; 6] = [
    "Spell",
    "Disease",
    "Power",
    "LesserPower",
    "Ability",
    "Poison",
];
const LEVELS: [&str; 5] = ["Novice", "Apprentice", "Journeyman", "Expert", "Master"];
const RANGES: [&str; 3] = ["Self", "Touch", "Target"];

#[derive(Clone, Copy)]
struct Mgef {
    base_cost: f32,
    school: u32,
}

struct Effect {
    code: [u8; 4],
    magnitude: u32,
    area: u32,
    duration: u32,
    range: u32,
    script_school: Option<u32>,
}

struct Spell {
    form_id: u32,
    edid: String,
    full: String,
    ty: u8,
    cost: u32,
    level: u8,
    flags: u8,
    effects: Vec<Effect>,
}

fn zstr(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).into_owned()
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// UESP *Oblivion:Spell Making*, "Spell Cost": `B × M × D × A` per effect, each factor floored at 1,
/// times 1.5 for a Target effect.
fn effect_cost(effect: &Effect, base_cost: f32) -> f32 {
    let b = (base_cost / 10.0).max(0.0);
    let m = (effect.magnitude.max(1) as f32).powf(1.28);
    let d = effect.duration.max(1) as f32;
    let a = (effect.area as f32 * 0.15).max(1.0);
    let cost = b * m * d * a;
    if effect.range == 2 {
        cost * 1.5
    } else {
        cost
    }
}

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: spell_cast_census <Oblivion.esm>"))?;
    let bytes = std::fs::read(&path)?;

    let mut mgefs: BTreeMap<[u8; 4], Mgef> = BTreeMap::new();
    {
        let mut reader = EsmReader::new(&bytes);
        let _ = reader.read_file_header()?;
        reader.for_each_record(bytes.len(), Some(b"MGEF"), |_, subs: Vec<SubRecord>| {
            let code = subs
                .iter()
                .find(|s| &s.sub_type == b"EDID")
                .and_then(|s| s.data.get(..4))
                .map(|b| <[u8; 4]>::try_from(b).unwrap());
            let data = subs
                .iter()
                .find(|s| &s.sub_type == b"DATA")
                .map(|s| s.data.clone());
            let parsed = code
                .zip(data)
                .and_then(|(code, data)| Some((code, u32_at(&data, 4)?, u32_at(&data, 12)?)));
            if let Some((code, cost, school)) = parsed {
                mgefs.insert(
                    code,
                    Mgef {
                        base_cost: f32::from_bits(cost),
                        school,
                    },
                );
            }
        })?;
    }

    let mut spells: Vec<Spell> = Vec::new();
    {
        let mut reader = EsmReader::new(&bytes);
        let _ = reader.read_file_header()?;
        reader.for_each_record(
            bytes.len(),
            Some(b"SPEL"),
            |header, subs: Vec<SubRecord>| {
                let mut spell = Spell {
                    form_id: header.form_id,
                    edid: String::new(),
                    full: String::new(),
                    ty: 0,
                    cost: 0,
                    level: 0,
                    flags: 0,
                    effects: Vec::new(),
                };
                for sub in &subs {
                    match &sub.sub_type {
                        b"EDID" => spell.edid = zstr(&sub.data),
                        b"FULL" => spell.full = zstr(&sub.data),
                        b"SPIT" if sub.data.len() >= 16 => {
                            spell.ty = sub.data[0];
                            spell.cost = u32_at(&sub.data, 4).unwrap_or(0);
                            spell.level = sub.data[8];
                            spell.flags = sub.data[12];
                        }
                        b"EFID" if sub.data.len() >= 4 => {
                            spell.effects.push(Effect {
                                code: sub.data[..4].try_into().unwrap(),
                                magnitude: 0,
                                area: 0,
                                duration: 0,
                                range: 0,
                                script_school: None,
                            });
                        }
                        b"EFIT" if sub.data.len() >= 20 => {
                            if let Some(effect) = spell.effects.last_mut() {
                                effect.magnitude = u32_at(&sub.data, 4).unwrap_or(0);
                                effect.area = u32_at(&sub.data, 8).unwrap_or(0);
                                effect.duration = u32_at(&sub.data, 12).unwrap_or(0);
                                effect.range = u32_at(&sub.data, 16).unwrap_or(0);
                            }
                        }
                        b"SCIT" if sub.data.len() >= 8 => {
                            if let Some(effect) = spell.effects.last_mut() {
                                effect.script_school = u32_at(&sub.data, 4);
                            }
                        }
                        _ => {}
                    }
                }
                spells.push(spell);
            },
        )?;
    }

    println!(
        "SPEL records: {}   MGEF records: {}",
        spells.len(),
        mgefs.len()
    );

    // 1. type x level x manual flag
    let mut by_type: BTreeMap<(u8, u8, bool), usize> = BTreeMap::new();
    for s in &spells {
        *by_type
            .entry((s.ty, s.level, s.flags & 1 != 0))
            .or_default() += 1;
    }
    println!("\n== type / level / Manual Spell Cost -> count");
    for ((ty, level, manual), n) in &by_type {
        println!(
            "  {:12} {:10} manual={:5} {}",
            TYPES.get(*ty as usize).unwrap_or(&"?"),
            LEVELS.get(*level as usize).unwrap_or(&"?"),
            manual,
            n
        );
    }

    // 2. flags
    let mut flag_counts = [0usize; 8];
    for s in &spells {
        for (bit, count) in flag_counts.iter_mut().enumerate() {
            if s.flags & (1 << bit) != 0 {
                *count += 1;
            }
        }
    }
    println!("\n== SPIT flag bits set (0 Manual Cost, 1 ImmuneSilence1, 2 PCStart, 3 ImmuneSilence2, 4 AreaIgnoresLOS, 5 ScriptAlways, 6 NoAbsorbReflect, 7 TouchExplodes)");
    println!("  {:?}", flag_counts);

    // 3. effects: range, area, school source
    let (mut by_range, mut area_gt0, mut script_effects, mut unknown_mgef, mut total) =
        ([0usize; 3], 0usize, 0usize, 0usize, 0usize);
    let mut school_none_nonscript = 0usize;
    for s in spells.iter().filter(|s| s.ty == 0 || s.ty == 3) {
        for e in &s.effects {
            total += 1;
            if let Some(slot) = by_range.get_mut(e.range as usize) {
                *slot += 1;
            }
            area_gt0 += (e.area > 0) as usize;
            script_effects += e.script_school.is_some() as usize;
            match mgefs.get(&e.code) {
                None => unknown_mgef += 1,
                Some(m) if m.school >= 6 && e.script_school.is_none() => school_none_nonscript += 1,
                _ => {}
            }
        }
    }
    println!("\n== effects of Spell / LesserPower records: {total}");
    println!("  range Self {} Touch {} Target {}   area>0: {}   with SCIT: {}   MGEF not found: {}   MGEF school None and no SCIT: {}", by_range[0], by_range[1], by_range[2], area_gt0, script_effects, unknown_mgef, school_none_nonscript);

    // 4. stored cost vs equation for non-manual Spell records; level vs thresholds
    let (mut compared, mut exact, mut within1, mut manual_compared) =
        (0usize, 0usize, 0usize, 0usize);
    let mut worst: Vec<(f32, &Spell, f32)> = Vec::new();
    let mut level_by_cost: BTreeMap<u8, (u32, u32)> = BTreeMap::new();
    let mut threshold_mismatch = 0usize;
    let mut mismatch_shown = 0usize;
    for s in spells.iter().filter(|s| s.ty == 0) {
        let computed: f32 = s
            .effects
            .iter()
            .map(|e| effect_cost(e, mgefs.get(&e.code).map_or(0.0, |m| m.base_cost)))
            .sum();
        if s.flags & 1 != 0 {
            manual_compared += 1;
        } else {
            compared += 1;
            let rounded = computed.floor();
            let diff = (rounded - s.cost as f32).abs();
            exact += (diff < 0.5) as usize;
            within1 += (diff <= 1.0) as usize;
            worst.push((diff, s, computed));
        }
        let entry = level_by_cost.entry(s.level).or_insert((u32::MAX, 0));
        entry.0 = entry.0.min(s.cost);
        entry.1 = entry.1.max(s.cost);
        let level_cost = if s.flags & 1 != 0 {
            s.cost
        } else {
            computed.floor().min(u32::MAX as f32) as u32
        };
        let expected = match level_cost {
            0..=25 => 0,
            26..=62 => 1,
            63..=149 => 2,
            150..=399 => 3,
            _ => 4,
        };
        if expected != s.level {
            threshold_mismatch += 1;
            if mismatch_shown < 8 && s.flags & 1 == 0 && computed < 1.0e6 {
                mismatch_shown += 1;
                println!(
                    "  level mismatch: {:34} Level {:10} but equation cost {:.1} -> {}",
                    s.edid,
                    LEVELS.get(s.level as usize).unwrap_or(&"?"),
                    computed,
                    LEVELS[expected as usize]
                );
            }
        }
    }
    println!("\n== stored SPIT cost vs the UESP effect-cost equation (Spell type, Manual Spell Cost clear): {compared} compared, {exact} equal after flooring, {within1} within 1; {manual_compared} Manual Spell Cost spells skipped");
    worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    for (diff, s, computed) in worst.iter().take(6) {
        println!(
            "  worst: {:28} stored {:5} computed {:8.2} (diff {:.1})  {}",
            s.edid, s.cost, computed, diff, s.full
        );
    }
    println!("\n== SPIT Level vs the stored cost range (Spell type), and spells whose Level differs from the UESP thresholds applied to the equation cost (Manual: stored cost): {threshold_mismatch}");
    for (level, (lo, hi)) in &level_by_cost {
        println!(
            "  {:10} cost {}..{}",
            LEVELS.get(*level as usize).unwrap_or(&"?"),
            lo,
            hi
        );
    }

    // 5. school of the highest-cost effect vs spells with several schools
    let mut multi_school = Vec::new();
    for s in spells.iter().filter(|s| s.ty == 0 && s.effects.len() > 1) {
        let mut schools: Vec<u32> = s
            .effects
            .iter()
            .filter_map(|e| {
                e.script_school
                    .or_else(|| mgefs.get(&e.code).map(|m| m.school))
            })
            .collect();
        schools.sort_unstable();
        schools.dedup();
        if schools.len() > 1 {
            multi_school.push(s);
        }
    }
    println!(
        "\n== Spell records with effects from more than one school: {}",
        multi_school.len()
    );
    for s in multi_school.iter().take(6) {
        let parts: Vec<String> = s
            .effects
            .iter()
            .map(|e| {
                let school = e
                    .script_school
                    .or_else(|| mgefs.get(&e.code).map(|m| m.school))
                    .unwrap_or(6);
                format!(
                    "{}:{}:{}",
                    String::from_utf8_lossy(&e.code),
                    SCHOOLS.get(school as usize).unwrap_or(&"?"),
                    RANGES.get(e.range as usize).unwrap_or(&"?")
                )
            })
            .collect();
        println!("  {:28} {}  [{}]", s.edid, s.full, parts.join(" "));
    }

    // 6. test-spell candidates
    let describe = |s: &Spell| {
        let parts: Vec<String> = s
            .effects
            .iter()
            .map(|e| {
                let m = mgefs.get(&e.code);
                format!(
                    "{}({}) {}x{}s a{} {}",
                    String::from_utf8_lossy(&e.code),
                    m.map_or("?", |m| SCHOOLS
                        .get(m.school as usize)
                        .copied()
                        .unwrap_or("?")),
                    e.magnitude,
                    e.duration,
                    e.area,
                    RANGES.get(e.range as usize).unwrap_or(&"?")
                )
            })
            .collect();
        format!(
            "{:28} {:28} {:10} cost {:4} flags {:02x}  {}",
            s.edid,
            s.full,
            LEVELS.get(s.level as usize).unwrap_or(&"?"),
            s.cost,
            s.flags,
            parts.join("; ")
        )
    };
    println!("\n== candidates (Spell type)");
    let pick = |label: &str, filter: &dyn Fn(&Spell) -> bool| {
        println!("  -- {label}");
        for s in spells.iter().filter(|s| s.ty == 0 && filter(s)).take(4) {
            println!("     {}", describe(s));
        }
    };
    pick("single effect, Self", &|s| {
        s.effects.len() == 1 && s.effects[0].range == 0 && s.flags & 1 == 0
    });
    pick("single effect, Target, Destruction", &|s| {
        s.effects.len() == 1
            && s.effects[0].range == 2
            && mgefs.get(&s.effects[0].code).is_some_and(|m| m.school == 2)
    });
    pick("single effect, Touch", &|s| {
        s.effects.len() == 1 && s.effects[0].range == 1
    });
    pick("Manual Spell Cost", &|s| s.flags & 1 != 0);
    pick("area > 0", &|s| s.effects.iter().any(|e| e.area > 0));
    pick("script effect (SCIT)", &|s| {
        s.effects.iter().any(|e| e.script_school.is_some())
    });
    pick("Touch Spell Explodes w/ no Target (bit 7)", &|s| {
        s.flags & 0x80 != 0
    });

    // 7. named spells: every further argument is an EditorID substring; stored vs computed cost per spell
    for needle in std::env::args().skip(2) {
        println!("\n== spells matching `{needle}`");
        for s in spells.iter().filter(|s| {
            s.edid
                .to_ascii_lowercase()
                .contains(&needle.to_ascii_lowercase())
        }) {
            let computed: f32 = s
                .effects
                .iter()
                .map(|e| effect_cost(e, mgefs.get(&e.code).map_or(0.0, |m| m.base_cost)))
                .sum();
            println!("  {}  computed {:.2}", describe(s), computed);
        }
    }

    // 8. single-effect, non-manual, no-area Spell records: the stored cost against the equation
    println!("\n== single-effect, non-manual, area 0 Spell records (stored vs computed)");
    let mut shown = 0usize;
    let (mut n, mut within_half, mut within_one, mut ratio_sum) = (0usize, 0usize, 0usize, 0.0f64);
    for s in spells.iter().filter(|s| {
        s.ty == 0
            && s.flags & 1 == 0
            && s.effects.len() == 1
            && s.effects[0].area == 0
            && mgefs.contains_key(&s.effects[0].code)
    }) {
        let computed = effect_cost(&s.effects[0], mgefs[&s.effects[0].code].base_cost);
        n += 1;
        let diff = (computed - s.cost as f32).abs();
        within_half += (computed.floor() == s.cost as f32) as usize;
        within_one += (diff <= 1.0) as usize;
        if computed > 0.0 {
            ratio_sum += s.cost as f64 / computed as f64;
        }
        if diff > 1.0 && shown < 8 {
            shown += 1;
            println!(
                "  off by {:6.2}: {}  computed {:.2}",
                diff,
                describe(s),
                computed
            );
        }
    }
    println!("  {n} spells: {within_half} equal after flooring, {within_one} within 1.0, mean stored/computed {:.3}", ratio_sum / n.max(1) as f64);

    // 9. machine-readable: form id, flags, stored cost, equation cost (for comparison with published in-game costs)
    if std::env::var("SPELL_CENSUS_DUMP").is_ok() {
        for s in spells.iter().filter(|s| s.ty == 0) {
            let computed: f32 = s
                .effects
                .iter()
                .map(|e| effect_cost(e, mgefs.get(&e.code).map_or(0.0, |m| m.base_cost)))
                .sum();
            println!(
                "SPELL {:08X} {} stored {} computed {:.3} edid {} level {}",
                s.form_id, s.flags, s.cost, computed, s.edid, s.level
            );
        }
    }
    Ok(())
}
