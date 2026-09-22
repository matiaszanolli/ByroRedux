//! Host mirrors of the display transforms in `presentation.frag`.
//!
//! The GLSL is authoritative (it renders); these mirrors exist so the
//! transforms' *behaviour* — monotonicity, grey balance, output bounds — is
//! pinned by `cargo test` instead of only by screenshots, the same contract
//! `bloom.rs`'s `bright_pass_scale` mirror holds for the bloom knee. A true
//! cross-validation would require executing the GLSL; like that precedent,
//! the two are tied together by source-text pins
//! ([`shaders_pin_the_mirror_constants`]) plus the behavioural tests below.
//!
//! Two operators:
//!
//! - **ACES** (default) — the Narkowicz 2015 rational fit, unchanged since it
//!   landed in the presentation pass. Scalar per-channel; cheap.
//! - **AgX** — the "Minimal AgX" port of Troy Sobotka's display transform by
//!   Benjamin Wrensch (IOLITE engine, March 2023), MIT licensed. Chosen for
//!   Stage 1 of the cinematic-rendering plan (`RENDERING-PLAN.md`): AgX's
//!   highlight desaturation avoids the hue skew ACES exhibits on saturated
//!   brights. Default look, no CDL variant yet.
//!
//! Both output **linear** display-light: the swapchain is `B8G8R8A8_SRGB`,
//! so the hardware applies the sRGB OETF on write. AgX's reference EOTF
//! (`pow(x, 2.2)`, per the fitting-display convention of the original) is
//! therefore kept — it returns the signal to linear, it does not encode it.

/// Push-constant operator ids, shared with `presentation.frag`'s
/// `params.tonemapOp` (uint lane, #3578 idiom). #4584 — declared in
/// `shader_constants_data.rs` (the single source of truth) so build.rs
/// emits them into the generated `shader_constants.glsl`, and the shader
/// compares against `TONEMAP_OP_AGX` instead of a hand-typed literal.
pub use crate::shader_constants::{TONEMAP_OP_ACES, TONEMAP_OP_AGX};

/// Display-transform selection. `RendererConfig` carries this (an `Eq` enum,
/// not a float — see that struct's docs for why), the console can flip it
/// live, and `PresentationFrame` ships `shader_value()` to the fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TonemapOp {
    /// Narkowicz ACES fit — the historical default.
    #[default]
    Aces,
    /// Minimal AgX (Wrensch 2023, MIT).
    Agx,
}

impl TonemapOp {
    pub fn shader_value(self) -> u32 {
        match self {
            TonemapOp::Aces => TONEMAP_OP_ACES,
            TonemapOp::Agx => TONEMAP_OP_AGX,
        }
    }
}

/// Mirror of `aces()` in `presentation.frag` — Narkowicz 2015,
/// "Filmic Tonemapping Operators”.
pub fn aces(x: [f32; 3]) -> [f32; 3] {
    const A: f32 = 2.51;
    const B: f32 = 0.03;
    const C: f32 = 2.43;
    const D: f32 = 0.59;
    const E: f32 = 0.14;
    let channel = |x: f32| ((x * (A * x + B)) / (x * (C * x + D) + E)).clamp(0.0, 1.0);
    [channel(x[0]), channel(x[1]), channel(x[2])]
}

/// `v * m` with `m` given in GLSL `mat3(...)` column order — the form the
/// minimal-AgX gist writes its matrices in, so the tables below can be
/// transcribed verbatim.
fn row_vec_times_mat(v: [f32; 3], cols: [[f32; 3]; 3]) -> [f32; 3] {
    let mut out = [0.0; 3];
    for (i, item) in out.iter_mut().enumerate() {
        *item = v[0] * cols[0][i] + v[1] * cols[1][i] + v[2] * cols[2][i];
    }
    out
}

/// AgX working-space transform (gist `agx_mat`, columns verbatim).
// Transcription fidelity beats the precision lint: these are the reference's
// own digits, and the GLSL pin test asserts the same tables in the shader.
#[allow(clippy::excessive_precision)]
const AGX_MAT: [[f32; 3]; 3] = [
    [0.842_479_06, 0.042_328_24, 0.042_375_65],
    [0.078_433_6, 0.878_468_64, 0.078_433_6],
    [0.079_223_75, 0.079_166_13, 0.879_142_97],
];

/// Inverse of [`AGX_MAT`] (gist `agx_inv_mat`, columns verbatim).
#[allow(clippy::excessive_precision)]
const AGX_INV_MAT: [[f32; 3]; 3] = [
    [1.196_879, -0.052_896_85, -0.052_971_64],
    [-0.098_020_88, 1.151_903_1, -0.098_043_45],
    [-0.099_029_74, -0.098_961_18, 1.151_073_7],
];

/// AgX log-space window (gist constants): the EV range the sigmoid's input
/// is normalized to.
const AGX_MIN_EV: f32 = -12.473_93;
const AGX_MAX_EV: f32 = 4.026_069;

