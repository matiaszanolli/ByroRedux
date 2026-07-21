//! Fallout 4 era shader-property tests. Split from `shader_tests.rs` (#2056);
//! helpers live in the parent module.

use super::*;


/// Regression for #409: at `BSVER == 131` the parser must read
/// neither the u32 flag pair (gated on `bsver <= crate::version::bsver::FALLOUT4`) nor the
/// CRC-array counts (gated on `bsver >= crate::version::bsver::FO4_CRC_FLAGS`). This is NOT a bug —
/// nif.xml's `#BS_FO4#` is strict `BSVER == 130` and `#BS_GTE_132#`
/// starts at 132, leaving 131 as an intentional dev-stream gap
/// where the flag fields are absent altogether.
///
/// The test constructs a body 8 bytes shorter than BSVER 130 (no
/// flag pair) and assumes the pre-flag-pair part plus the
/// post-CRC part line up with `bsver == crate::version::bsver::FO4_SHADER_GAP`'s expected layout.
/// Consumes exactly the authored bytes.
#[test]
fn bs_lighting_bsver_131_skips_flag_pair_and_crc_counts() {
    let header = make_fo4_header_with_bsver(131);
    let mut data = Vec::new();

    // shader_type (read before NiObjectNET for BSVER 83-130... but
    // also 131 since the gate at `shader.rs::parse` is `bsver <
    // 155`). See the pre-flag-pair block in the source.
    data.extend_from_slice(&1u32.to_le_bytes()); // EnvironmentMap
                                                 // NiObjectNET: name (string-table idx 0), empty extra_data_refs,
                                                 // no controller.
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // NO flag pair at bsver == crate::version::bsver::FO4_SHADER_GAP (gate is `bsver <= crate::version::bsver::FALLOUT4`).
    // NO Num SF1/SF2 at bsver == crate::version::bsver::FO4_SHADER_GAP (gate is `bsver >= crate::version::bsver::FO4_CRC_FLAGS`).
    // Next field: uv_offset + uv_scale.
    for v in [0.0f32, 0.0, 1.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // texture_set_ref
    data.extend_from_slice(&3i32.to_le_bytes());
    // emissive_color + emissive_multiple
    for v in [0.0f32, 0.5, 1.0, 2.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // Root Material (FO4+ NiFixedString)
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // texture_clamp_mode, alpha, refraction_strength, glossiness
    data.extend_from_slice(&3u32.to_le_bytes());
    for v in [0.8f32, 0.0, 0.5] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // specular_color + specular_strength
    for v in [1.0f32, 0.9, 0.8, 1.5] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // FO4 common fields (BSVER 130..139): subsurface/rimlight/backlight/
    // grayscale/fresnel. rimlight=FLT_MAX so backlight bytes are
    // present per nif.xml 6609 (#1175).
    data.extend_from_slice(&0.3f32.to_le_bytes());
    data.extend_from_slice(&f32::MAX.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&0.7f32.to_le_bytes());
    data.extend_from_slice(&5.0f32.to_le_bytes());
    // Wetness (7 floats — same as BSVER 130; the wetness gate is
    // `>= 130` not per-BSVER-specific). `env_map_scale` slot
    // (offset 4 within the wetness block) only present at
    // `bsver == crate::version::bsver::FALLOUT4` strictly — at 131 the parser reads 6 floats.
    for v in [0.1f32, 0.2, 0.3, 0.5, 0.6, 0.95] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // shader_type=1 (EnvironmentMap) trailing: env_map_scale + 2 bools.
    data.extend_from_slice(&0.75f32.to_le_bytes());
    data.push(1u8);
    data.push(0u8);

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    assert_eq!(prop.shader_type, 1);
    // Flag pair stays at the pre-fill-in default `0` because the
    // 131 gate skips the u32 read — this is what the test pins.
    assert_eq!(prop.shader_flags_1, 0, "bsver=131 skips flag pair");
    assert_eq!(prop.shader_flags_2, 0, "bsver=131 skips flag pair");
    // CRC arrays stay empty because Num SF1/SF2 are gated `>= 132`.
    assert!(prop.sf1_crcs.is_empty());
    assert!(prop.sf2_crcs.is_empty());
    // Every authored byte consumed — no under-read into next block.
    assert_eq!(
        stream.position() as usize,
        expected_len,
        "bsver=131 body must consume exactly what was authored"
    );
}


/// Regression for #409: at `BSVER == 132` the parser must skip the
/// flag pair AND read `Num SF1` + the SF1 CRC array (but NOT
/// `Num SF2` which is gated on `>= 152`). Exercises the other side
/// of the BSVER 131 gap.
#[test]
fn bs_lighting_bsver_132_reads_crc_counts_but_not_flag_pair() {
    let header = make_fo4_header_with_bsver(132);
    let mut data = Vec::new();

    // shader_type + NiObjectNET as usual.
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // NO flag pair at bsver == crate::version::bsver::FO4_CRC_FLAGS (gate is `bsver <= crate::version::bsver::FALLOUT4`).
    // Num SF1 = 2, Num SF2 NOT read (gated on `bsver >= crate::version::bsver::FO76_SF2_CRCS`).
    data.extend_from_slice(&2u32.to_le_bytes());
    // Two SF1 CRC32 entries.
    data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
    data.extend_from_slice(&0xCAFEBABEu32.to_le_bytes());
    // Standard post-flag payload.
    for v in [0.0f32, 0.0, 1.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    data.extend_from_slice(&3i32.to_le_bytes());
    for v in [0.0f32, 0.5, 1.0, 2.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&3u32.to_le_bytes());
    for v in [0.8f32, 0.0, 0.5] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    for v in [1.0f32, 0.9, 0.8, 1.5] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    data.extend_from_slice(&0.3f32.to_le_bytes());
    // rimlight=FLT_MAX → backlight present (#1175 / nif.xml 6609).
    data.extend_from_slice(&f32::MAX.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&0.7f32.to_le_bytes());
    data.extend_from_slice(&5.0f32.to_le_bytes());
    // Wetness: same 6 floats as bsver 131 (no env_map_scale slot
    // since that's strict `bsver == crate::version::bsver::FALLOUT4`).
    for v in [0.1f32, 0.2, 0.3, 0.5, 0.6, 0.95] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    data.extend_from_slice(&0.75f32.to_le_bytes());
    data.push(1u8);
    data.push(0u8);

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    assert_eq!(prop.shader_type, 1);
    assert_eq!(prop.shader_flags_1, 0);
    assert_eq!(prop.shader_flags_2, 0);
    assert_eq!(prop.sf1_crcs, vec![0xDEADBEEF, 0xCAFEBABE]);
    assert!(
        prop.sf2_crcs.is_empty(),
        "Num SF2 requires bsver >= crate::version::bsver::FO76_SF2_CRCS"
    );
    assert_eq!(
        stream.position() as usize,
        expected_len,
        "bsver=132 must read CRC array but skip flag pair"
    );
}


/// Regression for #2002: BSVER 140+ no longer carries the legacy shader
/// type before `NiObjectNETData`. Reading it shifts Name and every following
/// field by four bytes.
#[test]
fn bs_lighting_bsver_140_skips_legacy_shader_type() {
    let header = make_fo4_header_with_bsver(140);
    let mut data = Vec::new();

    // NiObjectNET is the first field at BSVER >= FO4_DLC_UPPER.
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // CRC flags: Num SF1 only; Num SF2 starts at BSVER 152.
    data.extend_from_slice(&0u32.to_le_bytes());
    for v in [0.0f32, 0.0, 1.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    data.extend_from_slice(&3i32.to_le_bytes());
    for v in [0.0f32, 0.5, 1.0, 2.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&3u32.to_le_bytes());
    for v in [0.8f32, 0.0, 0.5] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    for v in [1.0f32, 0.9, 0.8, 1.5] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // BSVER 140 omits the FO4 subsurface block.
    data.extend_from_slice(&0.7f32.to_le_bytes());
    data.extend_from_slice(&5.0f32.to_le_bytes());
    // Wetness has six floats; shader_type defaults to 0, so no trailing data.
    for v in [0.1f32, 0.2, 0.3, 0.5, 0.6, 0.95] {
        data.extend_from_slice(&v.to_le_bytes());
    }

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    assert_eq!(prop.shader_type, 0);
    assert!(prop.sf1_crcs.is_empty());
    assert!(prop.sf2_crcs.is_empty());
    assert_eq!(stream.position() as usize, expected_len);
}


/// #1223 / D4-NEW-01 regression — the BSVER=130 BSLSP wire format does
/// NOT carry `env_map_scale` in the wetness block. Pre-#1223 the
/// wetness gate was `bsver == FALLOUT4`, which caused a bogus duplicate
/// read of `env_map_scale` (once here, once in the shader_type=1
/// trailing) and drifted every vanilla FO4 BSLSP by -4 (1.87M observed
/// over-reads on the FO4 corpus, masquerading as parse-rate ok thanks
/// to block_size recovery). This test pins the post-fix invariant:
/// parsing a 140-byte BSVER=130 BSLSP fixture (shader_type=0, no
/// trailing) consumes exactly 140 bytes — zero drift.
#[test]
fn parse_bs_lighting_fo4_bsver130_consumes_exactly_140_bytes() {
    let header = make_fo4_header();
    let mut data = Vec::new();
    // shader_type=0 (Default) — no trailing data after wetness.
    data.extend_from_slice(&0u32.to_le_bytes());
    // NiObjectNET: name (string-ref), extras_count, controller
    data.extend_from_slice(&3i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // shader_flags_1, shader_flags_2
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    // uv_offset, uv_scale
    for v in [0.0f32, 0.0, 1.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // texture_set_ref = NONE
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // emissive_color, emissive_multiple
    for v in [0.0f32, 0.0, 0.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // Root Material (string-ref)
    data.extend_from_slice(&4i32.to_le_bytes());
    // texture_clamp_mode, alpha, refraction_strength, glossiness
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    // specular_color, specular_strength
    for v in [0.0f32, 0.0, 0.0, 0.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // FO4 common: subsurface, rim, back (rim=FLT_MAX → back present)
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&f32::MAX.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    // grayscale, fresnel_power
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&5.0f32.to_le_bytes());
    // WetnessParams: 6 floats at BSVER=130 — NO env_map_scale slot.
    for v in [-1.0f32; 6] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // No shader_type=0 trailing.
    assert_eq!(
        data.len(),
        140,
        "BSVER=130 + shader_type=0 wire format = 140 B"
    );

    let mut stream = NifStream::new(&data, &header);
    let _prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    assert_eq!(
        stream.position(),
        140,
        "BSLSP at BSVER=130 must consume exactly 140 bytes — no env_map_scale duplicate-read (#1223)",
    );
}

/// FO4 appends a blend alpha to the legacy SkinTint RGB triple. It must
/// survive parsing: character base meshes commonly carry a placeholder RGB
/// with alpha zero, meaning "leave the actor's resolved skin texture alone."
/// Dropping the alpha made the renderer substitute one and black out bodies.
#[test]
fn parse_bs_lighting_fo4_skin_tint_preserves_alpha() {
    let header = make_fo4_header();
    let mut data = build_bs_lighting_fo4_env_map();
    // The common BSVER=130 body is 140 bytes. Replace the EnvironmentMap
    // trailing payload with SkinTint's RGB + alpha payload.
    data.truncate(140);
    data[0..4].copy_from_slice(&5u32.to_le_bytes());
    for value in [0.0f32, 0.0, 0.0, 0.0] {
        data.extend_from_slice(&value.to_le_bytes());
    }

    let mut stream = NifStream::new(&data, &header);
    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    match prop.shader_type_data {
        ShaderTypeData::SkinTint {
            skin_tint_color,
            skin_tint_alpha,
        } => {
            assert_eq!(skin_tint_color, [0.0; 3]);
            assert_eq!(skin_tint_alpha, Some(0.0));
        }
        other => panic!("expected SkinTint, got {other:?}"),
    }
    assert_eq!(stream.position(), data.len() as u64);
}


#[test]
fn parse_bs_lighting_fo4_env_map_with_wetness() {
    let header = make_fo4_header();
    let data = build_bs_lighting_fo4_env_map();
    let mut stream = NifStream::new(&data, &header);

    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    assert_eq!(prop.shader_type, 1);
    assert_eq!(prop.shader_flags_1, 0x80000000); // FO4 flags read correctly
                                                 // FO4 authors this as "smoothness" 0–1; parser normalizes to the
                                                 // 0–100 glossiness scale so downstream consumers stay in one convention.
                                                 // Wire byte is 0.5 → post-normalize = 50.0. 1e-4 tolerance because
                                                 // the conversion amplifies the f32 representation error by 100×.
    assert!((prop.glossiness - 50.0).abs() < 1e-4);
    assert!((prop.subsurface_rolloff - 0.3).abs() < 1e-6);
    // #1175: rimlight=FLT_MAX is the sentinel that gates Backlight presence.
    assert_eq!(prop.rimlight_power, f32::MAX);
    assert!((prop.backlight_power - 1.0).abs() < 1e-6);
    assert!((prop.grayscale_to_palette_scale - 0.7).abs() < 1e-6);
    assert!((prop.fresnel_power - 5.0).abs() < 1e-6);
    // Wetness params — BSVER=130 reads 6 floats (#1223). env_map_scale
    // belongs to the shader_type=1 trailing block at BSVER < FO4_DLC_UPPER.
    let w = prop.wetness.as_ref().unwrap();
    assert!((w.spec_scale - 0.1).abs() < 1e-6);
    assert_eq!(
        w.env_map_scale, 0.0,
        "env_map_scale stays at default in wetness at BSVER < 140 (#1223)"
    );
    assert!((w.metalness - 0.6).abs() < 1e-6);
    assert!(
        (w.unknown_1 - 0.95).abs() < 1e-6,
        "wetness.unknown_1 should round-trip at BSVER=130 (#403)"
    );
    // Shader type data: EnvironmentMap
    match prop.shader_type_data {
        ShaderTypeData::EnvironmentMap { env_map_scale } => {
            assert!((env_map_scale - 0.75).abs() < 1e-6);
        }
        _ => panic!("expected EnvironmentMap"),
    }
    assert_eq!(stream.position(), data.len() as u64);
}


/// #1175 — pin the inverted-case Backlight Power gate. Per nif.xml 6609,
/// Backlight Power is present iff Rimlight Power is the FLT_MAX sentinel.
/// When Rimlight Power is a real finite override (e.g. 2.5), no Backlight
/// float follows on disk — the next 4 bytes are Grayscale to Palette Scale.
///
/// Pre-#1175 the gate was logically inverted (`rim < 3.0e38 → read backlight`),
/// so a fixture with `rim=2.5` could match the inverted code by happening to
/// author a backlight float. This test pins the spec-correct shape: a finite
/// rim, NO backlight bytes, the grayscale value sits where backlight would.
#[test]
fn parse_bs_lighting_fo4_finite_rimlight_skips_backlight() {
    let header = make_fo4_header();
    let mut data = build_bs_lighting_fo4_env_map();

    // Locate the rim/back/gray triple inside the assembled body. The
    // helper writes `subsurface_rolloff=0.3` followed by `rim=FLT_MAX`,
    // `back=1.0`, `gray=0.7`. We rewrite the first 12 bytes after the
    // subsurface marker so the on-disk layout is:
    //   rim = 2.5  (finite, NOT the FLT_MAX sentinel)
    //   gray = 0.7  (immediately after rim — no backlight float)
    //   fresnel = 5.0  (shifted 4 bytes earlier vs sentinel layout)
    // and trim 4 trailing bytes so the buffer length matches.
    let mut new_data = Vec::with_capacity(data.len() - 4);
    let subsurface_off = data
        .windows(4)
        .position(|w| w == 0.3f32.to_le_bytes())
        .expect("locate subsurface marker");
    new_data.extend_from_slice(&data[..subsurface_off + 4]);
    new_data.extend_from_slice(&2.5f32.to_le_bytes()); // rim (finite override)
                                                       // backlight bytes intentionally absent
    new_data.extend_from_slice(&0.7f32.to_le_bytes()); // grayscale (was after backlight)
    new_data.extend_from_slice(&5.0f32.to_le_bytes()); // fresnel
                                                       // skip past the four floats the helper wrote after subsurface
                                                       // (rim, back, gray, fresnel = 16 B); resume at wetness.
    new_data.extend_from_slice(&data[subsurface_off + 4 + 4 * 4..]);
    data = new_data;

    let mut stream = NifStream::new(&data, &header);
    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    assert!((prop.rimlight_power - 2.5).abs() < 1e-6);
    assert_eq!(
        prop.backlight_power, 0.0,
        "finite rimlight must leave backlight at its absent-field default"
    );
    assert!((prop.grayscale_to_palette_scale - 0.7).abs() < 1e-6);
    assert!((prop.fresnel_power - 5.0).abs() < 1e-6);
    assert_eq!(stream.position(), data.len() as u64);
}


/// #1080 / FO4-D3-009 — pin the BSVER 130 (FO4) BGSM-stopcond boundary.
///
/// The `BSLightingShaderProperty` BGSM stopcond fires only at BSVER ≥ 155
/// (FO76+). For FO4 (BSVER=130), the full shader body MUST be parsed even
/// when `net.name` carries a `.bgsm` path — the stopcond mechanism didn't
/// exist in the FO4 era and dropping the body would silently lose every
/// FO4 wetness/subsurface/fresnel value.
///
/// This guard test fails if a future refactor lowers the stopcond
/// threshold from 155 to 130 (or removes the BSVER gate entirely):
/// the FO4 EnvironmentMap shader_type_data parse would short-circuit
/// before reading `env_map_scale = 0.75`.
#[test]
fn parse_bs_lighting_fo4_bgsm_name_does_not_stopcond() {
    // Header at BSVER=130 with strings[0] = a `.bgsm` path.
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 130,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("materials\\actors\\ironarmor.bgsm")],
        max_string_length: 32,
        num_groups: 0,
    };
    let data = build_bs_lighting_fo4_env_map();
    let mut stream = NifStream::new(&data, &header);
    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();

    // Full body MUST be parsed — the stopcond did NOT fire.
    assert!(
        !prop.material_reference,
        "FO4 BSVER=130 with .bgsm name must NOT trip the stopcond \
         (that mechanism is FO76+ only). See #1080 / FO4-D3-009."
    );
    // Wetness params present — the stopcond would skip these in FO76+.
    let w = prop.wetness.as_ref().expect("FO4 wetness must be parsed");
    assert!((w.spec_scale - 0.1).abs() < 1e-6);
    assert!((w.unknown_1 - 0.95).abs() < 1e-6);
    // Shader type 1 (EnvironmentMap) trailing data present.
    match prop.shader_type_data {
        ShaderTypeData::EnvironmentMap { env_map_scale } => {
            assert!(
                (env_map_scale - 0.75).abs() < 1e-6,
                "FO4 EnvironmentMap env_map_scale must round-trip even \
                 with a .bgsm name — see #1080."
            );
        }
        _ => panic!("expected EnvironmentMap shader_type_data"),
    }
    // Whole record consumed — no trailing bytes left, confirming the
    // parser walked the full FO4 BSLightingShaderProperty layout.
    assert_eq!(stream.position(), data.len() as u64);
}
