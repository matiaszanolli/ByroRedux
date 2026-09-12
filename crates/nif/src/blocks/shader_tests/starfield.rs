//! Starfield era shader-property tests. Split from `shader_tests.rs` (#2056);
//! helpers live in the parent module.

use super::*;

/// Regression: #2616 / SF-D6-01. Real block 6 (bsver 173, block_size 166,
/// full `BSLightingShaderProperty` body) extracted verbatim from
/// `Starfield - LODMeshes.ba2`'s
/// `meshes\lod\generated\ships\discovery\shiplandingmarker_lod_3.nif` —
/// not a synthetic builder (see SF-D6-03: editing a synthetic fixture to
/// match wrong output proves nothing). Pre-fix (`shader_type` skipped,
/// `root_material_path` read unconditionally) this decodes to a NaN
/// emissive color, an unresolvable `texture_set_ref`, a zero U-scale, and
/// an sf1 CRC that isn't a member of the known `BSShaderCRC32` set.
#[test]
fn parse_bs_lighting_real_starfield_block_is_semantically_valid() {
    // real file's bsver
    let header = NifHeader {
        // Real block's NiObjectNETData.name is string-table index 6,
        // resolving to "" (full-body path, not a material-reference stub).
        strings: vec![Arc::from(""); 7],
        ..NifHeader::detached(NifVersion::V20_2_0_7, 12, 173)
    };
    // Byte-for-byte block 6 payload (166 bytes), extracted with
    // `cargo run -p byroredux-bsa --example ba2_extract_one` +
    // `cargo run -p byroredux-nif --example trace_block` against the
    // real archive — see this test's doc comment for the source path.
    #[rustfmt::skip]
    let data: [u8; 166] = [
        0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff,
        0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0xad, 0xc2, 0xc5, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0xff, 0xff, 0xff, 0xff,
        0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
        0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
        0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0xbf,
        0x00, 0x00, 0xc8, 0x42, 0x00, 0x00, 0x58, 0x41, 0x00, 0x00, 0x00, 0x40,
        0x00, 0x00, 0x40, 0x40, 0xcd, 0xcc, 0xcc, 0x3d, 0x00, 0x00, 0x00, 0x00,
        0x0a, 0xd7, 0xa3, 0x3c, 0xb1, 0xbf, 0xec, 0x3c, 0x55, 0xa4, 0xc2, 0x3d,
        0x1b, 0x4c, 0x43, 0x3b, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00,
    ];

    let mut stream = NifStream::new(&data, &header);
    let prop = BSLightingShaderProperty::parse_with_size(&mut stream, Some(data.len() as u32))
        .expect("real Starfield BSLightingShaderProperty block must parse");

    assert_eq!(
        stream.position(),
        data.len() as u64,
        "must consume exactly the real block's byte count — no drift",
    );
    assert!(
        !prop.material_reference,
        "empty name (string idx 6 → \"\") must take the full-body path"
    );

    // Corrected alignment reads shader_type unconditionally.
    assert_eq!(prop.shader_type, 0);

    // Finite, non-negative emissive — pre-fix this was NaN (decoded from
    // texture_set_ref's 0xFFFFFFFF NULL sentinel one word early).
    for c in prop.emissive_color {
        assert!(
            c.is_finite() && c >= 0.0,
            "emissive component {c} must be finite and non-negative"
        );
    }
    assert_eq!(prop.emissive_color, [1.0, 1.0, 1.0]);

    // A well-defined (NULL) texture_set_ref — pre-fix this decoded to an
    // arbitrary non-null, unresolvable index (1065353216 on this exact
    // block).
    assert!(
        prop.texture_set_ref.is_null(),
        "texture_set_ref must resolve to a well-defined ref, got {:?}",
        prop.texture_set_ref
    );

    // Non-zero U-scale — pre-fix this block's uv_scale.x decoded to 0.0.
    assert!(
        prop.uv_scale[0] > 0.0,
        "uv_scale.x must be non-zero, got {}",
        prop.uv_scale[0]
    );
    assert_eq!(prop.uv_scale, [1.0, 1.0]);

    // sf1_crcs must be members of the known BSShaderCRC32 set — pre-fix
    // this decoded to a value with no membership in that set.
    assert_eq!(
        prop.sf1_crcs,
        vec![crate::shader_flags::bs_shader_crc32::VERTEX_COLORS],
        "sf1 CRC must decode to the real, named VERTEX_COLORS flag",
    );
}

