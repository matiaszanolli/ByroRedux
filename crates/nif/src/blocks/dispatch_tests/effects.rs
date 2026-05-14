//! Effect / light / camera dispatch tests.
//!
//! NiPointLight + variants, NiCamera, NiTextureEffect, legacy
//! particle-modifier chain.

use super::{fo4_header, oblivion_header};
use crate::blocks::*;
use crate::stream::NifStream;

/// Build an "empty NiAVObject" body sized for Oblivion. Same prefix
/// as the NiNode helper, minus the NiNode-specific children+effects
/// trailers. Used for NiLight bodies.
fn oblivion_niavobject_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&0u32.to_le_bytes()); // name len (empty inline)
    d.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
    d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
    d.extend_from_slice(&0u16.to_le_bytes()); // flags
    for _ in 0..3 {
        d.extend_from_slice(&0.0f32.to_le_bytes()); // translation
    }
    for row in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        for v in row {
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
    d.extend_from_slice(&0u32.to_le_bytes()); // empty properties list
    d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
    d
}

/// Build an "empty NiAVObject" body sized for FO4+ (NIF 20.2.0.7,
/// bsver 130). Same field order as Oblivion but: `name` is a string-
/// table index (i32, -1 = absent), `flags` is u32, properties list is
/// gone (bsver > 34), and the collision_ref is still present (NIF v
/// >= 10.0.1.0).
fn fo4_niavobject_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&(-1i32).to_le_bytes()); // name idx (none)
    d.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
    d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
    d.extend_from_slice(&0u32.to_le_bytes()); // flags (u32 since bsver > 26)
    for _ in 0..3 {
        d.extend_from_slice(&0.0f32.to_le_bytes()); // translation
    }
    for row in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        for v in row {
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
                                                // No properties list (bsver=130 > 34, dropped per nif.xml).
    d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
    d
}


/// Regression test for issue #156: NiLight hierarchy dispatch + payload.
#[test]
fn oblivion_lights_parse_with_attenuation_and_color() {
    use crate::blocks::light::{NiAmbientLight, NiPointLight, NiSpotLight};

    let header = oblivion_header();
    let av = oblivion_niavobject_bytes();

    // Common NiDynamicEffect + NiLight tail for an Oblivion torch:
    //   switch_state:u8=1, num_affected_nodes:u32=0,
    //   dimmer:f32=1.0,
    //   ambient:(0,0,0), diffuse:(1.0, 0.6, 0.2), specular:(0,0,0)
    fn dynamic_light_tail() -> Vec<u8> {
        let mut d = Vec::new();
        d.push(1u8); // switch_state
        d.extend_from_slice(&0u32.to_le_bytes()); // affected nodes count
        d.extend_from_slice(&1.0f32.to_le_bytes()); // dimmer
        for _ in 0..3 {
            d.extend_from_slice(&0.0f32.to_le_bytes()); // ambient color
        }
        for &c in &[1.0f32, 0.6, 0.2] {
            d.extend_from_slice(&c.to_le_bytes()); // diffuse color
        }
        for _ in 0..3 {
            d.extend_from_slice(&0.0f32.to_le_bytes()); // specular color
        }
        d
    }

    // NiAmbientLight: base + dynamic_light_tail, nothing else.
    let mut amb = av.clone();
    amb.extend_from_slice(&dynamic_light_tail());
    let mut stream = NifStream::new(&amb, &header);
    let block = parse_block("NiAmbientLight", &mut stream, Some(amb.len() as u32))
        .expect("NiAmbientLight dispatch");
    let light = block.as_any().downcast_ref::<NiAmbientLight>().unwrap();
    assert_eq!(light.base.dimmer, 1.0);
    assert!((light.base.diffuse_color.g - 0.6).abs() < 1e-6);
    assert_eq!(stream.position(), amb.len() as u64);

    // NiPointLight: base + tail + (const=1.0, lin=0.01, quad=0.0).
    let mut pl = av.clone();
    pl.extend_from_slice(&dynamic_light_tail());
    pl.extend_from_slice(&1.0f32.to_le_bytes()); // constant
    pl.extend_from_slice(&0.01f32.to_le_bytes()); // linear
    pl.extend_from_slice(&0.0f32.to_le_bytes()); // quadratic
    let mut stream = NifStream::new(&pl, &header);
    let block = parse_block("NiPointLight", &mut stream, Some(pl.len() as u32))
        .expect("NiPointLight dispatch");
    let p = block.as_any().downcast_ref::<NiPointLight>().unwrap();
    assert_eq!(p.constant_attenuation, 1.0);
    assert!((p.linear_attenuation - 0.01).abs() < 1e-6);
    assert_eq!(stream.position(), pl.len() as u64);

    // NiSpotLight: NiPointLight body + outer + exponent (Oblivion
    // v20.0.0.5 < 20.2.0.5, so no inner_spot_angle).
    let mut sl = av.clone();
    sl.extend_from_slice(&dynamic_light_tail());
    sl.extend_from_slice(&1.0f32.to_le_bytes()); // constant
    sl.extend_from_slice(&0.01f32.to_le_bytes()); // linear
    sl.extend_from_slice(&0.0f32.to_le_bytes()); // quadratic
    sl.extend_from_slice(&(std::f32::consts::FRAC_PI_4).to_le_bytes()); // outer
    sl.extend_from_slice(&2.0f32.to_le_bytes()); // exponent
    let mut stream = NifStream::new(&sl, &header);
    let block = parse_block("NiSpotLight", &mut stream, Some(sl.len() as u32))
        .expect("NiSpotLight dispatch");
    let s = block.as_any().downcast_ref::<NiSpotLight>().unwrap();
    assert!((s.outer_spot_angle - std::f32::consts::FRAC_PI_4).abs() < 1e-6);
    assert_eq!(s.inner_spot_angle, 0.0); // not in this version
    assert_eq!(s.exponent, 2.0);
    assert_eq!(stream.position(), sl.len() as u64);
}

