//! Unit tests for the property block parsers (NiAlphaProperty,
//! NiTexturingProperty, BSShader*, etc.). Extracted from
//! `properties.rs` to keep the production code coherent.

use super::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;
use std::sync::Arc;

fn make_header(user_version: u32, user_version_2: u32) -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version,
        user_version_2,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("Material")],
        max_string_length: 8,
        num_groups: 0,
    }
}

fn write_color(buf: &mut Vec<u8>, r: f32, g: f32, b: f32) {
    buf.extend_from_slice(&r.to_le_bytes());
    buf.extend_from_slice(&g.to_le_bytes());
    buf.extend_from_slice(&b.to_le_bytes());
}

fn assert_uniform_color(color: &NiColor, expected: f32) {
    assert!((color.r - expected).abs() < 1e-6);
    assert!((color.g - expected).abs() < 1e-6);
    assert!((color.b - expected).abs() < 1e-6);
}

fn build_material_oblivion() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // No NiProperty flags — until 10.0.1.2, tests use v20.2.0.7
    write_color(&mut data, 0.2, 0.2, 0.2);
    write_color(&mut data, 0.8, 0.6, 0.4);
    write_color(&mut data, 1.0, 1.0, 1.0);
    write_color(&mut data, 0.0, 0.0, 0.0);
    data.extend_from_slice(&25.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data
}

fn build_material_fnv() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // No NiProperty flags — until 10.0.1.2, FNV is v20.2.0.7
    write_color(&mut data, 0.5, 0.5, 0.5);
    write_color(&mut data, 0.1, 0.0, 0.0);
    data.extend_from_slice(&10.0f32.to_le_bytes());
    data.extend_from_slice(&0.8f32.to_le_bytes());
    data.extend_from_slice(&2.5f32.to_le_bytes());
    data
}

#[test]
fn parse_material_oblivion_reads_ambient_diffuse() {
    let header = make_header(0, 0);
    let data = build_material_oblivion();
    let mut stream = NifStream::new(&data, &header);
    let mat = NiMaterialProperty::parse(&mut stream).unwrap();
    assert!((mat.ambient.r - 0.2).abs() < 1e-6);
    assert!((mat.diffuse.r - 0.8).abs() < 1e-6);
    assert!((mat.diffuse.g - 0.6).abs() < 1e-6);
    assert!((mat.shininess - 25.0).abs() < 1e-6);
    assert!((mat.emissive_mult - 1.0).abs() < 1e-6);
}

#[test]
fn parse_material_fnv_skips_ambient_diffuse() {
    let header = make_header(11, 34);
    let data = build_material_fnv();
    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let mat = NiMaterialProperty::parse(&mut stream).unwrap();
    assert_uniform_color(&mat.ambient, 1.0);
    assert_uniform_color(&mat.diffuse, 1.0);
    assert!((mat.specular.r - 0.5).abs() < 1e-6);
    assert!((mat.emissive.r - 0.1).abs() < 1e-6);
    assert!((mat.shininess - 10.0).abs() < 1e-6);
    assert!((mat.alpha - 0.8).abs() < 1e-6);
    assert!((mat.emissive_mult - 2.5).abs() < 1e-6);
    assert_eq!(stream.position() as usize, expected_len);
}

#[test]
fn parse_material_fo3_also_skips_ambient_diffuse() {
    let header = make_header(11, 34);
    let data = build_material_fnv();
    let mut stream = NifStream::new(&data, &header);
    let mat = NiMaterialProperty::parse(&mut stream).unwrap();
    assert_uniform_color(&mat.ambient, 1.0);
    assert_uniform_color(&mat.diffuse, 1.0);
    assert!((mat.emissive_mult - 2.5).abs() < 1e-6);
}

fn build_flag_property_bytes() -> Vec<u8> {
    let mut data = Vec::new();
    // NiObjectNET: name (string table index 0)
    data.extend_from_slice(&0i32.to_le_bytes());
    // extra_data_refs: count=0
    data.extend_from_slice(&0u32.to_le_bytes());
    // controller_ref: -1
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // flags: u16 (bit 0 = enabled)
    data.extend_from_slice(&1u16.to_le_bytes());
    data
}

#[test]
fn parse_flag_property_specular() {
    let header = make_header(11, 34);
    let data = build_flag_property_bytes();
    let mut stream = NifStream::new(&data, &header);
    let prop = NiFlagProperty::parse(&mut stream, "NiSpecularProperty").unwrap();
    assert_eq!(prop.block_type_name(), "NiSpecularProperty");
    assert!(prop.enabled());
    assert_eq!(prop.flags, 1);
    assert_eq!(stream.position() as usize, data.len());
}

#[test]
fn parse_flag_property_wireframe_disabled() {
    let header = make_header(11, 34);
    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes()); // bit 0 = 0 → disabled
    let mut stream = NifStream::new(&data, &header);
    let prop = NiFlagProperty::parse(&mut stream, "NiWireframeProperty").unwrap();
    assert!(!prop.enabled());
}

/// Regression for #2003 / NIF-D1-04 — `NiShadeProperty.Flags` is gated
/// `vercond="#NI_BS_LTE_FO3#"`; FO3/FNV (bsver<=34) still carries it on
/// disk, same as the other three `NiFlagProperty` aliases.
#[test]
fn parse_flag_property_shade_fo3_reads_flags() {
    let header = make_header(11, 34);
    let data = build_flag_property_bytes();
    let mut stream = NifStream::new(&data, &header);
    let prop = NiFlagProperty::parse(&mut stream, "NiShadeProperty").unwrap();
    assert_eq!(prop.block_type_name(), "NiShadeProperty");
    assert_eq!(prop.flags, 1);
    assert_eq!(stream.position() as usize, data.len());
}

