//! Oblivion / Fallout 3 / Fallout NV era shader-property tests. Split from `shader_tests.rs` (#2056);
//! helpers live in the parent module.

use super::*;


/// Regression: #459 — `BSShaderTextureSet::parse` previously read
/// `Num Textures` as `i32` and clamped `.max(0) as u32`, silently
/// dropping any negative-interpreted length to an empty set. When
/// upstream drift flipped the high bit, the block quietly
/// succeeded at the wrong offset instead of failing loud. Verify
/// the u32 read still produces an empty set for zero, the expected
/// set for a normal count, and a loud error for a length that
/// obviously exceeds the remaining stream (the `allocate_vec`
/// budget guard from #388 catches it).
#[test]
fn parse_bsshader_texture_set_num_textures_as_u32() {
    let header = make_header(11, 34);

    // Case 1: zero textures → empty set, stream fully consumed.
    let zero = 0u32.to_le_bytes();
    let mut stream = NifStream::new(&zero, &header);
    let ts = BSShaderTextureSet::parse(&mut stream).unwrap();
    assert!(ts.textures.is_empty());
    assert_eq!(stream.position(), 4);

    // Case 2: 2 textures — normal path still works.
    let mut data = 2u32.to_le_bytes().to_vec();
    for name in ["diffuse.dds", "normal.dds"] {
        data.extend_from_slice(&(name.len() as u32).to_le_bytes());
        data.extend_from_slice(name.as_bytes());
    }
    let mut stream = NifStream::new(&data, &header);
    let ts = BSShaderTextureSet::parse(&mut stream).unwrap();
    assert_eq!(
        ts.textures,
        vec!["diffuse.dds".to_string(), "normal.dds".into()]
    );

    // Case 3: length of 0xFFFFFFFF (previously silently clamped to 0).
    // Under u32, this exceeds the remaining bytes in the stream and
    // the allocate_vec budget guard short-circuits with InvalidData —
    // loud enough for the outer block_sizes recovery to take over.
    let drift = 0xFFFF_FFFFu32.to_le_bytes();
    let mut stream = NifStream::new(&drift, &header);
    let err = BSShaderTextureSet::parse(&mut stream).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}


#[test]
fn parse_bsshader_fnv_reads_refraction_parallax() {
    // FNV (bsver=34): reads refraction (bsver>=15) + parallax (bsver>=24) = 16 bytes.
    let header = make_header(11, 34);
    let data = build_bsshader_bytes(34);
    let mut stream = NifStream::new(&data, &header);

    let prop = BSShaderPPLightingProperty::parse(&mut stream).unwrap();
    assert_eq!(prop.texture_set_ref.index(), Some(5));
    assert!((prop.refraction_strength - 0.5).abs() < 1e-6);
    assert_eq!(prop.refraction_fire_period, 10);
    assert!((prop.parallax_max_passes - 4.0).abs() < 1e-6);
    assert!((prop.parallax_scale - 1.5).abs() < 1e-6);
    // All data consumed: 38 base + 16 refraction/parallax = 54 bytes
    assert_eq!(stream.position(), 54);
}


/// Regression: #455 — `TileShaderProperty` parses the FO3
/// `BSShaderLightingProperty` base (NET + shader data + texture
/// clamp) + a trailing SizedString filename. Pre-fix the dispatch
/// aliased this type to `BSShaderPPLightingProperty::parse`, which
/// over-read 20-28 bytes of PP-specific fields and never populated
/// the filename. HUD overlays (stealth meter / airtimer / quest
/// markers) lost their texture path as a result.
#[test]
fn parse_tile_shader_property_fo3() {
    let header = make_header(11, 34); // FO3/FNV
    let mut data = Vec::new();
    // NiObjectNET: inline name (v <= 20.1.0.0 path)
    data.extend_from_slice(&0i32.to_le_bytes()); // name (string table index)
    data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                    // BSShaderPropertyData: 18 bytes
    data.extend_from_slice(&0u16.to_le_bytes()); // shade_flags
    data.extend_from_slice(&1u32.to_le_bytes()); // shader_type
    data.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_1
    data.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_2
    data.extend_from_slice(&0.0f32.to_le_bytes()); // env_map_scale
    data.extend_from_slice(&3u32.to_le_bytes()); // texture_clamp_mode
                                                 // file_name SizedString (u32 length + bytes; NO trailing null)
    let name = b"textures\\interface\\airtimer.dds";
    data.extend_from_slice(&(name.len() as u32).to_le_bytes());
    data.extend_from_slice(name);
    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = TileShaderProperty::parse(&mut stream)
        .expect("TileShaderProperty should parse with BSShaderLightingProperty base + filename");
    assert_eq!(
        stream.position() as usize,
        expected_len,
        "TileShaderProperty must consume exactly {expected_len} bytes",
    );
    assert_eq!(prop.texture_clamp_mode, 3);
    assert_eq!(prop.file_name, "textures\\interface\\airtimer.dds");
    assert_eq!(prop.shader.shader_type, 1);
}