/// Regression for #721 (NIF-D5-06): FO4+ NiLight reparents directly
/// onto NiAVObject — `Switch State` and the `Affected Nodes` list both
/// carry `vercond="#NI_BS_LT_FO4#"` in nif.xml line 3499-3504. Pre-fix
/// the parser keyed only on NIF version (always true for FO4) and
/// consumed 5 bytes of NiLight color data as the dropped fields,
/// throwing every mesh-embedded light through `block_size` recovery.
/// 681 light blocks across FO4 / FO76 / SF Meshes archives demoted.
///
/// This fixture has NO `switch_state` byte and NO `affected_nodes`
/// count/list — directly into NiLight `dimmer + 3 colors` after the
/// NiAVObject base. A pre-#721 parser would over-read by 5 bytes, the
/// `dimmer` would land on garbage, and the per-block size assertion
/// at the end would fail.
#[test]
fn fo4_point_light_skips_dynamic_effect_tail() {
    use crate::blocks::light::NiPointLight;

    let header = fo4_header();
    let mut bytes = fo4_niavobject_bytes();
    // No NiDynamicEffect tail on FO4+.
    bytes.extend_from_slice(&0.85f32.to_le_bytes()); // dimmer
    for _ in 0..3 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // ambient
    }
    for &c in &[1.0f32, 0.4, 0.1] {
        bytes.extend_from_slice(&c.to_le_bytes()); // diffuse
    }
    for _ in 0..3 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // specular
    }
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // constant_attenuation
    bytes.extend_from_slice(&0.02f32.to_le_bytes()); // linear
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // quadratic

    let expected_len = bytes.len();
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiPointLight", &mut stream, Some(bytes.len() as u32))
        .expect("FO4 NiPointLight must dispatch and consume the stream cleanly");
    let p = block.as_any().downcast_ref::<NiPointLight>().unwrap();
    // dimmer reads correctly only when the parser SKIPPED the FO4-
    // dropped NiDynamicEffect tail. Pre-fix this lands on the
    // ambient_r byte and reads as garbage.
    assert!(
        (p.base.dimmer - 0.85).abs() < 1e-6,
        "dimmer must read 0.85 (got {}) — parser likely over-read the FO4 NiDynamicEffect tail",
        p.base.dimmer
    );
    assert!((p.base.diffuse_color.r - 1.0).abs() < 1e-6);
    assert!((p.base.diffuse_color.g - 0.4).abs() < 1e-6);
    assert_eq!(p.constant_attenuation, 1.0);
    assert!((p.linear_attenuation - 0.02).abs() < 1e-6);
    assert_eq!(stream.position(), expected_len as u64);
}