/// Skyrim+ (bsver>34) counterpart — `Flags` is absent on disk; the
/// parser must not consume it and must default to SHADING_SMOOTH (1).
/// Pre-fix this read a phantom u16, shifting every field of the next
/// block by 2 bytes (recovered only via `block_sizes` on Skyrim+, but
/// with wrong `flags`).
#[test]
fn parse_flag_property_shade_skyrim_skips_flags() {
    let header = make_header(12, 83);
    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes()); // name
    data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                    // No trailing Flags — bsver=83 > FO3_FNV.
    let mut stream = NifStream::new(&data, &header);
    let prop = NiFlagProperty::parse(&mut stream, "NiShadeProperty").unwrap();
    assert_eq!(prop.flags, 1, "must default to SHADING_SMOOTH when absent");
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "must not read a phantom Flags u16 on Skyrim+"
    );

    // Sibling NiFlagProperty aliases still read Flags unconditionally —
    // append it and confirm they still consume it on the same bsver.
    data.extend_from_slice(&0u16.to_le_bytes());
    let mut stream = NifStream::new(&data, &header);
    let prop = NiFlagProperty::parse(&mut stream, "NiWireframeProperty").unwrap();
    assert_eq!(prop.flags, 0);
    assert_eq!(stream.position() as usize, data.len());
}

#[test]
fn parse_string_palette() {
    let header = make_header(11, 34);
    let mut data = Vec::new();
    let palette_str = "Bip01\0Bip01 Head\0Bip01 L Hand\0";
    data.extend_from_slice(&(palette_str.len() as u32).to_le_bytes());
    data.extend_from_slice(palette_str.as_bytes());
    data.extend_from_slice(&(palette_str.len() as u32).to_le_bytes()); // redundant length
    let mut stream = NifStream::new(&data, &header);
    let pal = NiStringPalette::parse(&mut stream).unwrap();
    assert_eq!(pal.get_string(0), Some("Bip01"));
    assert_eq!(pal.get_string(6), Some("Bip01 Head"));
    assert_eq!(pal.get_string(17), Some("Bip01 L Hand"));
    assert_eq!(pal.get_string(999), None);
    assert_eq!(stream.position() as usize, data.len());
}

/// #3516 — `TexDesc` carries two disjoint on-disk encodings in one `flags`
/// word, so `clamp_mode` is decoded at parse time and both branches must
/// agree on the *meaning*. Values are the measured real ones: a census over
/// `Fallout - Meshes.bsa` found 2236 of 2258 base descriptors authoring the
/// raw word `0x3200` (clamp 3 = WRAP/WRAP, filter 2), and Oblivion's
/// synthesized equivalent for the same clamp mode is `0x0023`.
#[test]
fn tex_desc_clamp_mode_decodes_from_the_right_nibble_per_version() {
    // Build a one-slot NiTexturingProperty and read back its base TexDesc.
    let parse_base = |version: NifVersion, body: &[u8]| {
        let header = NifHeader {
            version,
            little_endian: true,
            user_version: 11,
            user_version_2: 11,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };
        let mut data = Vec::new();
        // NiObjectNET base: inline empty name + 0 extras + null controller.
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        if version <= NifVersion::V10_0_1_2 || version >= NifVersion::V20_1_0_2 {
            data.extend_from_slice(&0u16.to_le_bytes()); // property flags
        }
        if version <= NifVersion::STRING_TABLE_THRESHOLD {
            data.extend_from_slice(&0u32.to_le_bytes()); // apply_mode
        }
        data.extend_from_slice(&1u32.to_le_bytes()); // texture_count = 1 (base only)
        data.push(1); // base has = 1
        data.extend_from_slice(&7i32.to_le_bytes()); // source_ref
        data.extend_from_slice(body);
        data.extend_from_slice(&0u32.to_le_bytes()); // num_shader_textures
        let mut stream = NifStream::new(&data, &header);
        NiTexturingProperty::parse(&mut stream)
            .expect("NiTexturingProperty should parse")
            .base_texture
            .expect("base slot is populated")
    };

    // FNV / FO3 (v20.2.0.7, >= 20.1.0.3): the raw `TexturingMapFlags` word.
    // Clamp is bits 12-15, so `flags & 0xF` — what the consumer used to read
    // — is 0 here and would have selected CLAMP_S_CLAMP_T for every one of
    // those 2236 WRAP/WRAP descriptors.
    let mut fnv_body = Vec::new();
    fnv_body.extend_from_slice(&0x3200u16.to_le_bytes());
    fnv_body.push(0); // has_transform = 0
    let fnv = parse_base(NifVersion::V20_2_0_7, &fnv_body);
    assert_eq!(fnv.flags, 0x3200, "the raw word is stored verbatim");
    assert_eq!(
        fnv.flags & 0xF,
        0,
        "the old low-nibble read really did yield 0"
    );
    assert_eq!(
        fnv.clamp_mode, 3,
        "0x3200 is clamp mode 3 (WRAP_S_WRAP_T), filter 2 — nif.xml's \
         `0xYZ00 = clamp mode Y, filter mode Z`"
    );

    // Oblivion (v20.0.0.5, < 20.1.0.3): three separate uints, packed into the
    // synthesized layout with clamp in the LOW nibble. Same clamp mode, and
    // the low-nibble read is correct here — which is why the bug was
    // invisible on the game the code was written against.
    let mut obl_body = Vec::new();
    obl_body.extend_from_slice(&3u32.to_le_bytes()); // clamp_mode = WRAP_S_WRAP_T
    obl_body.extend_from_slice(&2u32.to_le_bytes()); // filter_mode
    obl_body.extend_from_slice(&0u32.to_le_bytes()); // uv_set
    obl_body.push(0); // has_transform = 0
    let obl = parse_base(NifVersion::V20_0_0_5, &obl_body);
    assert_eq!(
        obl.flags, 0x0023,
        "synthesized: clamp low nibble, filter next"
    );
    assert_eq!(
        obl.clamp_mode, 3,
        "same authored clamp mode as the FNV case"
    );

    // And the clamped end of the range, so the test cannot pass by returning
    // a constant 3.
    let mut clamp_body = Vec::new();
    clamp_body.extend_from_slice(&0x0200u16.to_le_bytes()); // clamp 0, filter 2
    clamp_body.push(0);
    assert_eq!(
        parse_base(NifVersion::V20_2_0_7, &clamp_body).clamp_mode,
        0,
        "0x0200 is CLAMP_S_CLAMP_T — the 21 FNV descriptors that authored it"
    );
}