/// #1510 / NIF-NEW-05 — nif.xml `#BS_F76# = (BSVER == 155)` ("Fallout 76
/// stream 155 only"): the `BSShaderType155` field, WetnessParams
/// `unknown_2`, and the Translucency / texture-array tail are FO76-ONLY.
/// Starfield (BSVER 172) omits them. The #746/#747 `>= 155` gates (via
/// the #1279 `parse_fo76_plus` split) made Starfield read them,
/// over-reading every full-body `BSLightingShaderProperty` past its
/// block_size into the NIF footer — 1036 NiUnknown on the Starfield
/// corpus (0 → 1036 regression vs the a9c7bc9e baseline). This pins the
/// corrected Starfield body shape: the FO76-only fields stay at default.
///
/// #2622 / SF-D6-02 corrected this test's own premise: `BSSPLuminanceParams`
/// is NOT FO76-only — it's present on Starfield too, immediately after a
/// 4-field (not 6-field) wetness block. See `read_wetness_block`'s doc.
#[test]
fn parse_bs_lighting_starfield_minimal_omits_fo76_only_tail() {
    let header = make_starfield_header(""); // empty name → full-body path
    let data = build_starfield_bs_lighting_minimal();
    let mut stream = NifStream::new(&data, &header);

    let prop =
        BSLightingShaderProperty::parse(&mut stream).expect("Starfield BLSP full body must parse");
    assert_eq!(
        stream.position(),
        data.len() as u64,
        "Starfield body must consume exactly — no FO76-only tail reads",
    );
    let w = prop
        .wetness
        .as_ref()
        .expect("wetness present (BSVER >= 130)");
    assert_eq!(
        w.unknown_2, 0.0,
        "unknown_2 is FO76-only (== 155); absent on Starfield",
    );
    assert_eq!(
        (w.metalness, w.unknown_1),
        (0.0, 0.0),
        "metalness/unknown_1 are FO76-only (== 155); their wire position \
         on Starfield is BSSPLuminanceParams instead, not wetness",
    );
    let lum = prop
        .luminance
        .as_ref()
        .expect("BSSPLuminanceParams IS present on Starfield (#2622 / SF-D6-02)");
    assert_eq!(
        (
            lum.lum_emittance,
            lum.exposure_offset,
            lum.final_exposure_min,
            lum.final_exposure_max
        ),
        (100.0, 13.5, 2.0, 3.0),
        "corpus-verified documented BSSPLuminanceParams defaults",
    );
    assert!(!prop.do_translucency);
    assert!(prop.translucency.is_none());
    assert!(prop.texture_arrays.is_empty());
    assert!(matches!(prop.shader_type_data, ShaderTypeData::None));
}

