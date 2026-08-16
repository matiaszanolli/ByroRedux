//! TEMP scratch (audit 2026-08-16): Skyrim slot-role vs SLSF flag survey.
//! `slot_to_role` branches ONLY on `shader_type`; nif.xml's BSShaderTextureSet
//! doc attributes several slots to SLSF *flags* instead. Measure the disagreement.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::shader::{BSLightingShaderProperty, BSShaderTextureSet};
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

// SLSF1
const F1_FACEGEN_DETAIL: u32 = 1 << 10;
const F1_MSN: u32 = 1 << 12;
const F1_FACEGEN_RGB_TINT: u32 = 1 << 21;
// SLSF2
const F2_GLOW_MAP: u32 = 1 << 6;
const F2_MULTILAYER: u32 = 1 << 24;
const F2_SOFT_LIGHTING: u32 = 1 << 25;
const F2_RIM_LIGHTING: u32 = 1 << 26;
const F2_BACK_LIGHTING: u32 = 1 << 27;

fn main() {
    let mut props = 0usize;
    // (case) -> count
    let mut cases: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut samples: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    let mut note = |cases: &mut BTreeMap<&'static str, usize>,
                    samples: &mut BTreeMap<&'static str, Vec<String>>,
                    k: &'static str,
                    s: String| {
        *cases.entry(k).or_default() += 1;
        let e = samples.entry(k).or_default();
        if e.len() < 6 {
            e.push(s);
        }
    };

    for path in std::env::args().skip(1) {
        let Ok(arc) = BsaArchive::open(&path) else {
            eprintln!("open fail {path}");
            continue;
        };
        let names: Vec<String> = arc
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .collect();
        eprintln!("{path}: {} nifs", names.len());
        for name in &names {
            let Ok(bytes) = arc.extract(name) else { continue };
            let Ok(scene) = parse_nif(&bytes) else { continue };
            if scene.bsver >= 130 {
                continue;
            }
            for block in scene.blocks.iter() {
                let Some(sh) = block.as_any().downcast_ref::<BSLightingShaderProperty>() else {
                    continue;
                };
                props += 1;
                let ts = sh
                    .texture_set_ref
                    .index()
                    .and_then(|i| scene.get_as::<BSShaderTextureSet>(i))
                    .map(|t| t.textures.clone())
                    .unwrap_or_default();
                let get = |i: usize| -> &str { ts.get(i).map(|s| s.as_str()).unwrap_or("") };
                let ty = sh.shader_type;
                let f1 = sh.shader_flags_1;
                let f2 = sh.shader_flags_2;
                let tint_family = matches!(ty, 4 | 5 | 6);
                let msn = f1 & F1_MSN != 0;
                let ec = sh.emissive_color;
                let ec_lit = ec[0] > 0.0 || ec[1] > 0.0 || ec[2] > 0.0;

                // slot 2 -> Emissive today for non-tint types.
                if !get(2).is_empty() && !tint_family {
                    if f2 & F2_GLOW_MAP != 0 {
                        note(&mut cases, &mut samples, "AA slot2 authored, GLOW_MAP SET (correct)", format!("{name} ty={ty} tex={}", get(2)));
                    } else {
                        note(&mut cases, &mut samples, "AB slot2 authored, GLOW_MAP CLEAR (mis-roled)", format!("{name} ty={ty} ec={ec:?} tex={}", get(2)));
                        if ec_lit {
                            note(&mut cases, &mut samples, "AC slot2 GLOW_MAP CLEAR + lit emissive_color (LIVE)", format!("{name} ty={ty} ec={ec:?} em={} tex={}", sh.emissive_multiple, get(2)));
                        }
                    }
                    if f2 & F2_RIM_LIGHTING != 0 && f2 & F2_GLOW_MAP == 0 {
                        note(&mut cases, &mut samples, "slot2 RIM->glow_map", format!("{name} ty={ty} ec={ec:?} em={} tex={}", sh.emissive_multiple, get(2)));
                        if ec_lit {
                            note(&mut cases, &mut samples, "slot2 RIM->glow_map WITH LIT emissive_color", format!("{name} ty={ty} ec={ec:?} em={} tex={}", sh.emissive_multiple, get(2)));
                        }
                    }
                    if f2 & F2_SOFT_LIGHTING != 0 && f2 & F2_GLOW_MAP == 0 {
                        note(&mut cases, &mut samples, "slot2 SOFT->glow_map", format!("{name} ty={ty} ec={ec:?} em={} tex={}", sh.emissive_multiple, get(2)));
                        if ec_lit {
                            note(&mut cases, &mut samples, "slot2 SOFT->glow_map WITH LIT emissive_color", format!("{name} ty={ty} ec={ec:?} em={} tex={}", sh.emissive_multiple, get(2)));
                        }
                    }
                    if f1 & F1_FACEGEN_RGB_TINT != 0 {
                        note(&mut cases, &mut samples, "slot2 FGRGBTINT->glow_map", format!("{name} ty={ty} tex={}", get(2)));
                    }
                }
                // slot 3 -> Height (POM) today for non-FaceTint types.
                if !get(3).is_empty() && ty != 4 && f1 & F1_FACEGEN_DETAIL != 0 {
                    note(&mut cases, &mut samples, "slot3 FACEGENDETAIL->parallax", format!("{name} ty={ty} tex={}", get(3)));
                }
                // slot 7 -> Specular today when MSN.
                if !get(7).is_empty() && msn && ty != 11 && f2 & F2_BACK_LIGHTING != 0 {
                    note(&mut cases, &mut samples, "slot7 BACKLIGHT+MSN->specular", format!("{name} ty={ty} tex={}", get(7)));
                }
                // slot 7 non-empty without MSN and not type 11 -> dropped today.
                if !get(7).is_empty() && !msn && ty != 11 {
                    note(&mut cases, &mut samples, "slot7 no-MSN -> DROPPED", format!("{name} ty={ty} f2bl={} tex={}", f2 & F2_BACK_LIGHTING != 0, get(7)));
                }
                // slot 6 non-empty on non-type-11, non-FaceTint -> dropped today.
                if !get(6).is_empty() && ty != 11 && ty != 4 {
                    note(&mut cases, &mut samples, "slot6 non-MLP -> DROPPED", format!("{name} ty={ty} f2mlp={} tex={}", f2 & F2_MULTILAYER != 0, get(6)));
                }
                // Multi-layer flag set but shader type != 11 (slot 6 would be dropped).
                if f2 & F2_MULTILAYER != 0 && ty != 11 {
                    note(&mut cases, &mut samples, "F2_MULTILAYER but ty!=11", format!("{name} ty={ty} s6={}", get(6)));
                }
                // slot 4/5 authored on tint family -> dropped by design.
                if tint_family && (!get(4).is_empty() || !get(5).is_empty()) {
                    note(&mut cases, &mut samples, "tint-family slot4/5 authored -> DROPPED", format!("{name} ty={ty} s4={} s5={}", get(4), get(5)));
                }
            }
        }
    }
    println!("BSLightingShaderProperty (pre-FO4) parsed: {props}");
    for (k, v) in &cases {
        println!("\n{k}: {v}");
        for s in samples.get(k).unwrap() {
            println!("    {s}");
        }
    }
}