/// #3530 — Apply Mode has two on-disk homes and was read then discarded at
/// both. nif.xml annotates value 4 directly: `APPLY_HILIGHT2` is the
/// *"Parallax Flag in some Oblivion meshes"*, and Oblivion has no other
/// parallax signal (the dedicated slot-7 parallax texture is v20.2.0.5+).
#[test]
fn ni_texturing_property_apply_mode_decodes_from_both_homes() {
    let build = |version: NifVersion, flags: u16, standalone_apply: u32| {
        let header = NifHeader {
            version,
            little_endian: true,
            user_version: 11,
            user_version_2: 11,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };
        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_le_bytes()); // name
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
        if version <= NifVersion::V10_0_1_2 || version >= NifVersion::V20_1_0_2 {
            data.extend_from_slice(&flags.to_le_bytes());
        }
        if version <= NifVersion::STRING_TABLE_THRESHOLD {
            data.extend_from_slice(&standalone_apply.to_le_bytes());
        }
        data.extend_from_slice(&0u32.to_le_bytes()); // texture_count = 0
        data.push(0); // base slot: has = 0 (read unconditionally)
        data.extend_from_slice(&0u32.to_le_bytes()); // num_shader_textures
        let mut stream = NifStream::new(&data, &header);
        NiTexturingProperty::parse(&mut stream).expect("should parse")
    };

    // Oblivion (v20.0.0.5) — standalone `uint` field. 1,433 vanilla
    // properties across 741 meshes carry APPLY_HILIGHT2.
    assert_eq!(build(NifVersion::V20_0_0_5, 0, 4).apply_mode, 4);
    assert_eq!(
        build(NifVersion::V20_0_0_5, 0, 2).apply_mode,
        2,
        "APPLY_MODULATE is the no-op default — 32,810 of the 35,161 vanilla \
         Oblivion properties"
    );

    // FNV / FO3 / Skyrim (v20.2.0.7) — bits 1-3 of the `TexturingFlags`
    // word (nif.xml mask 0x000E), not a standalone field. Measured 100 %
    // APPLY_MODULATE there, so the value is inert but must still decode.
    assert_eq!(
        build(NifVersion::V20_2_0_7, 0b0000_0100, 0).apply_mode,
        2,
        "0x0004 is Apply Mode 2 in the packed layout — bits 1-3, not 0-2"
    );
    assert_eq!(
        build(NifVersion::V20_2_0_7, 0b0000_1000, 0).apply_mode,
        4,
        "0x0008 is APPLY_HILIGHT2 in the packed layout"
    );
    // The Multitexture bit (0) and Decal Count (bits 4-11) must not leak in.
    assert_eq!(
        build(NifVersion::V20_2_0_7, 0b0000_1111_0000_1001, 0).apply_mode,
        4,
        "only bits 1-3 belong to Apply Mode"
    );
}

/// Regression test for issue #400 — NiTexturingProperty decal slots
/// (Oblivion pre-20.2.0.5 path, slots 6..=texture_count-1) are now
/// retained on the block instead of silently discarded. Builds a
/// header with texture_count=8 → 2 populated decal TexDescs, checks
/// both are reachable.
#[test]
fn parse_ni_texturing_property_retains_oblivion_decal_slots() {
    // Oblivion — v20.0.0.5, user_version=11. Pre-20.2.0.5 layout:
    // slots 0..=5 are base/dark/detail/gloss/glow/bump; decals
    // start at slot 6. No normal/parallax slots in this version.
    let header = NifHeader {
        version: NifVersion::V20_0_0_5,
        little_endian: true,
        user_version: 11,
        user_version_2: 11,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    };
    let mut data = Vec::new();
    // NiObjectNET base: inline name (empty) + 0 extras + null controller.
    data.extend_from_slice(&0u32.to_le_bytes()); // name: empty inline
    data.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                    // flags u16 (v <= 10.0.1.2 OR v >= 20.1.0.2) — 20.0.0.5 is
                                                    // in the middle gap, so NO flags field. apply_mode u32 reads
                                                    // (v <= 20.1.0.1).
    data.extend_from_slice(&1u32.to_le_bytes()); // apply_mode
                                                 // texture_count = 8 → slots 0..=7 consumed, slots 6 and 7
                                                 // become decals 0 and 1.
    data.extend_from_slice(&8u32.to_le_bytes());
    // Helper: minimal TexDesc for v=20.0.0.5 with has=1.
    // v < 10.1.0.3 → ELSE branch: source_ref + 3 × u32 (clamp / filter /
    // uv_set) + has_transform bool. (20.0.0.5 is below 20.1.0.3.)
    let push_populated = |data: &mut Vec<u8>, source: i32| {
        data.push(1); // has
        data.extend_from_slice(&source.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes()); // clamp_mode
        data.extend_from_slice(&0u32.to_le_bytes()); // filter_mode
        data.extend_from_slice(&0u32.to_le_bytes()); // uv_set
        data.push(0); // has_transform = 0
    };
    let push_empty = |data: &mut Vec<u8>| {
        data.push(0); // has = 0 → TexDesc is None, 1 byte total
    };
    // Slots 0..=5 all empty.
    push_empty(&mut data); // base
    push_empty(&mut data); // dark
    push_empty(&mut data); // detail
    push_empty(&mut data); // gloss
    push_empty(&mut data); // glow
    push_empty(&mut data); // bump
                           // 20.0.0.5 is NOT >= 20.2.0.5 → the normal/parallax slots are
                           // skipped. Decal loop picks up 8-6 = 2 slots.
    push_populated(&mut data, 101); // decal 0
    push_populated(&mut data, 202); // decal 1
                                    // Shader map list trailer (since v >= 10.0.1.0).
    data.extend_from_slice(&0u32.to_le_bytes()); // num_shader_textures = 0

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop =
        NiTexturingProperty::parse(&mut stream).expect("Oblivion NiTexturingProperty should parse");
    assert_eq!(
        stream.position() as usize,
        expected_len,
        "parse consumed {} bytes, expected {}",
        stream.position(),
        expected_len
    );
    assert_eq!(prop.texture_count, 8);
    assert_eq!(
        prop.decal_textures.len(),
        2,
        "expected 2 decal TexDescs (slots 6 + 7) in retained vec"
    );
    assert_eq!(prop.decal_textures[0].source_ref.index(), Some(101));
    assert_eq!(prop.decal_textures[1].source_ref.index(), Some(202));
}

