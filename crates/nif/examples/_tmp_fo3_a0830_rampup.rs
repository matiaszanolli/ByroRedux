//! TEMP scratch (audit 2026-08-30): FO3 corpus count of emitter meshes whose
//! authored birth-rate curve starts at 0 and is therefore rejected wholesale
//! by float_interpolator_rate's first-key-only read.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::controller::NiControllerSequence;
use byroredux_nif::blocks::interpolator::{NiFloatData, NiFloatInterpolator};
use byroredux_nif::blocks::properties::NiStringPalette;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

fn main() {
    let root = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data";
    let archives = [
        "Fallout - Meshes.bsa",
        "Anchorage - Main.bsa",
        "BrokenSteel - Main.bsa",
        "PointLookout - Main.bsa",
        "ThePitt - Main.bsa",
        "Zeta - Main.bsa",
    ];
    let mut em_files = 0usize;
    let mut no_rate = 0usize;
    let mut rampup_recoverable = 0usize;
    let mut peak_hist: BTreeMap<String, usize> = BTreeMap::new();
    let mut ex: Vec<String> = Vec::new();

    for arc_name in archives {
        let Ok(arc) = BsaArchive::open(&format!("{root}/{arc_name}")) else {
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
            let ems = byroredux_nif::import::import_nif_particle_emitters(&scene);
            if ems.is_empty() {
                continue;
            }
            em_files += 1;
            if ems.iter().any(|e| e.emitter_rate.is_some()) {
                continue;
            }
            no_rate += 1;
            // Walk every sequence's emitter-controller controlled blocks and
            // look for an authored curve whose FIRST key is 0 but whose peak
            // is positive — the shape float_interpolator_rate throws away.
            let mut peak = 0.0f32;
            for b in scene.blocks.iter() {
                let Some(seq) = b.as_any().downcast_ref::<NiControllerSequence>() else {
                    continue;
                };
                for cb in &seq.controlled_blocks {
                    let ct = cb
                        .controller_type
                        .as_ref()
                        .map(|s| s.to_string())
                        .or_else(|| {
                            cb.string_palette_ref
                                .index()
                                .and_then(|p| scene.get_as::<NiStringPalette>(p))
                                .and_then(|p| p.get_string(cb.controller_type_offset))
                                .map(|s| s.to_string())
                        });
                    if ct.as_deref().map(|s| s.contains("EmitterCtlr")) != Some(true) {
                        continue;
                    }
                    let Some(i) = cb.interpolator_ref.index() else {
                        continue;
                    };
                    let Some(fi) = scene.get_as::<NiFloatInterpolator>(i) else {
                        continue;
                    };
                    let Some(d) = fi
                        .data_ref
                        .index()
                        .and_then(|d| scene.get_as::<NiFloatData>(d))
                    else {
                        continue;
                    };
                    let keys: Vec<f32> = d.keys.keys.iter().map(|k| k.value).collect();
                    if keys.first().copied() != Some(0.0) {
                        continue;
                    }
                    for v in &keys {
                        if v.is_finite() && *v > peak {
                            peak = *v;
                        }
                    }
                }
            }
            if peak > 0.0 {
                rampup_recoverable += 1;
                *peak_hist.entry(format!("{peak:.1}")).or_default() += 1;
                if ex.len() < 12 {
                    ex.push(format!("{name} peak={peak}"));
                }
            }
        }
    }
    println!("FO3 emitter-bearing meshes = {em_files}");
    println!("  extract_emitter_rate == None = {no_rate}");
    println!("  ...with an authored ramp-up curve whose peak > 0 = {rampup_recoverable}");
    println!("  peak-rate histogram = {peak_hist:?}");
    for e in &ex {
        println!("    {e}");
    }
}
