//! Dump the motion-relevant authoring of a river / rapids / waterfall NIF:
//! every shader float controller with its nif.xml controlled variable and
//! key range (the UV scroll rate), water-shader flags, extra-data tags, the
//! phantom water volume, and each shape's local vertex span (slope).
//!
//! Evidence harness for WATAL W2. Usage:
//!   cargo run --release -p byroredux-nif --example water_mesh_probe -- <bsa> <path-in-bsa> [...]

use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::collision::{BhkBoxShape, BhkSimpleShapePhantom, BhkTransformShape};
use byroredux_nif::blocks::controller::BsShaderController;
use byroredux_nif::blocks::extra_data::NiExtraData;
use byroredux_nif::blocks::interpolator::{NiFloatData, NiFloatInterpolator};
use byroredux_nif::blocks::shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSWaterShaderProperty,
};
use byroredux_nif::blocks::tri_shape::BsTriShape;
use byroredux_nif::scene::NifScene;

/// nif.xml `EffectShaderControlledVariable` (Skyrim+).
fn effect_var(v: u32) -> &'static str {
    match v {
        0 => "EmissiveMultiple",
        1 => "FalloffStartAngle",
        2 => "FalloffStopAngle",
        3 => "FalloffStartOpacity",
        4 => "FalloffStopOpacity",
        5 => "AlphaTransparency",
        6 => "U Offset",
        7 => "U Scale",
        8 => "V Offset",
        9 => "V Scale",
        _ => "Unknown",
    }
}

/// nif.xml `LightingShaderControlledFloat` (Skyrim+).
fn lighting_var(v: u32) -> &'static str {
    match v {
        0 => "RefractionStrength",
        8 => "EnvMapScale",
        9 => "Glossiness",
        10 => "SpecularStrength",
        11 => "EmissiveMultiple",
        12 => "Alpha",
        20 => "U Offset",
        21 => "U Scale",
        22 => "V Offset",
        23 => "V Scale",
        _ => "Unknown",
    }
}