/// Regression test for issue #149 / runtime Oblivion trace:
/// NiTexturingProperty's shader-map-list tail is a `u32 count`
/// read unconditionally (no leading bool gate), per the
/// Gamebryo 2.3 source. An earlier fix (#149) followed nif.xml
/// and added a leading `has_shader_textures: bool` which
/// consumed the first byte of the u32 count, leaving the
/// parser 3 bytes short and misaligning every subsequent
/// block on Oblivion cell loads. Verify the empty-shader-list
/// case (count = 0) consumes exactly 4 bytes.
#[test]
fn parse_ni_texturing_property_with_zero_shader_maps() {
    let header = make_header(12, 83); // Skyrim LE — v20.2.0.7 path
    let mut data = Vec::new();
    // NiObjectNET: name string index, extra_data count, controller
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // flags u16 (v >= 20.1.0.2 path); no apply_mode at v20.2.0.7
    data.extend_from_slice(&0u16.to_le_bytes());
    // texture_count = 1 → only base_texture is read.
    data.extend_from_slice(&1u32.to_le_bytes());
    // base_texture TexDesc: has_texture = 0 → TexDesc skipped.
    data.push(0);
    // num_decals = texture_count.saturating_sub(8) = 0 → no loop.
    // num_shader_textures = 0 as u32 (4 bytes).
    data.extend_from_slice(&0u32.to_le_bytes());

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = NiTexturingProperty::parse(&mut stream)
        .expect("NiTexturingProperty with zero shader maps should parse");
    assert_eq!(prop.texture_count, 1);
    assert!(prop.base_texture.is_none());
    assert_eq!(
        stream.position() as usize,
        expected_len,
        "NiTexturingProperty consumed {} bytes, expected exactly {}",
        stream.position(),
        expected_len
    );
}

/// Boundary regression for #935 (post-#769 doctrine flip). nif.xml
/// gates `Apply Mode` with `until="20.1.0.1"` which is **inclusive**
/// per niftools/nifly (see version.rs doctrine). The field IS
/// present at v20.1.0.1 exactly. The first version that drops the
/// field is v20.1.0.2.
#[test]
fn parse_ni_texturing_property_apply_mode_at_v20_1_0_1_exactly() {
    let header = NifHeader {
        version: NifVersion::STRING_TABLE_THRESHOLD, // v20.1.0.1 — the until= boundary
        little_endian: true,
        user_version: 0,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    };
    let mut data = Vec::new();
    // NiObjectNETData: name = -1 (None), extras count = 0, controller = -1.
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // v20.1.0.1 is still inside `until="20.1.0.1"` (inclusive) so
    // Apply Mode IS read here. Flags is absent (gap version).
    data.extend_from_slice(&1u32.to_le_bytes()); // apply_mode = 1
    data.extend_from_slice(&0u32.to_le_bytes()); // texture_count = 0
    data.push(0); // base_texture has = 0 → None
    data.extend_from_slice(&0u32.to_le_bytes()); // shader_textures count = 0

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = NiTexturingProperty::parse(&mut stream)
        .expect("v20.1.0.1 NiTexturingProperty must consume Apply Mode under inclusive doctrine");
    assert_eq!(stream.position() as usize, expected_len);
    assert_eq!(prop.texture_count, 0);
    assert!(prop.base_texture.is_none());
    assert_eq!(prop.decal_textures.len(), 0);
}

/// Boundary above the inclusive `until="20.1.0.1"` — at v20.1.0.2
/// the Apply Mode field is finally absent and the new TexturingFlags
/// path is active (`since="20.1.0.2"`).
#[test]
fn parse_ni_texturing_property_no_apply_mode_at_v20_1_0_2() {
    let header = NifHeader {
        version: NifVersion::V20_1_0_2,
        little_endian: true,
        user_version: 0,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    };
    let mut data = Vec::new();
    // NiObjectNETData: name = -1 (None), extras count = 0, controller = -1.
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // v20.1.0.2: Flags u16 IS read (since=20.1.0.2 path), Apply Mode absent.
    data.extend_from_slice(&0u16.to_le_bytes()); // flags = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // texture_count = 0
    data.push(0); // base_texture has = 0 → None
    data.extend_from_slice(&0u32.to_le_bytes()); // shader_textures count = 0

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = NiTexturingProperty::parse(&mut stream)
        .expect("v20.1.0.2 NiTexturingProperty must skip Apply Mode under inclusive doctrine");
    assert_eq!(stream.position() as usize, expected_len);
    assert_eq!(prop.texture_count, 0);
}

