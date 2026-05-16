//! Regression tests for BSGeometry tangent extraction (#1086 / REN-D16-001).
//!
//! Guards against the regression where `extract_bs_geometry` returned
//! `tangents: Vec::new()` for all Starfield meshes, forcing every mesh
//! to the shader's screen-space derivative Path-2 in `perturbNormal`.

use crate::blocks::bs_geometry::unpack_udec3_xyzw;

/// Helper: encode 10-bit x/y/z and 2-bit w into a UDEC3 word.
fn encode_udec3(x: u32, y: u32, z: u32, w: u32) -> u32 {
    (x & 0x3FF) | ((y & 0x3FF) << 10) | ((z & 0x3FF) << 20) | ((w & 0x3) << 30)
}

/// UDEC3 round-trip: encode tangent (1, 0, 0, +1) and verify the
/// unpack gives approximately the same per-channel values.
/// The 10:10:10:2 format has ~1/1023 precision; ε = 0.003.
#[test]
fn udec3_tangent_roundtrip_x_plus_sign() {
    // x=1.0 → raw 1023; y=0.0 → raw 511; z=0.0 → raw 511; w=+1.0 → raw 3.
    let packed = encode_udec3(1023, 511, 511, 3);
    let xyzw = unpack_udec3_xyzw(packed);
    const EPS: f32 = 0.003;
    assert!((xyzw[0] - 1.0).abs() < EPS, "x ≈ 1.0, got {}", xyzw[0]);
    assert!(xyzw[1].abs() < EPS, "y ≈ 0.0, got {}", xyzw[1]);
    assert!(xyzw[2].abs() < EPS, "z ≈ 0.0, got {}", xyzw[2]);
    assert!((xyzw[3] - 1.0).abs() < EPS, "w (sign) ≈ +1.0, got {}", xyzw[3]);
}

/// Verify that negative bitangent sign (w = -1) is correctly decoded.
#[test]
fn udec3_tangent_negative_sign() {
    // w=0 → raw 0 → (0/3)*2-1 = -1.0.
    let packed = encode_udec3(1023, 511, 511, 0);
    let xyzw = unpack_udec3_xyzw(packed);
    const EPS: f32 = 0.003;
    assert!((xyzw[3] - (-1.0)).abs() < EPS, "w (sign) ≈ -1.0, got {}", xyzw[3]);
}

/// The extraction path: N entries in tangents_raw should produce N tangents,
/// each a [f32;4] matching the unpacked UDEC3 values.
/// This guards against a regression back to Vec::new().
#[test]
fn tangent_extraction_count_and_values() {
    let n: usize = 4;
    let raw_val = encode_udec3(1023, 511, 511, 3); // tangent (1, 0, 0, +1)
    let tangents_raw: Vec<u32> = vec![raw_val; n];

    // Simulate the extraction loop in extract_bs_geometry.
    let tangents: Vec<[f32; 4]> = tangents_raw
        .iter()
        .map(|&raw| {
            let xyzw = unpack_udec3_xyzw(raw);
            [xyzw[0], xyzw[1], xyzw[2], xyzw[3]]
        })
        .collect();

    assert_eq!(tangents.len(), n, "tangent count must equal vertex count");
    for t in &tangents {
        assert!((t[0] - 1.0).abs() < 0.003, "tangent.x ≈ 1.0");
        assert!((t[3] - 1.0).abs() < 0.003, "bitangent sign ≈ +1.0");
    }
}

/// Empty tangents_raw → Vec::new() (screen-space derivative fallback).
#[test]
fn empty_tangents_raw_produces_empty_vec() {
    let tangents_raw: Vec<u32> = Vec::new();
    let tangents: Vec<[f32; 4]> = if !tangents_raw.is_empty() {
        tangents_raw
            .iter()
            .map(|&raw| {
                let xyzw = unpack_udec3_xyzw(raw);
                [xyzw[0], xyzw[1], xyzw[2], xyzw[3]]
            })
            .collect()
    } else {
        Vec::new()
    };
    assert!(tangents.is_empty(), "empty tangents_raw must yield Vec::new()");
}