/// Regression for #774 / FO3-1-PARGATE — nif.xml:6247-6248 specifies
/// `vercond="#BSVER# #GT# 24"` (strictly greater) for the parallax
/// fields. FO3 ships content at bsver=24 which must NOT carry the
/// 8-byte parallax trailer; the prior `>= 24` gate over-read 8 phantom
/// bytes (masked at the recoverable-rate metric by `block_sizes`
/// re-alignment in the outer dispatch loop).
#[test]
fn parse_bsshader_fo3_bsver24_skips_parallax() {
    let header = make_header(11, 24);
    let data = build_bsshader_bytes(24);
    let mut stream = NifStream::new(&data, &header);

    let prop = BSShaderPPLightingProperty::parse(&mut stream).unwrap();
    assert_eq!(prop.texture_set_ref.index(), Some(5));
    // Refraction reads at bsver > crate::version::bsver::FO3_REFRACTION, so bsver=24 must populate them.
    assert!((prop.refraction_strength - 0.5).abs() < 1e-6);
    assert_eq!(prop.refraction_fire_period, 10);
    // Parallax gate is bsver > crate::version::bsver::FO3_PARALLAX, so bsver=24 must default.
    assert!((prop.parallax_max_passes - 4.0).abs() < 1e-6);
    assert!((prop.parallax_scale - 1.0).abs() < 1e-6);
    // 38 base + 8 refraction = 46 bytes; no parallax trailer.
    assert_eq!(stream.position(), 46);
}


#[test]
fn parse_bsshader_oblivion_no_extra_fields() {
    // Oblivion (bsver=0): no refraction or parallax fields.
    let header = make_header(0, 0);
    let data = build_bsshader_bytes(0);
    let mut stream = NifStream::new(&data, &header);

    let prop = BSShaderPPLightingProperty::parse(&mut stream).unwrap();
    assert_eq!(prop.texture_set_ref.index(), Some(5));
    assert_eq!(prop.refraction_strength, 0.0);
    assert_eq!(prop.refraction_fire_period, 0);
    assert!((prop.parallax_max_passes - 4.0).abs() < 1e-6); // defaults
    assert!((prop.parallax_scale - 1.0).abs() < 1e-6);
    // Only 38 bytes consumed (no extras)
    assert_eq!(stream.position(), 38);
}


// ── #749 / SF-D3-01: BGSM/BGEM/MAT stopcond suffix gate ───────────────