/// Pre-boundary spot check: at v20.1.0.0 the `Apply Mode` field is
/// present (as it is throughout `[3.3.0.13, 20.1.0.1]` inclusive).
#[test]
fn parse_ni_texturing_property_with_apply_mode_below_v20_1_0_1() {
    let header = NifHeader {
        version: NifVersion::V20_1_0_0, // v20.1.0.0 — below the boundary
        little_endian: true,
        user_version: 0,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    };
    let mut data = Vec::new();
    // v20.1.0.0 is BELOW the v20.1.0.1 string-table boundary, so
    // `read_string` uses the length-prefixed inline path: u32 len + bytes.
    data.extend_from_slice(&0u32.to_le_bytes()); // name: empty inline (len = 0)
    data.extend_from_slice(&0u32.to_le_bytes()); // extras count = 0
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref = -1
    data.extend_from_slice(&1u32.to_le_bytes()); // apply_mode = 1 (present pre-20.1.0.1)
    data.extend_from_slice(&0u32.to_le_bytes()); // texture_count = 0
    data.push(0); // base_texture has = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // shader_textures count = 0

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = NiTexturingProperty::parse(&mut stream)
        .expect("v20.1.0.0 NiTexturingProperty must consume Apply Mode");
    assert_eq!(stream.position() as usize, expected_len);
    assert_eq!(prop.texture_count, 0);
}

/// Regression: #119 / audit NIF-302 — a shader map entry with
/// `has_map = 1` at v >= 10.1.0.0 MUST consume its
/// `Has Texture Transform` bool and, if set, the 32-byte
/// transform body. Previously the loop skipped straight from
/// `flags` to `map_id`, putting the parser 1-33 bytes short per
/// non-empty shader map entry and cascading into every following
/// block. Two variants: has_transform=0 (just the bool) and
/// has_transform=1 (bool + 32-byte body).
#[test]
fn parse_ni_texturing_property_shader_map_consumes_has_transform_bool() {
    let header = make_header(12, 83); // Skyrim LE — v20.2.0.7, >= 20.1.0.3 flags path
    let mut data = Vec::new();
    // NiObjectNET base
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // NiProperty flags + texture_count = 0 (no slot-0 textures).
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    // `read_tex_desc` for base_texture runs unconditionally — reads
    // `has: bool` even when texture_count=0. Set it to 0 for an empty
    // slot entry.
    data.push(0); // base_texture has = 0
                  // num_shader_textures = 1
    data.extend_from_slice(&1u32.to_le_bytes());
    // Shader map entry — has_map = 1, then body.
    data.push(1); // has_map
    data.extend_from_slice(&7i32.to_le_bytes()); // source_ref
    data.extend_from_slice(&0x0102u16.to_le_bytes()); // flags (v >= 20.1.0.3)
    data.push(0); // has_transform = 0 (no trailing body)
    data.extend_from_slice(&42u32.to_le_bytes()); // map_id

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let _prop = NiTexturingProperty::parse(&mut stream).unwrap();
    assert_eq!(
        stream.position() as usize,
        expected_len,
        "shader map entry with has_transform=0 must consume the bool \
         between flags and map_id"
    );
}

#[test]
fn parse_ni_texturing_property_shader_map_consumes_full_transform() {
    let header = make_header(12, 83);
    let mut data = Vec::new();
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    // base_texture has = 0 (unconditional read).
    data.push(0);
    data.extend_from_slice(&1u32.to_le_bytes());
    // has_map = 1
    data.push(1);
    data.extend_from_slice(&11i32.to_le_bytes()); // source_ref
    data.extend_from_slice(&0x0304u16.to_le_bytes()); // flags
    data.push(1); // has_transform = 1 → 32-byte body follows
                  // TexTransform: translation(2) + scale(2) + rotation(1) + method(1 u32) + center(2) = 8 × 4B.
    for f in [0.25f32, -0.5, 2.0, 3.0, 0.75] {
        data.extend_from_slice(&f.to_le_bytes());
    }
    data.extend_from_slice(&2u32.to_le_bytes()); // transform_method
    data.extend_from_slice(&0.1f32.to_le_bytes()); // center x
    data.extend_from_slice(&0.2f32.to_le_bytes()); // center y
    data.extend_from_slice(&99u32.to_le_bytes()); // map_id

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let _prop = NiTexturingProperty::parse(&mut stream).unwrap();
    assert_eq!(
        stream.position() as usize,
        expected_len,
        "shader map entry with has_transform=1 must consume the \
         32-byte TexTransform body between flags and map_id"
    );
}

/// Regression test: `num_shader_textures = 1` + one shader map
/// with `has = 0` (no body) must parse to exactly `4 (count) +
/// 1 (has)` = 5 trailing bytes. Exercises the loop logic without
/// requiring a full shader Map body.
#[test]
fn parse_ni_texturing_property_with_empty_shader_map_entry() {
    let header = make_header(12, 83);
    let mut data = Vec::new();
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes()); // flags
    data.extend_from_slice(&1u32.to_le_bytes()); // texture_count
    data.push(0); // base_texture has=0
    data.extend_from_slice(&1u32.to_le_bytes()); // num_shader_textures = 1
    data.push(0); // shader map has = 0

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let _prop = NiTexturingProperty::parse(&mut stream).unwrap();
    assert_eq!(stream.position() as usize, expected_len);
}