/// Mirror of `agxDefaultContrastApprox()` — the polynomial fit of AgX's
/// contrast S-curve. Coefficients verbatim from the gist.
fn agx_contrast_approx(x: f32) -> f32 {
    let x2 = x * x;
    let x4 = x2 * x2;
    15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x
        - 0.002_32
}

/// Mirror of the minimal AgX chain in `presentation.frag`: inset → clamp →
/// log2 → EV window → contrast approx → outset → reference EOTF.
///
/// The input clamp to `[0, 1]` matches the GLSL; the final clamp exists on
/// the Rust side as well because the outset matrix can produce small
/// negatives that `powf(2.2)` would turn into NaN.
pub fn agx(val: [f32; 3]) -> [f32; 3] {
    let mut v = row_vec_times_mat(val, AGX_MAT);
    for item in &mut v {
        *item = item.clamp(0.0, 1.0);
    }
    let v = [v[0].log2(), v[1].log2(), v[2].log2()];
    let window = AGX_MAX_EV - AGX_MIN_EV;
    let mut v = [
        agx_contrast_approx((v[0] - AGX_MIN_EV) / window),
        agx_contrast_approx((v[1] - AGX_MIN_EV) / window),
        agx_contrast_approx((v[2] - AGX_MIN_EV) / window),
    ];
    v = row_vec_times_mat(v, AGX_INV_MAT);
    [
        v[0].max(0.0).powf(2.2).clamp(0.0, 1.0),
        v[1].max(0.0).powf(2.2).clamp(0.0, 1.0),
        v[2].max(0.0).powf(2.2).clamp(0.0, 1.0),
    ]
}

