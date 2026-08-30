//! TEMP scratch (audit 2026-08-30): FO3 mixed NiTexturingProperty +
//! BSShader* chain census — measures the #3517 exposure precisely.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::properties::NiTexturingProperty;
use byroredux_nif::blocks::shader::{
    BSShaderNoLightingProperty, BSShaderPPLightingProperty, SkyShaderProperty,
    TallGrassShaderProperty, TileShaderProperty, WaterShaderProperty,
};
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

fn bs_clamp(a: &dyn std::any::Any) -> Option<u32> {
    if let Some(s) = a.downcast_ref::<BSShaderPPLightingProperty>() {
        return Some(s.texture_clamp_mode);
    }
    if let Some(s) = a.downcast_ref::<BSShaderNoLightingProperty>() {
        return Some(s.texture_clamp_mode);
    }
    if let Some(s) = a.downcast_ref::<TileShaderProperty>() {
        return Some(s.texture_clamp_mode);
    }
    if let Some(s) = a.downcast_ref::<SkyShaderProperty>() {
        return Some(s.texture_clamp_mode);
    }
    if a.downcast_ref::<TallGrassShaderProperty>().is_some() {
        return None;
    }
    if a.downcast_ref::<WaterShaderProperty>().is_some() {
        return None;
    }
    None
}

fn main() {
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data".into());
    let archives = [
        "Fallout - Meshes.bsa",
        "Anchorage - Main.bsa",
        "BrokenSteel - Main.bsa",
        "PointLookout - Main.bsa",
        "ThePitt - Main.bsa",
        "Zeta - Main.bsa",
    ];

    let mut mixed = 0usize;
    // (ni_clamp, bs_clamp) pairs on mixed shapes
    let mut pair_hist: BTreeMap<(i32, i32), usize> = BTreeMap::new();
    let mut order_ni_first = 0usize;
    let mut order_bs_first = 0usize;
    // final value under the CURRENT code, simulated
    let mut final_hist: BTreeMap<u8, usize> = BTreeMap::new();
    // final value under a "shape's first writer wins / latch" policy
    let mut latch_hist: BTreeMap<u8, usize> = BTreeMap::new();
    let mut divergent = 0usize;
    let mut examples: Vec<String> = Vec::new();
    // Standalone BSShader clamp distribution (all shapes)
    let mut bs_only_clamp: BTreeMap<u32, usize> = BTreeMap::new();

    for arc_name in archives {
        let path = format!("{root}/{arc_name}");
        let Ok(arc) = BsaArchive::open(&path) else {
            continue;
        };
        for name in arc
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
        {
            let Ok(bytes) = arc.extract(name) else {
                continue;
            };
            let Ok(scene) = parse_nif(&bytes) else {
                continue;
            };
            for block in scene.blocks.iter() {
                let tn = block.block_type_name();
                if !(tn.contains("TriShape") || tn.contains("TriStrips")) {
                    continue;
                }
                let Some(av) = block.as_av_object() else {
                    continue;
                };
                let mut ni: Option<(usize, Option<u8>)> = None;
                let mut bs: Option<(usize, Option<u32>)> = None;
                for (pos, r) in av.properties().iter().enumerate() {
                    let Some(idx) = r.index() else { continue };
                    let Some(b) = scene.blocks.get(idx) else {
                        continue;
                    };
                    let a = b.as_any();
                    if let Some(tp) = a.downcast_ref::<NiTexturingProperty>() {
                        if ni.is_none() {
                            ni = Some((pos, tp.base_texture.as_ref().map(|d| d.clamp_mode)));
                        }
                    } else if let Some(c) = bs_clamp(a) {
                        if bs.is_none() {
                            bs = Some((pos, Some(c)));
                        }
                    }
                }
                if let (None, Some((_, Some(c)))) = (&ni, &bs) {
                    *bs_only_clamp.entry(*c).or_default() += 1;
                }
                let (Some((npos, nclamp)), Some((bpos, Some(bclamp)))) = (ni, bs) else {
                    continue;
                };
                mixed += 1;
                if npos < bpos {
                    order_ni_first += 1;
                } else {
                    order_bs_first += 1;
                }
                let nc = nclamp.map(|v| v as i32).unwrap_or(-1);
                *pair_hist.entry((nc, bclamp as i32)).or_default() += 1;

                // Simulate the current chain: iterate properties in order,
                // NiTexturingProperty arm = `if cur == 3 { cur = base.clamp }`
                // (no latch), BSShader arm = `if !consumed { cur = bs; consumed = true }`.
                let mut cur: u8 = 3;
                let mut consumed = false;
                for (pos, r) in av.properties().iter().enumerate() {
                    let Some(idx) = r.index() else { continue };
                    let Some(b) = scene.blocks.get(idx) else {
                        continue;
                    };
                    let a = b.as_any();
                    if a.downcast_ref::<NiTexturingProperty>().is_some() {
                        if pos == npos {
                            if cur == 3 {
                                if let Some(v) = nclamp {
                                    cur = v;
                                }
                            }
                        }
                    } else if bs_clamp(a).is_some() && pos == bpos && !consumed {
                        cur = bclamp as u8;
                        consumed = true;
                    }
                }
                *final_hist.entry(cur).or_default() += 1;

                // Latch policy (the #3517 suggested fix): first writer wins.
                let latch = if npos < bpos {
                    nclamp.unwrap_or(3)
                } else {
                    bclamp as u8
                };
                *latch_hist.entry(latch).or_default() += 1;
                if latch != cur {
                    divergent += 1;
                    if examples.len() < 8 {
                        examples.push(format!(
                            "{name} shape='{}' ni@{npos}={:?} bs@{bpos}={bclamp} current={cur} latch={latch}",
                            block.block_type_name(),
                            nclamp
                        ));
                    }
                }
            }
        }
        eprintln!("done {arc_name}");
    }

    println!("mixed shapes = {mixed}");
    println!("  order: ni_first={order_ni_first} bs_first={order_bs_first}");
    println!("  (ni_clamp, bs_clamp) hist = {pair_hist:?}");
    println!("  final clamp under CURRENT code = {final_hist:?}");
    println!("  final clamp under LATCH policy  = {latch_hist:?}");
    println!("  divergent shapes = {divergent}");
    for e in &examples {
        println!("    {e}");
    }
    println!("BSShader-only shape clamp hist = {bs_only_clamp:?}");
}
