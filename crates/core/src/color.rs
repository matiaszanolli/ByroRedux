//! sRGB ↔ linear color-space helpers.
//!
//! Gamebryo/Creation-era content stores colors in sRGB "monitor space"
//! (the D3D9 fixed-function pipeline never linearized them). The PBR
//! shader, cluster culling, and SVGF accumulate radiance in linear space,
//! so every authored color must be converted at parse time. Linearizing
//! in the shader is too late: by then the cluster budget and temporal
//! history have already consumed the wrong values. See RL-01 in
//! `docs/audits/AUDIT_RENDERER_2026-04-12c.md`.

/// Convert a single sRGB channel value in [0,1] to linear space using the
/// IEC 61966-2-1 piecewise curve (matches `GL_FRAMEBUFFER_SRGB` + the
/// `VK_FORMAT_*_SRGB` hardware sampler paths).
#[inline]
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linearize an RGB triplet authored in sRGB. Alpha channels (if any)
/// must be left untouched — they're opacity, not perceptual luminance.
#[inline]
pub fn srgb_rgb_to_linear(rgb: [f32; 3]) -> [f32; 3] {
    [
        srgb_to_linear(rgb[0]),
        srgb_to_linear(rgb[1]),
        srgb_to_linear(rgb[2]),
    ]
}

/// Convert an 8-bit sRGB channel (0..=255) directly to linear float.
/// Fused form of `c as f32 / 255.0` followed by [`srgb_to_linear`] — used
/// by every ESM/NIF call site that reads raw `u8` color bytes.
#[inline]
pub fn srgb_u8_to_linear(c: u8) -> f32 {
    srgb_to_linear(c as f32 / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints() {
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn midgray_matches_reference() {
        // sRGB 0.5 ≈ linear 0.2140 (canonical reference).
        let linear = srgb_to_linear(0.5);
        assert!((linear - 0.2140).abs() < 1e-3, "got {linear}");
    }

    #[test]
    fn toe_is_linear() {
        // Below the toe threshold, the conversion is the simple linear
        // division by 12.92.
        let v = 0.02;
        assert!((srgb_to_linear(v) - v / 12.92).abs() < 1e-6);
    }

    #[test]
    fn u8_fused_matches_split() {
        for c in [0u8, 1, 10, 64, 128, 200, 255] {
            let fused = srgb_u8_to_linear(c);
            let split = srgb_to_linear(c as f32 / 255.0);
            assert!((fused - split).abs() < 1e-6, "c={c}");
        }
    }

    #[test]
    fn warm_interior_light_example() {
        // Typical interior amber light authored as (255, 200, 120) in
        // sRGB-255 space — the canonical "2.3x dim" example from the
        // lighting audit.
        let srgb = [
            srgb_u8_to_linear(255),
            srgb_u8_to_linear(200),
            srgb_u8_to_linear(120),
        ];
        // Linear R ≈ 1.0, G ≈ 0.58, B ≈ 0.19. Before linearization the
        // values were (1.0, 0.784, 0.471) — the B channel nearly
        // 2.5× brighter than the true linear value.
        assert!((srgb[0] - 1.0).abs() < 1e-3);
        assert!((srgb[1] - 0.5775).abs() < 1e-2);
        assert!((srgb[2] - 0.1926).abs() < 1e-2);
    }
}