/// Regression test for issue #219: the TexDesc's per-slot UV transform
/// must be captured (previously the 32 transform bytes were skipped
/// and the values discarded). Builds a minimal NiTexturingProperty at
/// v20.2.0.7 with a base_texture that has `Has Texture Transform = 1`
/// and verifies that `prop.base_texture.transform` carries the exact
/// values — and that the stream position matches the payload size.
#[test]
fn parse_ni_texturing_property_captures_base_uv_transform() {
    let header = make_header(12, 83); // Skyrim LE — v20.2.0.7, >= 20.1.0.3 flags path
    let mut data = Vec::new();

    // NiObjectNET base.
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());

    // NiProperty flags (u16), texture_count = 1.
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());

    // base_texture TexDesc (has = 1, source_ref = 5, flags = 0x0302,
    // has_transform = 1, then the 32-byte body).
    data.push(1); // has
    data.extend_from_slice(&5i32.to_le_bytes()); // source_ref
    data.extend_from_slice(&0x0302u16.to_le_bytes()); // flags
    data.push(1); // has_transform
                  // Translation (u, v)
    data.extend_from_slice(&0.25f32.to_le_bytes());
    data.extend_from_slice(&(-0.5f32).to_le_bytes());
    // Scale (su, sv)
    data.extend_from_slice(&2.0f32.to_le_bytes());
    data.extend_from_slice(&3.0f32.to_le_bytes());
    // Rotation
    data.extend_from_slice(&0.75f32.to_le_bytes());
    // Transform method (u32 enum)
    data.extend_from_slice(&2u32.to_le_bytes());
    // Center (cu, cv)
    data.extend_from_slice(&0.1f32.to_le_bytes());
    data.extend_from_slice(&0.2f32.to_le_bytes());

    // No decals, no shader map list.
    data.extend_from_slice(&0u32.to_le_bytes());

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = NiTexturingProperty::parse(&mut stream).unwrap();
    assert_eq!(stream.position() as usize, expected_len);

    let base = prop
        .base_texture
        .as_ref()
        .expect("base_texture present (has=1)");
    assert_eq!(base.source_ref.0, 5);
    assert_eq!(base.flags, 0x0302);
    let tx = base
        .transform
        .expect("transform captured (has_transform=1)");
    assert!((tx.translation[0] - 0.25).abs() < 1e-6);
    assert!((tx.translation[1] + 0.5).abs() < 1e-6);
    assert!((tx.scale[0] - 2.0).abs() < 1e-6);
    assert!((tx.scale[1] - 3.0).abs() < 1e-6);
    assert!((tx.rotation - 0.75).abs() < 1e-6);
    assert_eq!(tx.transform_method, 2);
    assert!((tx.center[0] - 0.1).abs() < 1e-6);
    assert!((tx.center[1] - 0.2).abs() < 1e-6);
}

/// Parse the same layout with `has_transform = 0` and confirm the
/// parser leaves `transform = None` instead of inventing identity.
#[test]
fn parse_ni_texturing_property_transform_absent() {
    let header = make_header(12, 83);
    let mut data = Vec::new();
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());

    data.push(1); // base_texture has = 1
    data.extend_from_slice(&7i32.to_le_bytes()); // source_ref
    data.extend_from_slice(&0u16.to_le_bytes()); // flags
    data.push(0); // has_transform = 0 (no body bytes)

    data.extend_from_slice(&0u32.to_le_bytes()); // num_shader_textures

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = NiTexturingProperty::parse(&mut stream).unwrap();
    assert_eq!(stream.position() as usize, expected_len);
    let base = prop.base_texture.as_ref().unwrap();
    assert_eq!(base.source_ref.0, 7);
    assert!(base.transform.is_none());
}

/// Regression: #429 — at v20.0.0.5 (Oblivion), slots 6-7
/// (`Has Normal Texture` / `Has Parallax Texture`) do NOT exist
/// per nif.xml — they're gated `since 20.2.0.5`. Pre-fix the
/// parser read those bools unconditionally, so an Oblivion NIF
/// with `texture_count == 8` over-consumed 2 bytes of what
/// should have been decal-slot bools, then potentially more
/// bytes if those phantom bools came back as `1`. With no
/// `block_sizes` table to resync, every following block
/// misaligned. This test exercises the exact failure shape:
/// build a v20.0.0.5 NiTexturingProperty with `texture_count = 8`
/// (base + 5 absent slots + 2 decals), assert byte-exact
/// consumption, and confirm the decal `has = 0` bools after
/// position survive.
#[test]
fn parse_ni_texturing_property_oblivion_skips_normal_parallax_slots() {
    let mut header = make_header(11, 11);
    header.version = NifVersion::V20_0_0_5; // v20.0.0.5 — Oblivion
    let mut data = Vec::new();
    // NiObjectNET on Oblivion (v20.0.0.5 < 20.1.0.1): name is a
    // length-prefixed inline string (u32 length, then bytes), not
    // a string-table index. Write zero-length to mean "no name".
    data.extend_from_slice(&0u32.to_le_bytes()); // name length = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count = 0
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref = NULL
                                                    // No flags field — Oblivion sits in the 10.0.1.3..20.1.0.1
                                                    // gap where NiTexturingProperty has neither the legacy u16
                                                    // flags nor the modern TexturingFlags. Apply mode IS still
                                                    // present (until 20.1.0.1) — write the u32.
    data.extend_from_slice(&0u32.to_le_bytes()); // apply_mode
    data.extend_from_slice(&8u32.to_le_bytes()); // texture_count = 8
                                                 // Slots 0-5: each is a `has = 0` bool (no body). Slots 6-7
                                                 // do NOT exist on this version — pre-fix the parser would
                                                 // also try to read those, eating 2 bytes from below.
    data.extend_from_slice(&[0; 6]); // has = false
                                     // Pre-v20.2.0.5: decals start at slot 6. With texture_count=8,
                                     // num_decals = 8-6 = 2 decal slots, each a `has = 0` bool
                                     // (no body since has=false).
    data.extend_from_slice(&[0; 2]); // decal has = false

    // Trailer: shader textures count = 0 (since v10.0.1.0+).
    data.extend_from_slice(&0u32.to_le_bytes());

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = NiTexturingProperty::parse(&mut stream).unwrap();
    assert_eq!(
        stream.position() as usize,
        expected_len,
        "Oblivion NiTexturingProperty must consume exactly the bytes \
         it authored — pre-#429 the parser ate `has_normal` + \
         `has_parallax` bools that v20.0.0.5 doesn't carry, \
         over-consuming 2 bytes from the decal slot below"
    );
    // Sanity: every slot read came back empty (we wrote `has=0`
    // for everything).
    assert!(prop.base_texture.is_none());
    assert!(prop.dark_texture.is_none());
    assert!(prop.bump_texture.is_none());
    // normal_texture must be `None` because Oblivion doesn't
    // have the slot — pre-#429 it would have been Some(...) or
    // a parse error from over-reading.
    assert!(prop.normal_texture.is_none());
}