/// Regression for #4149 — `parse_shader_type_data_fo76`'s shader_type 4/5
/// (Skin/Hair Tint) arms read 16/12 extra bytes with no `bsver` gate,
/// contradicting this exact function's own comment three lines above the
/// call site that Starfield's block "ends after the luminance quad." Pins
/// the corrected gate: on Starfield (unlike FO76, where these arms are
/// still exercised — see `shader_tests::fo76`), a Skin/Hair-Tint
/// `shader_type` must not read past the luminance quad, and
/// `shader_type_data` falls back to `None` rather than decoding bytes
/// that were never authored on the wire.
///
/// No real Starfield character/hair NIF sample with `shader_type` 4 or 5
/// was available to corpus-verify this session (see the fix's own doc
/// comment) — this fixture pins the code's own pre-existing, corpus-backed
/// "ends after the luminance quad" claim (#2622) rather than a fresh
/// positive/negative corpus finding for these two shader types
/// specifically.
#[test]
fn parse_bs_lighting_starfield_skin_and_hair_tint_do_not_overrun_the_block() {
    for shader_type in [4u32, 5u32] {
        let header = make_starfield_header("");
        let mut data = build_starfield_bs_lighting_minimal();
        // Overwrite the shader_type field (offset 12: name(4) + extra(4) +
        // controller_ref(4) precede it) to 4 (Skin Tint) / 5 (Hair Tint).
        data[12..16].copy_from_slice(&shader_type.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap_or_else(|e| {
            panic!("Starfield BLSP with shader_type={shader_type} must parse: {e}")
        });
        assert_eq!(
            stream.position(),
            data.len() as u64,
            "shader_type={shader_type} must not read past the luminance quad \
             into the next block's data",
        );
        assert_eq!(prop.shader_type, shader_type);
        assert!(
            matches!(prop.shader_type_data, ShaderTypeData::None),
            "shader_type={shader_type}: no Skin/Hair-Tint tail was authored on \
             the wire for this fixture, so shader_type_data must stay None, \
             got {:?}",
            prop.shader_type_data,
        );
    }
}

/// #1606 — Starfield full-body `BSLightingShaderProperty` carries a
/// trailing block (originally byte-audited as 38 B, constant across the
/// 26 LODMeshes instances) that the FO76+ parser doesn't decode and
/// nif.xml doesn't document. `parse_with_size` captures it opaquely up to
/// `block_size` so the stream is self-consistent (no drift) and the bytes
/// survive for a future decoder.
///
/// #2622 / SF-D6-02 reclaimed 16 of those 38 bytes as `BSSPLuminanceParams`
/// (see `parse_fo76_plus` / `read_wetness_block`); the real remaining
/// opaque tail is corpus-verified at 30 B today (4,417 real
/// `Starfield - Meshes01.ba2` blocks, all constant). This test still pins
/// an arbitrary-but-distinct 38 B tail — it asserts the CAPTURE MECHANISM
/// (whatever trailing bytes exist up to `block_size` are preserved
/// opaquely), not the real-world tail's exact current size.
#[test]
fn parse_bs_lighting_starfield_captures_trailing_tail() {
    let header = make_starfield_header(""); // empty name → full-body path
    let body = build_starfield_bs_lighting_minimal();
    // Arbitrary-but-distinct 38 B (not the real tail's current size — see
    // the fn doc above) so we assert capture without asserting (unknown)
    // semantics.
    let tail: Vec<u8> = (0u8..38).collect();
    let mut data = body.clone();
    data.extend_from_slice(&tail);
    let block_size = data.len() as u32;

    let mut stream = NifStream::new(&data, &header);
    let prop = BSLightingShaderProperty::parse_with_size(&mut stream, Some(block_size))
        .expect("Starfield full body + tail must parse");
    assert_eq!(
        prop.starfield_tail, tail,
        "the trailing block_size bytes are captured opaquely",
    );
    assert_eq!(
        stream.position(),
        data.len() as u64,
        "tail capture consumes exactly to block_size — no drift",
    );
    // Body fields still decode unchanged.
    assert!(prop.wetness.is_some());
    assert!(matches!(prop.shader_type_data, ShaderTypeData::None));
}

/// #1606 — the tail is captured ONLY when a `block_size` is supplied and
/// there are trailing bytes. The legacy `parse(stream)` entry (no size)
/// and a block that consumed exactly to its boundary both yield an empty
/// tail — drift recovery continues to handle the no-size case as before.
#[test]
fn parse_bs_lighting_starfield_tail_empty_without_size_or_drift() {
    let header = make_starfield_header("");
    let body = build_starfield_bs_lighting_minimal();

    // (a) legacy parse(stream): no block_size → no tail capture.
    let mut s1 = NifStream::new(&body, &header);
    let p1 = BSLightingShaderProperty::parse(&mut s1).unwrap();
    assert!(
        p1.starfield_tail.is_empty(),
        "no block_size → no tail capture"
    );

    // (b) parse_with_size with the exact body size (no trailing bytes).
    let mut s2 = NifStream::new(&body, &header);
    let p2 = BSLightingShaderProperty::parse_with_size(&mut s2, Some(body.len() as u32)).unwrap();
    assert!(
        p2.starfield_tail.is_empty(),
        "consumed == block_size → empty tail",
    );
}

/// #1510 — Starfield material references are content-hash paths with NO
/// `.mat`/`.bgsm` suffix, so `is_material_reference` misses them. For
/// BSVER >= STARFIELD a non-empty Name means a reference (full bodies
/// carry an empty name), so the parser must return the stub and let
/// block_size skip the rest — NOT run the full-body path into the
/// 12-byte stub. That #749 mismatch produced 171 of the 1036 NiUnknown;
/// `!name.is_empty()` (the a9c7bc9e baseline gate) fixes it.
#[test]
fn parse_bs_lighting_starfield_hashpath_name_stubs() {
    // Header string 0 is a content-hash path (two hex segments, no
    // suffix) — `is_material_reference` would reject it.
    let header = make_starfield_header("8f3a91c4\\b27e5d06");
    let data = build_starfield_bs_lighting_minimal(); // name idx 0 → the hash-path
    let mut stream = NifStream::new(&data, &header);

    let prop =
        BSLightingShaderProperty::parse(&mut stream).expect("Starfield hash-path BLSP must stub");
    assert!(
        prop.material_reference,
        "a non-empty (hash-path) Starfield name must take the stub path",
    );
    assert_eq!(
        stream.position(),
        12,
        "stub consumes only the NiObjectNET base (name + extra + controller)",
    );
}

/// #1721 — sibling of `parse_bs_lighting_starfield_hashpath_name_stubs`.
/// Starfield material references are content-hash paths with NO
/// `.mat`/`.bgsm` suffix, so `is_material_reference` misses them. For
/// BSVER >= STARFIELD a non-empty Name means a reference (full bodies
/// carry an empty name), so `BSEffectShaderProperty::parse` must return
/// the 12-byte stub and let block_size skip the rest — NOT run the
/// full-body path off bytes the block doesn't carry. Pre-#1721 the
/// effect-shader parser kept the suffix-aware `is_material_reference`
/// gate (the #1510 fix only reached the BSLightingShaderProperty
/// sibling), so a hash-path effect shader over-read garbage
/// source-texture / base-color / falloff fields into its material.
#[test]
fn parse_bs_effect_starfield_hashpath_name_stubs() {
    // Header string 0 is a content-hash path (two hex segments, no
    // suffix) — `is_material_reference` would reject it.
    let header = make_starfield_header("8f3a91c4\\b27e5d06");
    // Only the NiObjectNET base is present; a full body would follow on
    // disk for a non-reference, but a hash-path name must stub before it.
    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes()); // name idx 0 → the hash-path
    data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count = 0
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref = -1
    let mut stream = NifStream::new(&data, &header);

    let prop =
        BSEffectShaderProperty::parse(&mut stream).expect("Starfield hash-path BSEffect must stub");
    assert!(
        prop.material_reference,
        "a non-empty (hash-path) Starfield name must take the stub path",
    );
    assert_eq!(
        stream.position(),
        12,
        "stub consumes only the NiObjectNET base (name + extra + controller)",
    );
}