/// Regression test for issue #153: NiCamera parsing.
#[test]
fn oblivion_ni_camera_roundtrip() {
    use crate::blocks::node::NiCamera;

    let header = oblivion_header();
    let mut bytes = oblivion_niavobject_bytes();
    // camera_flags u16
    bytes.extend_from_slice(&0u16.to_le_bytes());
    // frustum left/right/top/bottom
    bytes.extend_from_slice(&(-0.5f32).to_le_bytes());
    bytes.extend_from_slice(&0.5f32.to_le_bytes());
    bytes.extend_from_slice(&0.3f32.to_le_bytes());
    bytes.extend_from_slice(&(-0.3f32).to_le_bytes());
    // frustum near / far
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&5000.0f32.to_le_bytes());
    // use_orthographic byte bool = 0
    bytes.push(0u8);
    // viewport left/right/top/bottom
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    // lod_adjust
    bytes.extend_from_slice(&1.5f32.to_le_bytes());
    // scene_ref
    bytes.extend_from_slice(&9i32.to_le_bytes());
    // num_screen_polygons, num_screen_textures (both u32, both 0 on disk)
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());

    let mut stream = NifStream::new(&bytes, &header);
    let block =
        parse_block("NiCamera", &mut stream, Some(bytes.len() as u32)).expect("NiCamera dispatch");
    let c = block.as_any().downcast_ref::<NiCamera>().unwrap();
    assert!((c.frustum_right - 0.5).abs() < 1e-6);
    assert!((c.frustum_far - 5000.0).abs() < 1e-6);
    assert!(!c.use_orthographic);
    assert!((c.lod_adjust - 1.5).abs() < 1e-6);
    assert_eq!(c.scene_ref.index(), Some(9));
    assert_eq!(c.num_screen_polygons, 0);
    assert_eq!(c.num_screen_textures, 0);
    assert_eq!(stream.position(), bytes.len() as u64);
}

/// Regression test for issue #163: NiTextureEffect.
#[test]
fn oblivion_ni_texture_effect_roundtrip() {
    use crate::blocks::texture::NiTextureEffect;

    let header = oblivion_header();
    let mut bytes = oblivion_niavobject_bytes();
    // NiDynamicEffect base: switch_state=1, num_affected_nodes=0
    bytes.push(1u8);
    bytes.extend_from_slice(&0u32.to_le_bytes());
    // model_projection_matrix: 3x3 identity
    for row in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        for v in row {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    // model_projection_translation: (0, 0, 0)
    for _ in 0..3 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
    }
    // texture_filtering = 2 (trilerp)
    bytes.extend_from_slice(&2u32.to_le_bytes());
    // NO max_anisotropy at 20.0.0.5 (< 20.5.0.4)
    // texture_clamping = 0
    bytes.extend_from_slice(&0u32.to_le_bytes());
    // texture_type = 4 (env map)
    bytes.extend_from_slice(&4u32.to_le_bytes());
    // coordinate_generation_type = 0 (sphere map)
    bytes.extend_from_slice(&0u32.to_le_bytes());
    // source_texture_ref = 17
    bytes.extend_from_slice(&17i32.to_le_bytes());
    // enable_plane = 0
    bytes.push(0u8);
    // plane: normal (0, 1, 0), constant 0.5
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.5f32.to_le_bytes());
    // NO ps2_l / ps2_k at 20.0.0.5 (> 10.2.0.0)

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiTextureEffect", &mut stream, Some(bytes.len() as u32))
        .expect("NiTextureEffect dispatch");
    let e = block.as_any().downcast_ref::<NiTextureEffect>().unwrap();
    assert_eq!(e.texture_filtering, 2);
    assert_eq!(e.texture_type, 4);
    assert_eq!(e.coordinate_generation_type, 0);
    assert_eq!(e.source_texture_ref.index(), Some(17));
    assert!(!e.enable_plane);
    assert!((e.plane[1] - 1.0).abs() < 1e-6);
    assert!((e.plane[3] - 0.5).abs() < 1e-6);
    assert_eq!(e.max_anisotropy, 0); // absent for Oblivion
    assert_eq!(e.ps2_l, 0); // absent for Oblivion
    assert_eq!(stream.position(), bytes.len() as u64);
}