/// Regression: #484 — pin the `num_decals` boundary for v20.2.0.5+.
///
/// The #400/#429 fix computes `num_decals = texture_count.saturating_sub(8)`
/// on v20.2.0.5+ (FO3/FNV/SkyrimLE pre-BSTriShape path). `count == 8`
/// is the exact threshold where slots 0..7 are consumed (base/dark/
/// detail/gloss/glow/bump/normal/parallax) and no decals remain.
/// A future rewrite that flips the comparison (e.g. `saturating_sub(7)`,
/// or `>` instead of `>=`) would silently consume one extra decal
/// byte here and misalign every downstream block. The next-larger
/// test pins `count == 9 → 1 decal` from the other side of the boundary.
#[test]
fn num_decals_boundary_v20_2_0_5_count_8_yields_zero() {
    let header = make_header(11, 34); // FNV bsver=34
    let mut data = Vec::new();
    // NiObjectNET: string-table index for `name` (v >= 20.1.0.1).
    data.extend_from_slice(&(-1i32).to_le_bytes()); // name index = -1
    data.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                    // NiProperty.Flags (u16) present since 20.1.0.2.
    data.extend_from_slice(&0u16.to_le_bytes());
    // apply_mode omitted (gated `<= 20.1.0.1`) — v20.2.0.7 skips it.
    data.extend_from_slice(&8u32.to_le_bytes()); // texture_count = 8
                                                 // Slots 0..=7: base/dark/detail/gloss/glow/bump/normal/parallax.
                                                 // All `has = 0` — the parser's fixed-slot loop consumes every
                                                 // one so slot accounting lines up. No decals at count=8.
    data.extend_from_slice(&[0; 8]); // has = 0
                                     // Shader textures trailer.
    data.extend_from_slice(&0u32.to_le_bytes());

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = NiTexturingProperty::parse(&mut stream).expect("parse");
    assert_eq!(stream.position() as usize, expected_len);
    assert_eq!(prop.texture_count, 8);
    assert_eq!(
        prop.decal_textures.len(),
        0,
        "v20.2.0.5+ texture_count=8 must yield zero decals — slots 0..=7 consume the fixed allocation"
    );
}

/// Regression: #484 — v20.2.0.5+ `count == 9 → num_decals == 1`.
/// Pairs with the `count == 8` test above to lock both sides of the
/// `saturating_sub(8)` threshold.
#[test]
fn num_decals_boundary_v20_2_0_5_count_9_yields_one() {
    let header = make_header(11, 34);
    let mut data = Vec::new();
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes()); // flags
    data.extend_from_slice(&9u32.to_le_bytes()); // texture_count = 9
                                                 // Slots 0..=7 empty + 1 populated decal.
    data.extend_from_slice(&[0; 8]);
    // Decal 0 — v20.2.0.7 uses the modern TexDesc (v >= 20.1.0.3):
    //   has(bool) + source_ref(i32) + flags(u16) + has_transform(bool).
    data.push(1);
    data.extend_from_slice(&42i32.to_le_bytes()); // source_ref = 42
    data.extend_from_slice(&0u16.to_le_bytes()); // flags
    data.push(0); // has_transform = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // shader textures trailer

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = NiTexturingProperty::parse(&mut stream).expect("parse");
    assert_eq!(stream.position() as usize, expected_len);
    assert_eq!(prop.texture_count, 9);
    assert_eq!(
        prop.decal_textures.len(),
        1,
        "v20.2.0.5+ texture_count=9 must yield exactly one decal — locks saturating_sub(8) against off-by-one regressions"
    );
    assert_eq!(prop.decal_textures[0].source_ref.index(), Some(42));
}

/// Regression: #484 — pre-20.2.0.5 `count == 6 → num_decals == 0`.
/// Mirrors the v20.2.0.5+ test above but for the Oblivion-era
/// `saturating_sub(6)` branch (no normal + parallax slots).
#[test]
fn num_decals_boundary_pre_v20_2_0_5_count_6_yields_zero() {
    let mut header = make_header(11, 11);
    header.version = NifVersion::V20_0_0_5; // v20.0.0.5 — Oblivion
    let mut data = Vec::new();
    // Oblivion NiObjectNET: inline-string name (u32 length + bytes).
    data.extend_from_slice(&0u32.to_le_bytes()); // name length = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                    // No flags field on v20.0.0.5 (10.0.1.3..20.1.0.1 gap).
    data.extend_from_slice(&0u32.to_le_bytes()); // apply_mode
    data.extend_from_slice(&6u32.to_le_bytes()); // texture_count = 6
    data.extend_from_slice(&[0; 6]); // each slot `has = 0`
    data.extend_from_slice(&0u32.to_le_bytes()); // shader textures trailer

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = NiTexturingProperty::parse(&mut stream).expect("parse");
    assert_eq!(stream.position() as usize, expected_len);
    assert_eq!(prop.texture_count, 6);
    assert_eq!(
        prop.decal_textures.len(),
        0,
        "pre-20.2.0.5 texture_count=6 must yield zero decals — locks saturating_sub(6) threshold"
    );
}

/// Regression: #484 — pre-20.2.0.5 `count == 7 → num_decals == 1`.
/// Pairs with the `count == 6` test above for the Oblivion branch.
#[test]
fn num_decals_boundary_pre_v20_2_0_5_count_7_yields_one() {
    let mut header = make_header(11, 11);
    header.version = NifVersion::V20_0_0_5;
    let mut data = Vec::new();
    data.extend_from_slice(&0u32.to_le_bytes()); // name length
    data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
    data.extend_from_slice(&0u32.to_le_bytes()); // apply_mode
    data.extend_from_slice(&7u32.to_le_bytes()); // texture_count = 7
    data.extend_from_slice(&[0; 6]); // slots 0..=5 empty
                                     // Decal 0 — v20.0.0.5 is below 20.1.0.3, so TexDesc ELSE branch:
                                     //   has(bool) + source_ref + 3×u32 (clamp/filter/uv_set) + has_transform
    data.push(1);
    data.extend_from_slice(&99i32.to_le_bytes()); // source_ref = 99
    data.extend_from_slice(&0u32.to_le_bytes()); // clamp
    data.extend_from_slice(&0u32.to_le_bytes()); // filter
    data.extend_from_slice(&0u32.to_le_bytes()); // uv_set
    data.push(0); // has_transform
    data.extend_from_slice(&0u32.to_le_bytes()); // shader textures trailer

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = NiTexturingProperty::parse(&mut stream).expect("parse");
    assert_eq!(stream.position() as usize, expected_len);
    assert_eq!(prop.texture_count, 7);
    assert_eq!(
        prop.decal_textures.len(),
        1,
        "pre-20.2.0.5 texture_count=7 must yield exactly one decal"
    );
    assert_eq!(prop.decal_textures[0].source_ref.index(), Some(99));
}