/// #1881 — Starfield full-body `BSEffectShaderProperty` carries a trailing
/// tail (byte-audited as a constant +32 B across 166 LODMeshes/MeshesPatch
/// instances) that the FO76+ parser doesn't decode and nif.xml doesn't
/// document — the missed sibling of #1606's `BSLightingShaderProperty` fix.
/// `parse_with_size` captures it opaquely up to `block_size` so the stream
/// stays self-consistent (no +32 drift) and the bytes survive for a future
/// decoder.
#[test]
fn parse_bs_effect_starfield_captures_trailing_tail() {
    let header = make_starfield_header(""); // empty name → full-body path
    let body = build_starfield_bs_effect_minimal();
    // Sanity: the fixture body parses and consumes exactly (no drift) on its own.
    {
        let mut s = NifStream::new(&body, &header);
        let p = BSEffectShaderProperty::parse(&mut s).expect("fixture body must parse");
        assert!(!p.material_reference, "empty name → full body, not stub");
        assert_eq!(
            s.position(),
            body.len() as u64,
            "fixture body must consume exactly — bad fixture otherwise",
        );
    }
    // Arbitrary-but-distinct 32 B tail; assert capture without asserting semantics.
    let tail: Vec<u8> = (0u8..32).collect();
    let mut data = body.clone();
    data.extend_from_slice(&tail);
    let block_size = data.len() as u32;

    let mut stream = NifStream::new(&data, &header);
    let prop = BSEffectShaderProperty::parse_with_size(&mut stream, Some(block_size))
        .expect("Starfield full body + tail must parse");
    assert_eq!(
        prop.starfield_tail, tail,
        "the trailing block_size bytes are captured opaquely",
    );
    assert_eq!(
        stream.position(),
        data.len() as u64,
        "tail capture consumes exactly to block_size — no drift",
    );
}

/// #1881 — the tail is captured ONLY with a `block_size` and trailing bytes.
/// The legacy `parse(stream)` (no size) and a block consumed exactly to its
/// boundary both yield an empty tail — drift recovery handles the no-size case
/// as before. Mirrors the BLSP `..._tail_empty_without_size_or_drift` guard.
#[test]
fn parse_bs_effect_starfield_tail_empty_without_size_or_drift() {
    let header = make_starfield_header("");
    let body = build_starfield_bs_effect_minimal();

    // (a) legacy parse(stream): no block_size → no tail capture.
    let mut s1 = NifStream::new(&body, &header);
    let p1 = BSEffectShaderProperty::parse(&mut s1).unwrap();
    assert!(
        p1.starfield_tail.is_empty(),
        "no block_size → no tail capture"
    );

    // (b) parse_with_size with the exact body size (no trailing bytes).
    let mut s2 = NifStream::new(&body, &header);
    let p2 = BSEffectShaderProperty::parse_with_size(&mut s2, Some(body.len() as u32)).unwrap();
    assert!(
        p2.starfield_tail.is_empty(),
        "consumed == block_size → empty tail",
    );
}

