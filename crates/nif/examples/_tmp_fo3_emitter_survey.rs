//! Throwaway (FO3 audit dim2): survey authored particle-emitter decode
//! across a whole BSA. Reports how many NIFs carry a NiPSysEmitter and
//! how many yield params / rate / grow-fade base_scale downstream.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::particle::{
    NiPSysEmitter, NiPSysEmitterCtlr, NiPSysEmitterCtlrData, NiPSysGrowFadeModifier,
};
use byroredux_nif::parse_nif;

fn main() {
    let path = std::env::args().nth(1).expect("usage: <bsa>");
    let archive = BsaArchive::open(&path).expect("open");
    let files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();

    let (mut with_emitter, mut params_some, mut rate_some, mut gf_blocks, mut gf_scale_some) =
        (0u32, 0u32, 0u32, 0u32, 0u32);
    let mut ctlr_blocks = 0u32;
    let mut ctlrdata_blocks = 0u32;
    let mut bsvers: std::collections::BTreeMap<u32, u32> = Default::default();
    let mut rejected: Vec<String> = Vec::new();
    let mut rate_missing_with_ctlr: Vec<String> = Vec::new();
    let mut examples: Vec<String> = Vec::new();

    for name in &files {
        let Ok(bytes) = archive.extract(name) else { continue };
        let bsver = byroredux_nif::header::NifHeader::parse(&bytes)
            .map(|(h, _)| h.user_version_2)
            .unwrap_or(0);
        let Ok(scene) = parse_nif(&bytes) else { continue };
        let has_em = scene
            .blocks
            .iter()
            .any(|b| b.as_any().downcast_ref::<NiPSysEmitter>().is_some());
        let has_ctlr = scene
            .blocks
            .iter()
            .any(|b| b.as_any().downcast_ref::<NiPSysEmitterCtlr>().is_some());
        let has_ctlrdata = scene
            .blocks
            .iter()
            .any(|b| b.as_any().downcast_ref::<NiPSysEmitterCtlrData>().is_some());
        let gf: Vec<_> = scene
            .blocks
            .iter()
            .filter_map(|b| b.as_any().downcast_ref::<NiPSysGrowFadeModifier>())
            .collect();
        if has_ctlr { ctlr_blocks += 1; }
        if has_ctlrdata { ctlrdata_blocks += 1; }
        if !gf.is_empty() {
            gf_blocks += 1;
            if gf.iter().any(|m| m.base_scale.is_some()) { gf_scale_some += 1; }
        }
        if !has_em && !has_ctlr { continue; }
        if has_em {
            with_emitter += 1;
            *bsvers.entry(bsver).or_insert(0) += 1;
        }
        let mut pool = byroredux_core::string::StringPool::new();
        let imported = byroredux_nif::import::import_nif_scene(&scene, &mut pool);
        let p = imported.particle_emitters.iter().find(|e| e.emitter_params.is_some());
        if p.is_some() { params_some += 1; }
        else if has_em && rejected.len() < 20 { rejected.push(name.clone()); }
        let r = imported.particle_emitters.iter().find(|e| e.emitter_rate.is_some());
        if r.is_some() { rate_some += 1; }
        else if has_ctlr && rate_missing_with_ctlr.len() < 20 { rate_missing_with_ctlr.push(name.clone()); }
        if examples.len() < 12 {
            if let Some(e) = imported.particle_emitters.first() {
                if let Some(pp) = e.emitter_params {
                    examples.push(format!(
                        "{name}: bsver={bsver} rate={:?} speed={:.2} spdVar={:.2} decl={:.3} radius={:.2} radVar={:.2} life={:.2} lifeVar={:.2} bscale={:?} color={:?}",
                        e.emitter_rate, pp.speed, pp.speed_variation, pp.declination,
                        pp.initial_radius, pp.radius_variation, pp.life_span,
                        pp.life_span_variation, pp.base_scale, pp.initial_color));
                }
            }
        }
    }
    println!("files={} with_emitter={} params_some={} rate_some={}", files.len(), with_emitter, params_some, rate_some);
    println!("ctlr_files={} ctlrdata_files={} growfade_files={} growfade_with_base_scale={}", ctlr_blocks, ctlrdata_blocks, gf_blocks, gf_scale_some);
    println!("bsver distribution among emitter NIFs: {:?}", bsvers);
    println!("-- emitter present but params REJECTED ({}+ shown):", rejected.len());
    for r in &rejected { println!("   {r}"); }
    println!("-- ctlr present but rate None ({}+ shown):", rate_missing_with_ctlr.len());
    for r in &rate_missing_with_ctlr { println!("   {r}"); }
    println!("-- samples:");
    for e in &examples { println!("   {e}"); }
}
