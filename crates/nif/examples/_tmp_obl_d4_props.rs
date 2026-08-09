//! Throwaway (Oblivion audit dim4): property-chain ordering census.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::properties::{
    NiFlagProperty, NiStencilProperty, NiTexturingProperty, NiVertexColorProperty,
};
use byroredux_nif::blocks::tri_shape::NiTriShape;
use byroredux_nif::blocks::NiObject;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("usage: <bsa>");
    let archive = BsaArchive::open(&path).expect("open");
    let files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();

    let mut shapes = 0u64;
    let mut vcp_present = 0u64;
    let mut vcp_after_matprop = 0u64;
    let mut vcp_after_matprop_nondefault = 0u64;
    let mut vcp_before = 0u64;
    let mut shade_zero = 0u64;
    let mut shade_blocks = 0u64;
    let mut wire_blocks = 0u64;
    let mut stencil_draw: BTreeMap<u32, u64> = Default::default();
    let mut clamp_conflict = 0u64;
    let mut multi_texprop = 0u64;
    let mut samples: Vec<String> = Vec::new();
    let mut samples2: Vec<String> = Vec::new();

    for name in &files {
        let Ok(bytes) = archive.extract(name) else {
            continue;
        };
        let Ok(scene) = parse_nif(&bytes) else {
            continue;
        };

        for b in scene.blocks.iter() {
            if let Some(fp) = b.as_any().downcast_ref::<NiFlagProperty>() {
                match fp.block_type_name() {
                    "NiShadeProperty" => {
                        shade_blocks += 1;
                        if !fp.enabled() {
                            shade_zero += 1;
                        }
                    }
                    "NiWireframeProperty" => {
                        wire_blocks += 1;
                    }
                    _ => {}
                }
            }
            if let Some(sp) = b.as_any().downcast_ref::<NiStencilProperty>() {
                *stencil_draw.entry(sp.draw_mode).or_insert(0) += 1;
            }
        }

        for b in scene.blocks.iter() {
            let Some(shape) = b.as_any().downcast_ref::<NiTriShape>() else {
                continue;
            };
            shapes += 1;
            let mut mat_pos: Option<usize> = None;
            let mut vcp_pos: Option<usize> = None;
            let mut vcp_mode: Option<(u32, u32)> = None;
            let mut clamps: Vec<u8> = Vec::new();
            for (i, pr) in shape.av.properties.iter().enumerate() {
                let Some(idx) = pr.index() else { continue };
                let Some(pb) = scene.blocks.get(idx) else {
                    continue;
                };
                match pb.block_type_name() {
                    "NiMaterialProperty" => {
                        if mat_pos.is_none() {
                            mat_pos = Some(i);
                        }
                    }
                    "NiVertexColorProperty" => {
                        if vcp_pos.is_none() {
                            vcp_pos = Some(i);
                        }
                        if let Some(v) = pb.as_any().downcast_ref::<NiVertexColorProperty>() {
                            vcp_mode = Some((v.vertex_mode, v.lighting_mode));
                        }
                    }
                    "NiTexturingProperty" => {
                        if let Some(tp) = pb.as_any().downcast_ref::<NiTexturingProperty>() {
                            if let Some(bt) = tp.base_texture.as_ref() {
                                clamps.push((bt.flags & 0xF) as u8);
                            }
                        }
                    }
                    _ => {}
                }
            }
            if clamps.len() > 1 {
                multi_texprop += 1;
                if clamps.windows(2).any(|w| w[0] != w[1]) {
                    clamp_conflict += 1;
                    if samples2.len() < 5 {
                        samples2.push(format!("{name} clamps={clamps:?}"));
                    }
                }
            }
            if let Some(vp) = vcp_pos {
                vcp_present += 1;
                match mat_pos {
                    Some(mp) if mp < vp => {
                        vcp_after_matprop += 1;
                        // non-default mode = anything other than (2,1)
                        if let Some((vm, lm)) = vcp_mode {
                            if !(vm == 2 && lm == 1) {
                                vcp_after_matprop_nondefault += 1;
                                if samples.len() < 8 {
                                    samples.push(format!(
                                        "{name} vm={vm} lm={lm} matpos={mp} vcppos={vp}"
                                    ));
                                }
                            }
                        }
                    }
                    _ => {
                        vcp_before += 1;
                    }
                }
            }
        }
    }
    println!("shapes={shapes}");
    println!("NiVertexColorProperty on shape chain: {vcp_present}  (after NiMaterialProperty: {vcp_after_matprop}, of which NON-default mode: {vcp_after_matprop_nondefault}; before/no-matprop: {vcp_before})");
    for s in &samples {
        println!("   {s}");
    }
    println!("NiShadeProperty blocks={shade_blocks} flags==0(flat)={shade_zero}");
    println!("NiWireframeProperty blocks={wire_blocks}");
    println!("NiStencilProperty draw_mode: {stencil_draw:?}");
    println!("shapes with >1 NiTexturingProperty on direct chain: {multi_texprop}, clamp conflicts: {clamp_conflict}");
    for s in &samples2 {
        println!("   {s}");
    }
}