/// #2625 (SF-D6-04) — the tail capture must REPORT its length, because
/// capturing it destroys the drift signal that would otherwise report it.
///
/// `read_starfield_tail` consumes `block_size - consumed` inside the block
/// parser, so `parse_nif`'s later `consumed != block_size` comparison always
/// finds them equal and `drift_histogram` records nothing — no matter how much
/// of the block the parser failed to understand. Measured on four Starfield
/// archives: shader-block drift `{}` (empty) while tail lengths were bimodal
/// `{38: 1868, 42: 11}`. Eleven blocks disagreeing with 1868 others by four
/// bytes is exactly what a drift histogram exists to surface, and it was
/// invisible — which is why SF-D6-01 went undetected by telemetry nominally
/// watching for it.
#[test]
fn captured_starfield_tail_is_reported_through_the_ni_object_surface() {
    use crate::blocks::NiObject;

    let header = make_starfield_header(""); // empty name → full-body path
    let body = build_starfield_bs_lighting_minimal();
    let tail: Vec<u8> = (0u8..38).collect();
    let mut data = body.clone();
    data.extend_from_slice(&tail);
    let block_size = data.len() as u32;

    let mut stream = NifStream::new(&data, &header);
    let prop = BSLightingShaderProperty::parse_with_size(&mut stream, Some(block_size))
        .expect("Starfield full body + tail must parse");

    // The drift this block contributes is zero BY CONSTRUCTION — that is the
    // blind spot, restated as an assertion so the premise stays true.
    assert_eq!(
        stream.position(),
        data.len() as u64,
        "the tail read consumes exactly to block_size, so signed drift is 0 \
         and `drift_histogram` cannot see this block (#2625)"
    );

    // The length is what survives that, and it must reach `parse_nif` through
    // the trait rather than a downcast to a hardcoded list of types.
    let reported = (&prop as &dyn NiObject).opaque_tail_len();
    assert_eq!(
        reported,
        Some(tail.len()),
        "BSLightingShaderProperty must report its captured tail length so \
         `opaque_tail_histogram` can bucket it (#2625)"
    );

    // A block that consumes exactly to its boundary reports `Some(0)`, not
    // `None`: it HAS a tail slot and left nothing over. The distinction is the
    // difference between "no anomaly" and "this type is not instrumented".
    let mut exact = NifStream::new(&body, &header);
    let exact_prop = BSLightingShaderProperty::parse_with_size(&mut exact, Some(body.len() as u32))
        .expect("body with no trailing bytes must parse");
    assert_eq!(
        (&exact_prop as &dyn NiObject).opaque_tail_len(),
        Some(0),
        "an empty tail is `Some(0)` — `None` means the type has no capture at all"
    );
}

/// The reporting is only useful if `parse_nif` actually calls it and if every
/// tail-capturing type overrides it. Both are source-pinned, because the
/// alternative — a hardcoded list of types in `lib.rs` — is precisely the
/// shape that goes stale when a fifth tail-capturing block is added.
#[test]
fn every_tail_capturing_block_reports_it_and_parse_nif_records_it() {
    // (a) `parse_nif` must consult the trait and bucket the result.
    const LIB_RS: &str = include_str!("../../lib.rs");
    for needle in [
        "block.opaque_tail_len()",
        "bump_hist(&mut opaque_tail_histogram, type_name, tail_len)",
    ] {
        assert!(
            LIB_RS.contains(needle),
            "parse_nif no longer records opaque tails (`{needle}` missing) — \
             the histogram is populated nowhere and reads as empty rather than \
             as absent (#2625)"
        );
    }

    // (b) Every struct declaring a `starfield_tail` field must override
    // `opaque_tail_len`. A new tail-capturing block that skips the override
    // silently reintroduces the blind spot for its own type.
    for (label, src) in [
        ("blocks/shader.rs", include_str!("../shader.rs")),
        ("blocks/node.rs", include_str!("../node.rs")),
    ] {
        let declarations = src.matches("starfield_tail: Vec<u8>,").count();
        let overrides = src
            .matches("fn opaque_tail_len(&self) -> Option<usize>")
            .count();
        assert_eq!(
            overrides, declarations,
            "{label} declares {declarations} `starfield_tail` field(s) but has \
             {overrides} `opaque_tail_len` override(s) — every capturing type \
             must report, or its under-reads stay invisible to both histograms \
             (#2625)"
        );
    }
}
