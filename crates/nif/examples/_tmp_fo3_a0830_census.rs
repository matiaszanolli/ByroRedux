//! TEMP scratch (audit 2026-08-30): FO3 whole-corpus census.
//!
//! Sweeps every mesh-bearing FO3 archive and reports:
//!   * NiTexturingProperty presence + TexDesc.clamp_mode histogram (post-#3516)
//!   * mixed chains (a shape whose property chain holds BOTH a
//!     NiTexturingProperty and a BSShader* property) — #3517 exposure on FO3
//!   * inherited (NiNode-borne) property kinds
//!   * ImportedMaterial: texture_clamp_mode / is_pbr / material_kind / emissive
//!   * particle emitters: params / rate / grow-fade
//!   * B-spline interpolator block counts
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::properties::NiTexturingProperty;
use byroredux_nif::blocks::shader::{
    BSShaderNoLightingProperty, BSShaderPPLightingProperty, SkyShaderProperty,
    TallGrassShaderProperty, TileShaderProperty, WaterShaderProperty,
};
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

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

    let mut nifs = 0usize;
    let mut tex_prop = 0usize;
    let mut clamp_hist: BTreeMap<u8, usize> = BTreeMap::new();
    let mut clamp_raw_low_hist: BTreeMap<u16, usize> = BTreeMap::new();
    let mut apply_mode_hist: BTreeMap<u32, usize> = BTreeMap::new();
    // Per-shape chain composition.
    let mut shapes = 0usize;
    let mut shape_mixed = 0usize; // NiTexturingProperty + BSShader* on the SAME shape
    let mut shape_texprop_only = 0usize;
    let mut shape_bs_only = 0usize;
    let mut shape_neither = 0usize;
    // Inherited (NiNode) property kinds.
    let mut node_prop_kind: BTreeMap<String, usize> = BTreeMap::new();
    // Materials
    let mut meshes = 0usize;
    let mut mat_clamp: BTreeMap<u8, usize> = BTreeMap::new();
    let mut kind_hist: BTreeMap<u32, usize> = BTreeMap::new();
    let mut pbr_true = 0usize;
    let mut emsrc: BTreeMap<String, usize> = BTreeMap::new();
    let mut has_normal = 0usize;
    let mut has_base = 0usize;
    // Emitters
    let mut emitters = 0usize;
    let mut em_params = 0usize;
    let mut em_rate = 0usize;
    let mut em_files = 0usize;
    // Block type histogram of interest
    let mut bspline = 0usize;
    let mut bspline_files = 0usize;

    for arc_name in archives {
        let path = format!("{root}/{arc_name}");
        let Ok(arc) = BsaArchive::open(&path) else {
            eprintln!("skip {arc_name}");
            continue;
        };
        let files: Vec<String> = arc
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .collect();
        for name in &files {
            let Ok(bytes) = arc.extract(name) else {
                continue;
            };
            let Ok(scene) = parse_nif(&bytes) else {
                continue;
            };
            nifs += 1;

            // Classify every block index once.
            let mut is_texprop = vec![false; scene.blocks.len()];
            let mut is_bsshader = vec![false; scene.blocks.len()];
            let mut file_bspline = 0usize;
            for (i, block) in scene.blocks.iter().enumerate() {
                let a = block.as_any();
                if let Some(tp) = a.downcast_ref::<NiTexturingProperty>() {
                    tex_prop += 1;
                    is_texprop[i] = true;
                    *apply_mode_hist.entry(tp.apply_mode).or_default() += 1;
                    for d in [
                        tp.base_texture.as_ref(),
                        tp.detail_texture.as_ref(),
                        tp.normal_texture.as_ref(),
                        tp.glow_texture.as_ref(),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        *clamp_hist.entry(d.clamp_mode).or_default() += 1;
                        *clamp_raw_low_hist.entry(d.flags & 0xF).or_default() += 1;
                    }
                }
                if a.downcast_ref::<BSShaderPPLightingProperty>().is_some()
                    || a.downcast_ref::<BSShaderNoLightingProperty>().is_some()
                    || a.downcast_ref::<TileShaderProperty>().is_some()
                    || a.downcast_ref::<SkyShaderProperty>().is_some()
                    || a.downcast_ref::<TallGrassShaderProperty>().is_some()
                    || a.downcast_ref::<WaterShaderProperty>().is_some()
                {
                    is_bsshader[i] = true;
                }
                let tn = block.block_type_name();
                if tn.contains("BSpline") {
                    file_bspline += 1;
                    bspline += 1;
                }
            }
            if file_bspline > 0 {
                bspline_files += 1;
            }

            // Per-shape + per-node chain composition.
            for (i, block) in scene.blocks.iter().enumerate() {
                let a = block.as_any();
                if block.block_type_name().contains("TriShape")
                    || block.block_type_name().contains("TriStrips")
                {
                    shapes += 1;
                    let Some(av) = block.as_av_object() else {
                        continue;
                    };
                    let mut t = false;
                    let mut b = false;
                    for r in av.properties() {
                        if let Some(idx) = r.index() {
                            if idx < is_texprop.len() {
                                t |= is_texprop[idx];
                                b |= is_bsshader[idx];
                            }
                        }
                    }
                    match (t, b) {
                        (true, true) => shape_mixed += 1,
                        (true, false) => shape_texprop_only += 1,
                        (false, true) => shape_bs_only += 1,
                        (false, false) => shape_neither += 1,
                    }
                } else if block.block_type_name() == "NiNode"
                    || block.block_type_name() == "BSFadeNode"
                {
                    if let Some(av) = block.as_av_object() {
                        for r in av.properties() {
                            if let Some(idx) = r.index() {
                                if idx < scene.blocks.len() {
                                    *node_prop_kind
                                        .entry(scene.blocks[idx].block_type_name().to_string())
                                        .or_default() += 1;
                                }
                            }
                        }
                    }
                }
                let _ = i;
            }

            let mut pool = byroredux_core::string::StringPool::new();
            for m in byroredux_nif::import::import_nif(&scene, &mut pool) {
                meshes += 1;
                *mat_clamp.entry(m.material.texture_clamp_mode).or_default() += 1;
                *kind_hist.entry(m.material.material_kind).or_default() += 1;
                *emsrc
                    .entry(format!("{:?}", m.material.emissive_source))
                    .or_default() += 1;
                if m.material.is_pbr {
                    pbr_true += 1;
                }
                if m.material.textures.base_color.is_some() {
                    has_base += 1;
                }
                if m.material.textures.normal.is_some() {
                    has_normal += 1;
                }
            }
            let ems = byroredux_nif::import::import_nif_particle_emitters(&scene);
            if !ems.is_empty() {
                em_files += 1;
            }
            for e in &ems {
                emitters += 1;
                if e.emitter_params.is_some() {
                    em_params += 1;
                }
                if e.emitter_rate.is_some() {
                    em_rate += 1;
                }
            }
        }
        eprintln!("done {arc_name}");
    }

    println!("nifs={nifs} shapes={shapes} meshes={meshes}");
    println!("NiTexturingProperty blocks={tex_prop}");
    println!("  TexDesc.clamp_mode hist (decoded)  = {clamp_hist:?}");
    println!("  TexDesc raw flags&0xF hist (pre-fix)= {clamp_raw_low_hist:?}");
    println!("  apply_mode hist = {apply_mode_hist:?}");
    println!(
        "shape chains: mixed={shape_mixed} texprop_only={shape_texprop_only} bs_only={shape_bs_only} neither={shape_neither}"
    );
    println!("NiNode-borne property kinds = {node_prop_kind:?}");
    println!("material texture_clamp_mode hist = {mat_clamp:?}");
    println!("material_kind hist = {kind_hist:?}");
    println!("emissive_source hist = {emsrc:?}");
    println!("is_pbr={pbr_true} has_base_tex={has_base} has_normal_tex={has_normal}");
    println!("emitters={emitters} (files={em_files}) params={em_params} rate={em_rate}");
    println!("BSpline blocks={bspline} in {bspline_files} files");
}