/// Dispatch to the selected operator, mirroring `presentation.frag`'s
/// `tonemap()` switch.
pub fn tonemap(op: TonemapOp, x: [f32; 3]) -> [f32; 3] {
    match op {
        TonemapOp::Aces => aces(x),
        TonemapOp::Agx => agx(x),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grey in must stay grey out for both operators — a channel skew here
    /// would tint every neutral surface in the game. Tolerance is
    /// reference-informed: ACES is exactly grey-preserving (per-channel
    /// scalar fit), while the minimal-AgX tables are rounded to ~7 digits
    /// and its rows do not sum to exactly 1, leaving a ~2e-4 channel spread
    /// on neutral input. That spread is the reference's own behaviour, not a
    /// transcription error — the constant pin below guards the latter.
    #[test]
    fn grey_input_stays_grey_balanced() {
        for grey in [0.01, 0.18, 0.5, 1.0, 2.0, 8.0, 64.0] {
            for op in [TonemapOp::Aces, TonemapOp::Agx] {
                let out = tonemap(op, [grey; 3]);
                let spread = out.iter().fold(0.0f32, |a, c| a.max(*c))
                    - out.iter().fold(f32::MAX, |a, c| a.min(*c));
                assert!(
                    spread < 5.0e-4,
                    "{op:?} broke grey balance at input {grey}: {out:?} (spread {spread})"
                );
            }
        }
    }

    /// Monotonicity along the grey ramp — the property the plan's oracle-L6
    /// rung is specified on. ACES is strictly monotonic (per-channel scalar
    /// fit). AgX is monotonic only within a small band: its outset matrix
    /// carries negative off-diagonals and rows not summing to exactly 1, so
    /// the channels couple and a strictly-grey ramp can microscopically dip
    /// (observed 2.5e-6 at the clamp boundary). The band, not strictness, is
    /// the reference's contract.
    #[test]
    fn grey_ramp_is_monotonic() {
        for op in [TonemapOp::Aces, TonemapOp::Agx] {
            let band = match op {
                TonemapOp::Aces => 0.0,
                TonemapOp::Agx => 1.0e-4,
            };
            let mut prev = [f32::MIN; 3];
            let mut x = 0.0f32;
            while x <= 128.0 {
                let out = tonemap(op, [x; 3]);
                for c in 0..3 {
                    assert!(
                        out[c] + band >= prev[c],
                        "{op:?} is non-monotonic at input {x}: {out:?} after {prev:?}"
                    );
                }
                prev = out;
                x += 0.05;
            }
        }
    }

    /// Per-channel monotonicity on saturated primaries. ACES: strict. AgX:
    /// its inset table clamps at 1.0 (inputs above ~1.19 pin the channel)
    /// and the negative outset off-diagonals pull a driven channel DOWN as
    /// the other channels' sigmoid outputs rise (hue coupling) — observed
    /// dips of ~0.01 for red at 1.25→1.5. Both are reference behaviour; the
    /// gate is that a channel never *inverts* materially while its own
    /// input grows.
    #[test]
    fn saturated_channels_are_monotonic() {
        for op in [TonemapOp::Aces, TonemapOp::Agx] {
            let band = match op {
                TonemapOp::Aces => 0.0,
                TonemapOp::Agx => 0.02,
            };
            for channel in 0..3 {
                let mut prev = f32::MIN;
                let mut x = 0.0f32;
                while x <= 64.0 {
                    let mut input = [0.0f32; 3];
                    input[channel] = x;
                    let out = tonemap(op, input)[channel];
                    assert!(
                        out + band >= prev,
                        "{op:?} channel {channel} non-monotonic at {x}: {out} after {prev}"
                    );
                    prev = out;
                    x += 0.25;
                }
            }
        }
    }

    /// Output bounds. Black must map to black (AgX's polynomial has a
    /// negative constant term, so this pins the clamps); everything must
    /// land in [0, 1] including pathological HDR inputs.
    #[test]
    fn outputs_are_bounded_and_black_maps_to_black() {
        for op in [TonemapOp::Aces, TonemapOp::Agx] {
            let black = tonemap(op, [0.0; 3]);
            assert!(
                black.iter().all(|c| *c < 1.0e-4),
                "{op:?} must map black to black, got {black:?}"
            );
            for x in [0.0, 0.18, 1.0, 10.0, 1000.0, 1.0e6] {
                let out = tonemap(op, [x, 0.5, 4.0]);
                assert!(
                    out.iter().all(|c| (0.0..=1.0).contains(c)),
                    "{op:?} out of [0,1] at input {x}: {out:?}"
                );
            }
        }
    }

    /// AgX's headline property vs ACES: saturated brights desaturate toward
    /// white rather than skewing hue. A pure red at very high input must be
    /// *less* saturated after AgX than after ACES.
    #[test]
    fn agx_desaturates_bright_saturated_input_more_than_aces() {
        let input = [64.0, 0.0, 0.0];
        let saturation = |v: [f32; 3]| {
            let mx = v[0].max(v[1]).max(v[2]);
            let mn = v[0].min(v[1]).min(v[2]);
            (mx - mn) / mx.max(1.0e-6)
        };
        assert!(
            saturation(agx(input)) < saturation(aces(input)),
            "AgX should desaturate bright reds harder than ACES: \
             agx {:?} vs aces {:?}",
            agx(input),
            aces(input)
        );
    }

    /// The mirror must track the shader. The GLSL tables and coefficients
    /// are transcribed verbatim, so each load-bearing constant is asserted
    /// present in `presentation.frag` — editing one side without the other
    /// trips this, mirroring `bloom.rs`'s
    /// `bright_pass_mirrors_the_shader_expression` pin.
    #[test]
    fn shaders_pin_the_mirror_constants() {
        let frag = include_str!("../shaders/presentation.frag");
        for constant in [
            "2.51", "0.03", "2.43", "0.59", "0.14", // ACES
            "0.842479062253094", "0.0423282422610123", "0.0423756549057051",
            "0.0784335999999992", "0.878468636469772", "0.0784336",
            "0.0792237451477643", "0.0791661274605434", "0.879142973793104",
            "1.19687900512017", "-0.0528968517574562", "-0.0529716355144438",
            "-0.0980208811401368", "1.15190312990417", "-0.0980434501171241",
            "-0.0990297440797205", "-0.0989611768448433", "1.15107367264116",
            "15.5", "40.14", "31.96", "6.868", "0.4298", "0.1191", "0.00232",
            "-12.47393", "4.026069", "2.2",
        ] {
            assert!(
                frag.contains(constant),
                "presentation.frag no longer contains the AgX/ACES constant \
                 `{constant}` — the shader and tonemap.rs mirror have drifted"
            );
        }
        // Full-precision table entries must not be silently re-rounded in
        // the Rust mirror (the pin above reads the GLSL, this reads Rust).
        let here = include_str!("tonemap.rs");
        for table_entry in ["0.842_479_06", "1.151_903_1", "-0.099_029_74"] {
            assert!(
                here.contains(table_entry),
                "tonemap.rs lost full-precision table entry `{table_entry}`"
            );
        }
    }

    /// Operator ids are a wire contract with the push-constant uint lane;
    /// renumbering would silently re-map every saved profile.
    #[test]
    fn operator_ids_are_stable() {
        assert_eq!(TONEMAP_OP_ACES, 0);
        assert_eq!(TONEMAP_OP_AGX, 1);
        assert_eq!(TonemapOp::Aces.shader_value(), TONEMAP_OP_ACES);
        assert_eq!(TonemapOp::Agx.shader_value(), TONEMAP_OP_AGX);
        // #4584 — the generated header must carry the same ids, and the
        // shader must compare against the define, not a hand-typed literal.
        let header = include_str!("../shaders/include/shader_constants.glsl");
        assert!(header.contains("#define TONEMAP_OP_ACES 0u"));
        assert!(header.contains("#define TONEMAP_OP_AGX 1u"));
        let frag = include_str!("../shaders/presentation.frag");
        assert!(
            frag.contains("params.tonemapOp == TONEMAP_OP_AGX"),
            "presentation.frag must dispatch on the generated define (#4584)"
        );
    }
}