fn keys_summary(scene: &NifScene, interp_idx: Option<usize>) -> String {
    let Some(idx) = interp_idx else {
        return "no interpolator".into();
    };
    let Some(interp) = scene.get_as::<NiFloatInterpolator>(idx) else {
        return format!("interp #{idx} not NiFloatInterpolator");
    };
    let Some(data) = interp
        .data_ref
        .index()
        .and_then(|d| scene.get_as::<NiFloatData>(d))
    else {
        return format!("constant {:.4}", interp.value);
    };
    let k = &data.keys.keys;
    match (k.first(), k.last()) {
        (Some(a), Some(b)) => {
            let dt = b.time - a.time;
            let rate = if dt.abs() > 1e-6 {
                (b.value - a.value) / dt
            } else {
                0.0
            };
            format!(
                "{} {:?} keys t[{:.3}..{:.3}] v[{:.4}..{:.4}] rate {:+.4}/s",
                k.len(),
                data.keys.key_type,
                a.time,
                b.time,
                a.value,
                b.value,
                rate
            )
        }
        _ => "empty key group".into(),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (bsa, paths) = args
        .split_first()
        .expect("usage: water_mesh_probe <bsa> <path>...");
    let archive = BsaArchive::open(bsa).expect("open archive");
    for path in paths {
        println!("### {path}");
        let bytes = match archive.extract(path) {
            Ok(b) => b,
            Err(e) => {
                println!("  extract failed: {e}");
                continue;
            }
        };
        let scene = match byroredux_nif::parse_nif(&bytes) {
            Ok(s) => s,
            Err(e) => {
                println!("  parse failed: {e}");
                continue;
            }
        };
        // What the production importer hands the animation system.
        if let Some(clip) = byroredux_nif::anim::import_embedded_animations(&scene) {
            for (node, ch) in &clip.float_channels {
                println!(
                    "  clip float channel {node} → {:?} ({} keys)",
                    ch.target,
                    ch.keys.len()
                );
            }
        }
        for (i, block) in scene.blocks.iter().enumerate() {
            let any = block.as_any();
            if let Some(c) = any.downcast_ref::<BsShaderController>() {
                let var = format!("{:?}", c.kind);
                let named = match c.type_name {
                    "BSEffectShaderPropertyFloatController" => var
                        .trim_start_matches("EffectFloat(")
                        .trim_end_matches(')')
                        .parse()
                        .map(effect_var)
                        .unwrap_or("?"),
                    "BSLightingShaderPropertyFloatController" => var
                        .trim_start_matches("LightingFloat(")
                        .trim_end_matches(')')
                        .parse()
                        .map(lighting_var)
                        .unwrap_or("?"),
                    _ => "-",
                };
                println!(
                    "  #{i:<3} {} {var} = {named}: {}",
                    c.type_name,
                    keys_summary(&scene, c.base.interpolator_ref.index())
                );
            } else if let Some(w) = any.downcast_ref::<BSWaterShaderProperty>() {
                let flags = w.water_shader_flags;
                let names = [
                    "DISPLACEMENT",
                    "LOD",
                    "DEPTH",
                    "ACTOR_IN_WATER",
                    "ACTOR_IN_WATER_IS_MOVING",
                    "UNDERWATER",
                    "REFLECTIONS",
                    "REFRACTIONS",
                    "VERTEX_UV",
                    "VERTEX_ALPHA_DEPTH",
                    "PROCEDURAL",
                    "FOG",
                    "UPDATE_CONSTANTS",
                    "CUBEMAP",
                ];
                let set: Vec<&str> = (0..14)
                    .filter(|b| flags & (1 << b) != 0)
                    .map(|b| names[b])
                    .collect();
                println!(
                    "  #{i:<3} BSWaterShaderProperty flags={flags:#06x} {set:?} uv_off={:?} uv_scale={:?}",
                    w.uv_offset, w.uv_scale
                );
            } else if let Some(fx) = any.downcast_ref::<BSEffectShaderProperty>() {
                println!(
                    "  #{i:<3} BSEffectShaderProperty name={:?} controller={:?} uv_off={:?} uv_scale={:?} tex={:?}",
                    fx.net.name,
                    fx.net.controller_ref.index(),
                    fx.uv_offset,
                    fx.uv_scale,
                    fx.source_texture
                );
            } else if let Some(lit) = any.downcast_ref::<BSLightingShaderProperty>() {
                println!(
                    "  #{i:<3} BSLightingShaderProperty type={} uv_off={:?} uv_scale={:?}",
                    lit.shader_type, lit.uv_offset, lit.uv_scale
                );
            } else if let Some(e) = any.downcast_ref::<NiExtraData>() {
                if e.type_name == "BSXFlags"
                    || e.type_name.contains("Boolean")
                    || e.type_name.contains("Integer")
                {
                    println!(
                        "  #{i:<3} {} name={:?} int={:?}",
                        e.type_name, e.name, e.integer_value
                    );
                }
            } else if let Some(p) = any.downcast_ref::<BhkSimpleShapePhantom>() {
                println!(
                    "  #{i:<3} bhkSimpleShapePhantom shape={:?} transform={:?}",
                    p.shape_ref.index(),
                    p.transform
                );
            } else if let Some(t) = any.downcast_ref::<BhkTransformShape>() {
                println!(
                    "  #{i:<3} bhkTransformShape shape={:?} transform={:?}",
                    t.shape_ref.index(),
                    t.transform
                );
            } else if let Some(b) = any.downcast_ref::<BhkBoxShape>() {
                println!(
                    "  #{i:<3} bhkBoxShape half-extents(havok units)={:?} radius={}",
                    b.dimensions, b.radius
                );
            } else if let Some(s) = any.downcast_ref::<BsTriShape>() {
                let mut lo = [f32::INFINITY; 3];
                let mut hi = [f32::NEG_INFINITY; 3];
                for v in &s.vertices {
                    for (a, c) in [v.x, v.y, v.z].into_iter().enumerate() {
                        lo[a] = lo[a].min(c);
                        hi[a] = hi[a].max(c);
                    }
                }
                let t = &s.av.transform;
                println!(
                    "  #{i:<3} BSTriShape name={:?} verts={} tris={} span=[{:.0},{:.0},{:.0}] z[{:.0}..{:.0}] xlate=[{:.0},{:.0},{:.0}] scale={} shader={:?}",
                    s.av.net.name,
                    s.vertices.len(),
                    s.num_triangles,
                    hi[0] - lo[0],
                    hi[1] - lo[1],
                    hi[2] - lo[2],
                    lo[2],
                    hi[2],
                    t.translation.x,
                    t.translation.y,
                    t.translation.z,
                    t.scale,
                    s.shader_property_ref.index().map(|i| scene.blocks[i].block_type_name()),
                );
            }
        }
    }
}