/// Regression: #2004 — nif.xml defines exactly 4 decal slots. An
/// anomalous `texture_count` implying more than 4 (here 13, which on
/// v20.2.0.5+ computes `num_decals = 13 - 8 = 5`) must be rejected as
/// a parse error rather than reading TexDescs the format doesn't
/// define and misaligning every following block.
#[test]
fn num_decals_above_fixed_maximum_is_parse_error() {
    let header = make_header(11, 34); // FNV bsver=34
    let mut data = Vec::new();
    data.extend_from_slice(&(-1i32).to_le_bytes()); // name index = -1
    data.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
    data.extend_from_slice(&0u16.to_le_bytes()); // flags
    data.extend_from_slice(&13u32.to_le_bytes()); // texture_count = 13 -> num_decals = 5
                                                  // Slots 0..=7 (base/dark/detail/gloss/glow/bump/normal/parallax), all
                                                  // empty — must be fully consumed before the decal-count check runs.
    data.extend_from_slice(&[0; 8]); // has = 0

    let mut stream = NifStream::new(&data, &header);
    let err = NiTexturingProperty::parse(&mut stream)
        .expect_err("texture_count implying >4 decal slots must be rejected");
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

/// Regression for #1843 (NIF-D1-01) — `read_tex_desc`'s leading `has`
/// bool (nif.xml `NiTexturingProperty.Has Base Texture` etc., no
/// `since=` gate) is the version-dependent `bool` basic type: 32-bit at
/// v4.0.0.2 (Morrowind-era NetImmerse, pre-4.1.0.1), not the fixed 1 byte
/// the parser used to read unconditionally. `NiTexturingProperty` predates
/// Gamebryo, so `base_texture` is reachable in this band on real content.
#[test]
fn parse_ni_texturing_property_at_v4_0_0_2_reads_32_bit_has_base_texture() {
    let header = NifHeader {
        version: NifVersion::V4_0_0_2,
        little_endian: true,
        user_version: 0,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    };
    let mut data = Vec::new();
    // NiObjectNETData, pre-string-table (v < 20.1.0.1) inline path:
    // name = u32 len-prefixed string (empty), single extra_data ref
    // (v < 10.0.1.0), controller_ref.
    data.extend_from_slice(&0u32.to_le_bytes()); // name: empty inline (len = 0)
    data.extend_from_slice(&(-1i32).to_le_bytes()); // extra_data_ref = NULL
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref = NULL
                                                    // NiProperty.Flags: `until=10.0.1.2` — present at v4.0.0.2.
    data.extend_from_slice(&0u16.to_le_bytes()); // flags = 0
                                                 // Apply Mode: `since=3.3.0.13, until=20.1.0.1` — present at v4.0.0.2.
    data.extend_from_slice(&1u32.to_le_bytes()); // apply_mode = 1 (APPLY_MODULATE)
    data.extend_from_slice(&0u32.to_le_bytes()); // texture_count = 0
                                                 // `Has Base Texture` — the version-dependent bool, 32-bit here.
    data.extend_from_slice(&0u32.to_le_bytes()); // base_texture has = false (32-bit)
                                                 // No shader-textures trailer: `since=10.0.1.0`, absent at v4.0.0.2.

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = NiTexturingProperty::parse(&mut stream)
        .expect("v4.0.0.2 NiTexturingProperty must parse with 32-bit has_base_texture");
    assert_eq!(
        stream.position() as usize,
        expected_len,
        "at v4.0.0.2, Has Base Texture must be read as 32-bit; a fixed \
         1-byte read would leave 3 bytes unconsumed and misalign every \
         downstream block"
    );
    assert!(prop.base_texture.is_none());
}

/// The mirror failure mode: the OLD (pre-fix) fixed-1-byte layout is 3
/// bytes shorter than the wire-correct 32-bit layout. Feeding it to the
/// current (fixed) parser at v4.0.0.2 must fail with an EOF-class error.
#[test]
fn parse_ni_texturing_property_at_v4_0_0_2_rejects_8_bit_bool_layout() {
    let header = NifHeader {
        version: NifVersion::V4_0_0_2,
        little_endian: true,
        user_version: 0,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    };
    let mut data = Vec::new();
    data.extend_from_slice(&0u32.to_le_bytes()); // name: empty inline
    data.extend_from_slice(&(-1i32).to_le_bytes()); // extra_data_ref
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
    data.extend_from_slice(&0u16.to_le_bytes()); // flags
    data.extend_from_slice(&1u32.to_le_bytes()); // apply_mode
    data.extend_from_slice(&0u32.to_le_bytes()); // texture_count = 0
    data.push(0); // base_texture has (WRONG WIDTH — pre-fix 1-byte layout)

    let mut stream = NifStream::new(&data, &header);
    assert!(
        NiTexturingProperty::parse(&mut stream).is_err(),
        "an 8-bit-bool-layout buffer (3 bytes shorter than the wire-correct \
         32-bit layout) must fail to parse at v4.0.0.2 — if it succeeds, the \
         parser silently regressed back to a fixed-width bool read"
    );
}