/// Direct unit tests for `is_material_reference` — the suffix-aware
/// gate shared between the FO76+/Starfield shader-property stopconds
/// and `material_path_from_name`. Pre-#749 the stopcond fired on any
/// non-empty Name, so editor labels with no path suffix collapsed
/// into stub material references with all PBR scalars zeroed.
#[test]
fn is_material_reference_recognises_known_suffixes() {
    // `super::*` already pulls in the helper; this test fixes its
    // semantics so future audits land against the same gate.

    // True cases: documented suffixes, mixed case, stale terminators.
    assert!(is_material_reference("materials/weapons/rifle.bgsm"));
    assert!(is_material_reference("materials/weapons/rifle.BGSM"));
    assert!(is_material_reference("materials/effects/glow.bgem"));
    assert!(is_material_reference("materials/effects/glow.BGEM"));
    assert!(is_material_reference("materials/sf/armor.mat"));
    assert!(is_material_reference("materials/sf/armor.MAT"));
    assert!(is_material_reference("materials/weapons/rifle.bgsm\0\0"));
    assert!(is_material_reference("materials/weapons/rifle.bgsm  "));
    assert!(is_material_reference("materials/weapons/rifle.bgsm \0\0 "));

    // False cases: editor labels, plain words, empty.
    assert!(!is_material_reference(""));
    assert!(!is_material_reference("Material_Slot_01"));
    assert!(!is_material_reference("FaceTint_FOR_BLINK"));
    assert!(!is_material_reference("rifle"));
    assert!(!is_material_reference("rifle.dds"));
    assert!(!is_material_reference("rifle.bgsmext"));
    assert!(!is_material_reference("   "));
}


/// FO3/FNV (bsver=34) must NOT read the emissive color field — it is absent
/// on pre-Skyrim PPLighting blocks.  Verifies the bsver > crate::version::bsver::FO3_FNV gate is strict.
#[test]
fn bsshader_pplighting_fnv_has_no_emissive_color() {
    let header = make_header(11, 34); // FNV
    let data = build_bsshader_bytes(34); // no emissive bytes appended
    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);

    let prop = BSShaderPPLightingProperty::parse(&mut stream)
        .expect("FNV PPLighting should parse without emissive field");

    assert_eq!(
        stream.position() as usize,
        expected_len,
        "FNV PPLighting (bsver=34) must not over-read into emissive bytes"
    );
    // Default emissive when absent: [0,0,0,1]
    assert_eq!(prop.emissive_color, [0.0, 0.0, 0.0, 1.0]);
}


/// #1331 sibling — nif.xml (line 6236) gates the four falloff fields on
/// `#BSVER# #GT# 26`. A transitional v20.2.0.7/bsver=11 export detects as
/// the `Fallout3` variant, so the old `variant().avobject_flags_u32()` gate
/// read 16 phantom falloff bytes past end-of-block (EOF / misalign). With
/// the per-file BSVER ≤ 26 gate the absent-field default branch is taken.
/// Red before the fix (over-read → parse error), green after.
#[test]
fn no_lighting_falloff_absent_when_bsver_le_26() {
    let header = make_header(11, 11);
    let data = build_no_lighting_bytes("ui\\elem.dds", None);
    let mut stream = NifStream::new(&data, &header);
    let prop = BSShaderNoLightingProperty::parse(&mut stream)
        .expect("v20.2.0.7 / bsver=11 BSShaderNoLightingProperty should parse");
    assert_eq!(prop.file_name, "ui\\elem.dds");
    // Absent-field defaults.
    assert_eq!(prop.falloff_start_angle, 0.0);
    assert_eq!(prop.falloff_start_opacity, 1.0);
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "bsver ≤ 26 must NOT read the four falloff floats"
    );
}


/// #1331 sibling — retail FO3/FNV (bsver=34 > 26) reads all four falloff
/// floats. Pins the upper branch so the fix doesn't regress sized games.
#[test]
fn no_lighting_falloff_present_when_bsver_gt_26() {
    let header = make_header(11, 34);
    let data = build_no_lighting_bytes("ui\\elem.dds", Some([0.1, 0.2, 0.3, 0.4]));
    let mut stream = NifStream::new(&data, &header);
    let prop = BSShaderNoLightingProperty::parse(&mut stream)
        .expect("v20.2.0.7 / bsver=34 BSShaderNoLightingProperty should parse");
    assert!((prop.falloff_start_angle - 0.1).abs() < 1e-6);
    assert!((prop.falloff_stop_angle - 0.2).abs() < 1e-6);
    assert!((prop.falloff_start_opacity - 0.3).abs() < 1e-6);
    assert!((prop.falloff_stop_opacity - 0.4).abs() < 1e-6);
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "bsver > 26 must read all four falloff floats"
    );
}