/// Regression test for issue #143: legacy particle modifier chain
/// and NiParticleSystemController. These types ship in every
/// Oblivion magic FX / fire / dust / blood mesh and hard-fail the
/// whole file when one is missing (no block_sizes fallback).
#[test]
fn oblivion_legacy_particle_modifier_chain_roundtrip() {
    use crate::blocks::legacy_particle::{
        NiGravity, NiParticleBomb, NiParticleColorModifier, NiParticleGrowFade, NiParticleRotation,
        NiPlanarCollider, NiSphericalCollider,
    };

    let header = oblivion_header();

    // Helpers.
    fn niptr_modifier_prefix() -> Vec<u8> {
        // next_modifier = -1, controller = -1
        let mut d = Vec::new();
        d.extend_from_slice(&(-1i32).to_le_bytes());
        d.extend_from_slice(&(-1i32).to_le_bytes());
        d
    }
    fn collider_prefix() -> Vec<u8> {
        let mut d = niptr_modifier_prefix();
        d.extend_from_slice(&0.5f32.to_le_bytes()); // bounce
        d.push(0u8); // spawn_on_collide
        d.push(1u8); // die_on_collide
        d
    }

    // NiParticleColorModifier: base + color_data_ref.
    let mut bytes = niptr_modifier_prefix();
    bytes.extend_from_slice(&7i32.to_le_bytes());
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiParticleColorModifier", &mut s, Some(bytes.len() as u32)).unwrap();
    let m = b
        .as_any()
        .downcast_ref::<NiParticleColorModifier>()
        .unwrap();
    assert_eq!(m.color_data_ref.index(), Some(7));
    assert_eq!(s.position(), bytes.len() as u64);

    // NiParticleGrowFade: base + grow + fade.
    let mut bytes = niptr_modifier_prefix();
    bytes.extend_from_slice(&0.25f32.to_le_bytes());
    bytes.extend_from_slice(&0.75f32.to_le_bytes());
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiParticleGrowFade", &mut s, Some(bytes.len() as u32)).unwrap();
    let m = b.as_any().downcast_ref::<NiParticleGrowFade>().unwrap();
    assert!((m.grow - 0.25).abs() < 1e-6);
    assert!((m.fade - 0.75).abs() < 1e-6);
    assert_eq!(s.position(), bytes.len() as u64);

    // NiParticleRotation: base + random_initial_axis + Vec3 axis + speed.
    let mut bytes = niptr_modifier_prefix();
    bytes.push(1u8);
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&2.5f32.to_le_bytes());
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiParticleRotation", &mut s, Some(bytes.len() as u32)).unwrap();
    let m = b.as_any().downcast_ref::<NiParticleRotation>().unwrap();
    assert!(m.random_initial_axis);
    assert_eq!(m.initial_axis, [0.0, 1.0, 0.0]);
    assert!((m.rotation_speed - 2.5).abs() < 1e-6);
    assert_eq!(s.position(), bytes.len() as u64);

    // NiParticleBomb: base + decay + duration + delta_v + start +
    // decay_type + symmetry_type + position + direction.
    let mut bytes = niptr_modifier_prefix();
    for v in [0.1f32, 1.0, 2.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&1u32.to_le_bytes()); // decay_type
    bytes.extend_from_slice(&0u32.to_le_bytes()); // symmetry_type
    for v in [0.0f32, 0.0, 0.0, 0.0, 0.0, 1.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiParticleBomb", &mut s, Some(bytes.len() as u32)).unwrap();
    let m = b.as_any().downcast_ref::<NiParticleBomb>().unwrap();
    assert_eq!(m.decay_type, 1);
    assert_eq!(m.direction, [0.0, 0.0, 1.0]);
    assert_eq!(s.position(), bytes.len() as u64);

    // NiGravity: base + decay + force + field_type + position + direction.
    let mut bytes = niptr_modifier_prefix();
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // decay
    bytes.extend_from_slice(&9.81f32.to_le_bytes()); // force
    bytes.extend_from_slice(&1u32.to_le_bytes()); // planar field
    for v in [0.0f32, 0.0, 0.0, 0.0, -1.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiGravity", &mut s, Some(bytes.len() as u32)).unwrap();
    let m = b.as_any().downcast_ref::<NiGravity>().unwrap();
    assert!((m.force - 9.81).abs() < 1e-6);
    assert_eq!(m.field_type, 1);
    assert_eq!(m.direction[1], -1.0);
    assert_eq!(s.position(), bytes.len() as u64);

    // NiPlanarCollider: collider_prefix + height + width + position +
    // x_vector + y_vector + NiPlane (vec3 normal + f32 constant).
    let mut bytes = collider_prefix();
    bytes.extend_from_slice(&10.0f32.to_le_bytes()); // height
    bytes.extend_from_slice(&5.0f32.to_le_bytes()); // width
    for v in [0.0f32; 3] {
        bytes.extend_from_slice(&v.to_le_bytes());
    } // position
    for v in [1.0f32, 0.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    } // x_vector
    for v in [0.0f32, 0.0, 1.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    } // y_vector
    for v in [0.0f32, 1.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    } // plane normal
    bytes.extend_from_slice(&0.25f32.to_le_bytes()); // plane constant
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiPlanarCollider", &mut s, Some(bytes.len() as u32)).unwrap();
    let m = b.as_any().downcast_ref::<NiPlanarCollider>().unwrap();
    assert!(m.die_on_collide);
    assert!((m.height - 10.0).abs() < 1e-6);
    assert_eq!(m.plane, [0.0, 1.0, 0.0, 0.25]);
    assert_eq!(s.position(), bytes.len() as u64);

    // NiSphericalCollider: collider_prefix + radius + position.
    let mut bytes = collider_prefix();
    bytes.extend_from_slice(&3.5f32.to_le_bytes()); // radius
    for v in [1.0f32, 2.0, 3.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiSphericalCollider", &mut s, Some(bytes.len() as u32)).unwrap();
    let m = b.as_any().downcast_ref::<NiSphericalCollider>().unwrap();
    assert!((m.radius - 3.5).abs() < 1e-6);
    assert_eq!(m.position, [1.0, 2.0, 3.0]);
    assert_eq!(s.position(), bytes.len() as u64);
}
