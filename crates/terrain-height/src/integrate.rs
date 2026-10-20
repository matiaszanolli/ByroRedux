//! Normal map → height field, by Frankot–Chellappa integration.
//!
//! A tangent-space normal map stores the surface slope at every texel. The
//! height whose gradient best matches those slopes in the least-squares sense
//! is, for a periodic domain, a closed form in the Fourier domain (Frankot &
//! Chellappa, "A Method for Enforcing Integrability in Shape from Shading
//! Algorithms", IEEE PAMI 10(4), 1988):
//!
//! ```text
//! H(u, v) = -j (u·P(u, v) + v·Q(u, v)) / (u² + v²),   H(0, 0) = 0
//! ```
//!
//! with `P`, `Q` the transforms of `p = ∂h/∂x`, `q = ∂h/∂y`. The periodic
//! domain is not an approximation here: terrain textures tile, so the field
//! has to wrap, and solving on the torus is what makes the result tile by
//! construction.

use crate::field::HeightField;
use crate::Rgba;
use rustfft::num_complex::Complex32;
use rustfft::FftPlanner;

/// Which way the green channel points. Red is `+x` = toward increasing
/// texture `u` in every tangent-space convention; green differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GreenConvention {
    /// Green = `+y` toward the top of the image (OpenGL / "Y+").
    YUp,
    /// Green = `+y` toward the bottom of the image (DirectX / "Y-").
    YDown,
}

/// The slope field a normal map encodes, in height units per texel, with
/// `y` running down the image (row order).
pub struct Gradients {
    pub width: usize,
    pub height: usize,
    /// `∂h/∂x`.
    pub p: Vec<f32>,
    /// `∂h/∂y` read as if green pointed up ([`GreenConvention::YUp`]);
    /// negate for [`GreenConvention::YDown`].
    pub q_y_up: Vec<f32>,
}

/// Smallest `n.z` the slope division accepts. A normal lying in the tangent
/// plane is an infinite slope; clamping it bounds the step a single texel
/// can contribute instead of letting one bad texel dominate the solve.
const MIN_NORMAL_Z: f32 = 0.1;

impl Gradients {
    /// Slopes from an RGB(A)8 tangent-space normal map. `z` is
    /// reconstructed from `x` and `y` for every texel rather than read from
    /// blue: BC5 maps (Fallout 4) carry no blue at all, and on the others the
    /// unit-length reconstruction removes block-compression error in `z`.
    pub fn from_normal_map(img: &Rgba) -> Self {
        let n = img.width * img.height;
        let mut p = Vec::with_capacity(n);
        let mut q_y_up = Vec::with_capacity(n);
        for px in img.pixels.chunks_exact(4).take(n) {
            let x = px[0] as f32 / 127.5 - 1.0;
            let y = px[1] as f32 / 127.5 - 1.0;
            let z = (1.0 - x * x - y * y).max(0.0).sqrt().max(MIN_NORMAL_Z);
            // n ∝ (-∂h/∂x, -∂h/∂y_up, 1); y_up = -y_row, so ∂h/∂y_row = n.y/n.z.
            p.push(-x / z);
            q_y_up.push(y / z);
        }
        Self {
            width: img.width,
            height: img.height,
            p,
            q_y_up,
        }
    }

    /// Mean absolute curl `|∂p/∂y - ∂q/∂x|` of the slope field read with
    /// `convention`, by central differences with wrap. A real gradient field
    /// has zero curl; reading green the wrong way round turns
    /// `∂p/∂y - ∂q/∂x` into `∂p/∂y + ∂q/∂x`, which for any surface that is not
    /// separable in `x` and `y` is not.
    pub fn mean_curl(&self, convention: GreenConvention) -> f32 {
        let (w, h) = (self.width as isize, self.height as isize);
        let sign = match convention {
            GreenConvention::YUp => 1.0,
            GreenConvention::YDown => -1.0,
        };
        let at = |v: &[f32], x: isize, y: isize| -> f32 {
            v[(y.rem_euclid(h) * w + x.rem_euclid(w)) as usize]
        };
        let mut total = 0.0f64;
        for y in 0..h {
            for x in 0..w {
                let dp_dy = 0.5 * (at(&self.p, x, y + 1) - at(&self.p, x, y - 1));
                let dq_dx = 0.5 * sign * (at(&self.q_y_up, x + 1, y) - at(&self.q_y_up, x - 1, y));
                total += (dp_dy - dq_dx).abs() as f64;
            }
        }
        (total / (w * h) as f64) as f32
    }

    /// The green convention under which this map is integrable, and how
    /// decisively: `other_curl / chosen_curl` (≥ 1; near 1 means the map
    /// cannot tell, as for a surface that varies along one axis only).
    pub fn detect_convention(&self) -> (GreenConvention, f32) {
        let up = self.mean_curl(GreenConvention::YUp);
        let down = self.mean_curl(GreenConvention::YDown);
        if up <= down {
            (
                GreenConvention::YUp,
                if up > 0.0 { down / up } else { f32::INFINITY },
            )
        } else {
            (
                GreenConvention::YDown,
                down.max(f32::MIN_POSITIVE).recip() * up,
            )
        }
    }

