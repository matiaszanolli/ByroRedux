//! Throwaway (Oblivion audit dim4): material-import census over a BSA.
use byroredux_bsa::BsaArchive;
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

    let mut meshes = 0u64;
    let mut c: BTreeMap<&'static str, u64> = Default::default();
    let mut clamp: BTreeMap<u8, u64> = Default::default();
    let mut atfunc: BTreeMap<u8, u64> = Default::default();
    let mut blend: BTreeMap<(u8, u8), u64> = Default::default();
    let mut zfunc: BTreeMap<u8, u64> = Default::default();
    let mut vcm: BTreeMap<u8, u64> = Default::default();
    let mut kind: BTreeMap<u32, u64> = Default::default();
    let mut esrc: BTreeMap<String, u64> = Default::default();
    let mut samples: BTreeMap<&'static str, Vec<String>> = Default::default();
    let mut emmult: BTreeMap<String, u64> = Default::default();

    let mut inc = |m: &mut BTreeMap<&'static str, u64>, k: &'static str| *m.entry(k).or_insert(0) += 1;

    for name in &files {
        let Ok(bytes) = archive.extract(name) else { continue };
        let Ok(scene) = parse_nif(&bytes) else { continue };
        let mut pool = byroredux_core::string::StringPool::new();
        let imported = byroredux_nif::import::import_nif_scene(&scene, &mut pool);
        for m in &imported.meshes {
            let mat = &m.material;
            meshes += 1;
            if mat.material_path.is_some() { inc(&mut c, "material_path"); 
                let e = samples.entry("material_path").or_default(); if e.len()<5 { e.push(name.clone()); } }
            if mat.is_pbr { inc(&mut c, "is_pbr"); }
            if mat.from_bgsm { inc(&mut c, "from_bgsm"); }
            if mat.metalness_override.is_some() { inc(&mut c, "metalness_override"); }
            if mat.wireframe { inc(&mut c, "wireframe");
                let e = samples.entry("wireframe").or_default(); if e.len()<5 { e.push(name.clone()); } }
            if mat.flat_shading { inc(&mut c, "flat_shading");
                let e = samples.entry("flat_shading").or_default(); if e.len()<5 { e.push(name.clone()); } }
            if mat.two_sided { inc(&mut c, "two_sided"); }
            if mat.is_decal { inc(&mut c, "is_decal"); }
            if mat.has_alpha { inc(&mut c, "alpha_blend"); }
            if mat.alpha_test { inc(&mut c, "alpha_test"); }
            if !mat.z_test { inc(&mut c, "z_test_off"); }
            if !mat.z_write { inc(&mut c, "z_write_off"); }
            if mat.textures.base_color.is_some() { inc(&mut c, "tex_base"); }
            if mat.textures.normal.is_some() { inc(&mut c, "tex_normal"); }
            if mat.textures.dark.is_some() { inc(&mut c, "tex_dark");
                let e = samples.entry("tex_dark").or_default(); if e.len()<5 { e.push(name.clone()); } }
            if mat.textures.detail.is_some() { inc(&mut c, "tex_detail"); }
            if mat.textures.smooth_spec.is_some() { inc(&mut c, "tex_gloss"); }
            if mat.textures.emissive.is_some() { inc(&mut c, "tex_glow"); }
            if mat.textures.height.is_some() { inc(&mut c, "tex_parallax");
                let e = samples.entry("tex_parallax").or_default(); if e.len()<5 { e.push(name.clone()); } }
            if mat.textures.environment.is_some() { inc(&mut c, "tex_env"); }
            if mat.textures.decals.iter().any(|d| d.is_some()) { inc(&mut c, "tex_decalslot"); }
            if mat.specular_strength == 0.0 { inc(&mut c, "spec_strength_zero"); }
            if mat.env_map_scale != 0.0 { inc(&mut c, "env_map_scale_nonzero"); }
            if mat.uv_scale != [1.0,1.0] || mat.uv_offset != [0.0,0.0] { inc(&mut c, "uv_transform"); }
            *clamp.entry(mat.texture_clamp_mode).or_insert(0) += 1;
            if mat.alpha_test { *atfunc.entry(mat.alpha_test_func).or_insert(0) += 1; }
            if mat.has_alpha { *blend.entry((mat.src_blend_mode, mat.dst_blend_mode)).or_insert(0) += 1; }
            *zfunc.entry(mat.z_function).or_insert(0) += 1;
            *vcm.entry(mat.vertex_color_mode).or_insert(0) += 1;
            *kind.entry(mat.material_kind).or_insert(0) += 1;
            *esrc.entry(format!("{:?}", mat.emissive_source)).or_insert(0) += 1;
            *emmult.entry(format!("{:.2}", mat.emissive_mult)).or_insert(0) += 1;
        }
    }
    println!("files={} meshes={}", files.len(), meshes);
    for (k, v) in &c { println!("  {k:26} {v}"); }
    println!("clamp_mode: {clamp:?}");
    println!("alpha_test_func: {atfunc:?}");
    println!("blend(src,dst): {blend:?}");
    println!("z_function: {zfunc:?}");
    println!("vertex_color_mode: {vcm:?}");
    println!("material_kind: {kind:?}");
    println!("emissive_source: {esrc:?}");
    println!("emissive_mult: {emmult:?}");
    for (k, v) in &samples { println!("sample {k}: {v:?}"); }
}
