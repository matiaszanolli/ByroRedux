//! Tests for `alpha_flag_tests` extracted from ../material.rs (refactor stage A).
//!
//! Same qualified path preserved (`alpha_flag_tests::FOO`).

//! Regression tests for issue #152 — NiAlphaProperty bit extraction.
//! Verify the cutout-vs-blend precedence and threshold scaling.
use super::*;
use crate::blocks::base::NiObjectNETData;

fn alpha_prop(flags: u16, threshold: u8) -> NiAlphaProperty {
    NiAlphaProperty {
        net: NiObjectNETData {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: crate::types::BlockRef::NULL,
        },
        flags,
        threshold,
    }
}

#[test]
fn alpha_blend_only_sets_blend() {
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x0001, 128));
    assert!(info.alpha_blend);
    assert!(!info.alpha_test);
    assert_eq!(info.alpha_threshold, 0.0);
}

#[test]
fn alpha_test_only_sets_test_and_scales_threshold() {
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x0200, 128));
    assert!(!info.alpha_blend);
    assert!(info.alpha_test);
    assert!((info.alpha_threshold - (128.0 / 255.0)).abs() < 1e-5);
}

#[test]
fn alpha_test_and_blend_prefers_test() {
    // Foliage with both bits set: alpha-test wins because the
    // discard + depth-write path sorts cleanly without back-to-front
    // pre-sort of the alpha-blend pipeline.
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x0201, 200));
    assert!(!info.alpha_blend, "alpha_blend should yield to alpha_test");
    assert!(info.alpha_test);
    assert!((info.alpha_threshold - (200.0 / 255.0)).abs() < 1e-5);
}

#[test]
fn neither_bit_leaves_defaults() {
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x0000, 255));
    assert!(!info.alpha_blend);
    assert!(!info.alpha_test);
    assert_eq!(info.alpha_threshold, 0.0);
}

#[test]
fn threshold_extremes_clamp_expected_range() {
    let mut info_min = MaterialInfo::default();
    apply_alpha_flags(&mut info_min, &alpha_prop(0x0200, 0));
    assert_eq!(info_min.alpha_threshold, 0.0);

    let mut info_max = MaterialInfo::default();
    apply_alpha_flags(&mut info_max, &alpha_prop(0x0200, 255));
    assert!((info_max.alpha_threshold - 1.0).abs() < 1e-5);
}

/// #263: alpha test function bits 10-12 are extracted.
#[test]
fn alpha_test_func_greaterequal_default() {
    // flags = 0x1A00: test enable (0x200) + GREATEREQUAL (6 << 10 = 0x1800)
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x1A00, 128));
    assert!(info.alpha_test);
    assert_eq!(info.alpha_test_func, 6); // GREATEREQUAL
}

#[test]
fn alpha_test_func_less() {
    // flags = 0x0600: test enable (0x200) + LESS (1 << 10 = 0x400)
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x0600, 64));
    assert!(info.alpha_test);
    assert_eq!(info.alpha_test_func, 1); // LESS
}

#[test]
fn alpha_test_func_always() {
    // flags = 0x0200: test enable (0x200) + ALWAYS (0 << 10 = 0x000)
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x0200, 128));
    assert!(info.alpha_test);
    assert_eq!(info.alpha_test_func, 0); // ALWAYS
}

#[test]
fn alpha_test_func_default_when_no_test() {
    // When alpha test is disabled, func should stay at default (6).
    let info = MaterialInfo::default();
    assert_eq!(info.alpha_test_func, 6); // GREATEREQUAL default
}