    /// Least-squares height for these slopes on the torus, in texel units,
    /// with zero mean.
    ///
    /// `highpass_sigma` (texels), when set, removes the part of the surface
    /// broader than a Gaussian of that width, exactly as subtracting a
    /// Gaussian blur of the height would, but inside the same Fourier solve.
    /// The `1/ω` in the closed form amplifies any small low-frequency bias an
    /// authored normal map carries into swells spanning the whole tile, and
    /// those swells, not the stones, would otherwise set the normalised range.
    pub fn integrate(
        &self,
        convention: GreenConvention,
        highpass_sigma: Option<f32>,
    ) -> HeightField {
        let (w, h) = (self.width, self.height);
        let sign = match convention {
            GreenConvention::YUp => 1.0,
            GreenConvention::YDown => -1.0,
        };
        let mut pf: Vec<Complex32> = self.p.iter().map(|&v| Complex32::new(v, 0.0)).collect();
        let mut qf: Vec<Complex32> = self
            .q_y_up
            .iter()
            .map(|&v| Complex32::new(sign * v, 0.0))
            .collect();
        let mut planner = FftPlanner::<f32>::new();
        fft_2d(&mut planner, &mut pf, w, h, false);
        fft_2d(&mut planner, &mut qf, w, h, false);

        let freq = |k: usize, n: usize| -> f32 {
            // Signed DFT frequency in radians per texel.
            let k = if k <= n / 2 {
                k as f32
            } else {
                k as f32 - n as f32
            };
            std::f32::consts::TAU * k / n as f32
        };
        let mut hf = vec![Complex32::new(0.0, 0.0); w * h];
        for ky in 0..h {
            let v = freq(ky, h);
            for kx in 0..w {
                let u = freq(kx, w);
                let denom = u * u + v * v;
                if denom == 0.0 {
                    continue; // DC: height is defined up to a constant.
                }
                let i = ky * w + kx;
                // -j (u P + v Q) / (u² + v²)
                let num = pf[i] * u + qf[i] * v;
                // Gaussian high-pass: 1 - FT(blur) = 1 - exp(-ω²σ²/2).
                let keep =
                    highpass_sigma.map_or(1.0, |sigma| 1.0 - (-0.5 * denom * sigma * sigma).exp());
                hf[i] = Complex32::new(num.im, -num.re) * (keep / denom);
            }
        }
        fft_2d(&mut planner, &mut hf, w, h, true);
        let scale = 1.0 / (w * h) as f32;
        HeightField {
            width: w,
            height: h,
            data: hf.iter().map(|c| c.re * scale).collect(),
        }
    }
}

/// In-place 2D DFT (unnormalised) over a row-major `w × h` buffer.
fn fft_2d(planner: &mut FftPlanner<f32>, buf: &mut [Complex32], w: usize, h: usize, inverse: bool) {
    let row = if inverse {
        planner.plan_fft_inverse(w)
    } else {
        planner.plan_fft_forward(w)
    };
    for r in buf.chunks_exact_mut(w) {
        row.process(r);
    }
    let col = if inverse {
        planner.plan_fft_inverse(h)
    } else {
        planner.plan_fft_forward(h)
    };
    let mut column = vec![Complex32::new(0.0, 0.0); h];
    for x in 0..w {
        for y in 0..h {
            column[y] = buf[y * w + x];
        }
        col.process(&mut column);
        for y in 0..h {
            buf[y * w + x] = column[y];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{bake_normal_map, bumpy_field, correlation};

    /// A height field baked into a normal map and integrated back must come
    /// out as the same shape (up to scale and offset, which normalisation
    /// discards anyway), for both green conventions, with the convention
    /// itself recovered from the map.
    #[test]
    fn integration_recovers_a_baked_height_field() {
        let truth = bumpy_field(64, 48);
        for convention in [GreenConvention::YUp, GreenConvention::YDown] {
            let img = bake_normal_map(&truth, convention, 6.0);
            let g = Gradients::from_normal_map(&img.view());
            let (detected, decisiveness) = g.detect_convention();
            assert_eq!(detected, convention);
            assert!(decisiveness > 3.0, "{decisiveness}");
            let h = g.integrate(detected, None);
            let r = correlation(&truth.data, &h.data);
            assert!(r > 0.99, "{convention:?}: r = {r}");
            // Reading green the wrong way round gives a different surface.
            let wrong = match convention {
                GreenConvention::YUp => GreenConvention::YDown,
                GreenConvention::YDown => GreenConvention::YUp,
            };
            assert!(correlation(&truth.data, &g.integrate(wrong, None).data) < 0.9);
        }
    }

    /// Raised bumps integrate as raised, not as dents: the sign of the
    /// recovered field matches the source, so stones poke out of dirt.
    #[test]
    fn integration_preserves_relief_sign() {
        let truth = bumpy_field(32, 32);
        let img = bake_normal_map(&truth, GreenConvention::YDown, 4.0);
        let h = Gradients::from_normal_map(&img.view()).integrate(GreenConvention::YDown, None);
        assert!(correlation(&truth.data, &h.data) > 0.99);
    }
}
