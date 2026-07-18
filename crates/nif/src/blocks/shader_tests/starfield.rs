//! Starfield era shader-property tests. Split from `shader_tests.rs` (#2056);
//! helpers live in the parent module.

use super::*;


/// #1510 / NIF-NEW-05 — nif.xml `#BS_F76# = (BSVER == 155)` ("Fallout 76
/// stream 155 only"): the `BSShaderType155` field, WetnessParams
/// `unknown_2`, and the Luminance / Translucency / texture-array tail are
/// FO76-ONLY. Starfield (BSVER 172) omits them. The #746/#747 `>= 155`
/// gates (via the #1279 `parse_fo76_plus` split) made Starfield read
/// them, over-reading every full-body `BSLightingShaderProperty` past
/// its block_size into the NIF footer — 1036 NiUnknown on the Starfield
/// corpus (0 → 1036 regression vs the a9c7bc9e baseline). This pins the
/// corrected Starfield body shape: the FO76-only fields stay at default.
#[test]
fn parse_bs_lighting_starfield_minimal_omits_fo76_only_tail() {
    let header = make_starfield_header(""); // empty name → full-body path
    let data = build_starfield_bs_lighting_minimal();
    let mut stream = NifStream::new(&data, &header);

    let prop = BSLightingShaderProperty::parse(&mut stream)
        .expect("Starfield BLSP full body must parse");
    assert_eq!(
        stream.position(),
        data.len() as u64,
        "Starfield body must consume exactly — no FO76-only tail reads",
    );
    let w = prop.wetness.as_ref().expect("wetness present (BSVER >= 130)");
    assert_eq!(
        w.unknown_2, 0.0,
        "unknown_2 is FO76-only (== 155); absent on Starfield",
    );
    assert!(prop.luminance.is_none(), "luminance is FO76-only");
    assert!(!prop.do_translucency);
    assert!(prop.translucency.is_none());
    assert!(prop.texture_arrays.is_empty());
    assert!(matches!(prop.shader_type_data, ShaderTypeData::None));
}


/// #1606 — Starfield full-body `BSLightingShaderProperty` carries a
/// trailing block (byte-audited as 38 B = 9× f32 + 2 B, constant across
/// the 26 LODMeshes instances) that the FO76+ parser doesn't decode and
/// nif.xml doesn't document. `parse_with_size` captures it opaquely up to
/// `block_size` so the stream is self-consistent (no +38 drift) and the
/// bytes survive for a future decoder.
#[test]
fn parse_bs_lighting_starfield_captures_trailing_tail() {
    let header = make_starfield_header(""); // empty name → full-body path
    let body = build_starfield_bs_lighting_minimal();
    // Real layout is 9 f32 + 2 B; pin an arbitrary-but-distinct 38 B so we
    // assert capture without asserting (unknown) semantics.
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
    assert!(p1.starfield_tail.is_empty(), "no block_size → no tail capture");

    // (b) parse_with_size with the exact body size (no trailing bytes).
    let mut s2 = NifStream::new(&body, &header);
    let p2 =
        BSLightingShaderProperty::parse_with_size(&mut s2, Some(body.len() as u32)).unwrap();
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

    let prop = BSLightingShaderProperty::parse(&mut stream)
        .expect("Starfield hash-path BLSP must stub");
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

    let prop = BSEffectShaderProperty::parse(&mut stream)
        .expect("Starfield hash-path BSEffect must stub");
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
    assert!(p1.starfield_tail.is_empty(), "no block_size → no tail capture");

    // (b) parse_with_size with the exact body size (no trailing bytes).
    let mut s2 = NifStream::new(&body, &header);
    let p2 =
        BSEffectShaderProperty::parse_with_size(&mut s2, Some(body.len() as u32)).unwrap();
    assert!(
        p2.starfield_tail.is_empty(),
        "consumed == block_size → empty tail",
    );
}
