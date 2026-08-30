//! TEMP scratch (audit 2026-08-30): does a LATER NiPSysEmitterCtlr resolve a
//! rate on the FO3 meshes where extract_emitter_rate returns None?
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::interpolator::{
    NiBlendFloatInterpolator, NiFloatData, NiFloatInterpolator,
};
use byroredux_nif::blocks::particle::NiPSysEmitterCtlr;
use byroredux_nif::parse_nif;
use byroredux_nif::scene::NifScene;

fn resolve_blend_interpolator_target(scene: &NifScene, interp_idx: usize) -> Option<usize> {
    let b = scene.get_as::<NiBlendFloatInterpolator>(interp_idx)?;
    b.base
        .items
        .iter()
        .filter_map(|it| {
            it.interpolator_ref
                .index()
                .map(|i| (i, it.normalized_weight))
        })
        .max_by(|a, c| a.1.partial_cmp(&c.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
}

fn sane(r: f32) -> Option<f32> {
    (r.is_finite() && 0.0 < r && r < 3.0e38).then_some(r)
}

fn float_interp_rate(scene: &NifScene, idx: usize) -> Option<f32> {
    let interp = scene.get_as::<NiFloatInterpolator>(idx)?;
    if let Some(d) = interp.data_ref.index() {
        if let Some(first) = scene
            .get_as::<NiFloatData>(d)
            .and_then(|x| x.keys.keys.first())
        {
            if let Some(r) = sane(first.value) {
                return Some(r);
            }
        }
    }
    sane(interp.value)
}

/// Same tier chain as `extract_emitter_rate`, but for ONE named controller.
fn rate_for_ctlr(scene: &NifScene, ctlr: &NiPSysEmitterCtlr) -> Option<f32> {
    let idx = ctlr.interpolator_ref.index()?;
    if let Some(r) = float_interp_rate(scene, idx) {
        return Some(r);
    }
    if let Some(sub) = resolve_blend_interpolator_target(scene, idx) {
        if let Some(r) = float_interp_rate(scene, sub) {
            return Some(r);
        }
    }
    if let Some(b) = scene.get_as::<NiBlendFloatInterpolator>(idx) {
        if let Some(r) = sane(b.value) {
            return Some(r);
        }
    }
    None
}

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
    let mut gap_files = 0usize;
    let mut recoverable = 0usize;
    let mut ex: Vec<String> = Vec::new();
    let mut seq_hist: std::collections::BTreeMap<(usize, usize), usize> =
        std::collections::BTreeMap::new();
    let mut interp_hist: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
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
            if ems.is_empty() || ems.iter().any(|e| e.emitter_rate.is_some()) {
                continue;
            }
            let ctlrs: Vec<&NiPSysEmitterCtlr> = scene
                .blocks
                .iter()
                .filter_map(|b| b.as_any().downcast_ref::<NiPSysEmitterCtlr>())
                .collect();
            if ctlrs.is_empty() {
                continue;
            }
            gap_files += 1;
            let nseq = scene
                .blocks
                .iter()
                .filter(|b| b.block_type_name() == "NiControllerSequence")
                .count();
            let nmgr = scene
                .blocks
                .iter()
                .filter(|b| b.block_type_name().contains("ControllerManager"))
                .count();
            *seq_hist.entry((nseq, nmgr)).or_default() += 1;
            let rates: Vec<Option<f32>> = ctlrs.iter().map(|c| rate_for_ctlr(&scene, c)).collect();
            for c in &ctlrs {
                let t = c
                    .interpolator_ref
                    .index()
                    .and_then(|i| scene.blocks.get(i))
                    .map(|b| b.block_type_name().to_string())
                    .unwrap_or_else(|| "<null-ref>".into());
                let v = c
                    .interpolator_ref
                    .index()
                    .and_then(|i| scene.get_as::<NiFloatInterpolator>(i).map(|x| x.value));
                let bv = c.interpolator_ref.index().and_then(|i| {
                    scene
                        .get_as::<NiBlendFloatInterpolator>(i)
                        .map(|x| (x.value, x.base.items.len()))
                });
                *interp_hist
                    .entry(format!("{t} float_value={v:?} blend={bv:?}"))
                    .or_default() += 1;
            }
            if rates.iter().skip(1).any(|r| r.is_some()) {
                recoverable += 1;
                if ex.len() < 12 {
                    ex.push(format!("{name}  per-controller rates = {rates:?}"));
                }
            }
        }
    }
    println!("FO3 emitter files with a controller but no extracted rate = {gap_files}");
    println!("  ...of which a LATER controller resolves a rate = {recoverable}");
    for e in &ex {
        println!("    {e}");
    }
    println!("gap-set (NiControllerSequence, ControllerManager) counts: {seq_hist:?}");
    println!("gap-set interpolator shapes:");
    for (k, v) in &interp_hist {
        println!("   {v:5}  {k}");
    }
}
