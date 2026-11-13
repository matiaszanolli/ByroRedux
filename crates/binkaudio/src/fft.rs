//! A radix-2 complex FFT and the inverse DCT built on it (A5) — `[ours]`, standard mathematics.

#[derive(Clone, Copy, Default)]
struct C {
    re: f64,
    im: f64,
}

/// `x[n] = sum_k c_k cos(pi k (n + 0.5) / N)` for n = 0..N, N a power of two (A5's transform before
/// its scale): the real part of a 2N-point inverse DFT of `c_k e^(i pi k / 2N)`.
pub(crate) struct Idct {
    n: usize,
    /// e^(i pi k / 2N), k = 0..N.
    pre: Vec<C>,
    /// e^(+2 pi i j / 2N), j = 0..N: the inverse DFT's twiddles.
    tw: Vec<C>,
    rev: Vec<u32>,
    buf: Vec<C>,
}

impl Idct {
    pub fn new(n: usize) -> Self {
        assert!(n.is_power_of_two());
        let m = 2 * n;
        let pi = std::f64::consts::PI;
        let pre = (0..n).map(|k| C { re: (pi * k as f64 / m as f64).cos(), im: (pi * k as f64 / m as f64).sin() }).collect();
        let tw = (0..n)
            .map(|j| C { re: (2.0 * pi * j as f64 / m as f64).cos(), im: (2.0 * pi * j as f64 / m as f64).sin() })
            .collect();
        let bits = m.trailing_zeros();
        let rev = (0..m as u32).map(|i| i.reverse_bits() >> (32 - bits)).collect();
        Self { n, pre, tw, rev, buf: vec![C::default(); m] }
    }

    pub fn run(&mut self, c: &[f64], out: &mut [f64]) {
        let (n, m) = (self.n, 2 * self.n);
        for v in self.buf.iter_mut() {
            *v = C::default();
        }
        for ((&v, p), &r) in c.iter().zip(&self.pre).zip(&self.rev).take(n) {
            self.buf[r as usize] = C { re: v * p.re, im: v * p.im };
        }
        let mut len = 2;
        while len <= m {
            let step = m / len;
            for start in (0..m).step_by(len) {
                for j in 0..len / 2 {
                    let w = self.tw[j * step];
                    let (a, b) = (self.buf[start + j], self.buf[start + j + len / 2]);
                    let t = C { re: b.re * w.re - b.im * w.im, im: b.re * w.im + b.im * w.re };
                    self.buf[start + j] = C { re: a.re + t.re, im: a.im + t.im };
                    self.buf[start + j + len / 2] = C { re: a.re - t.re, im: a.im - t.im };
                }
            }
            len *= 2;
        }
        for (o, v) in out.iter_mut().zip(&self.buf[..n]) {
            *o = v.re;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn matches_the_direct_sum() {
        let n = 64;
        let c: Vec<f64> = (0..n).map(|k| ((k * 37 % 11) as f64 - 5.0) * 0.7).collect();
        let mut out = vec![0.0; n];
        super::Idct::new(n).run(&c, &mut out);
        for (i, o) in out.iter().enumerate() {
            let want: f64 =
                (0..n).map(|k| c[k] * (std::f64::consts::PI * k as f64 * (i as f64 + 0.5) / n as f64).cos()).sum();
            assert!((o - want).abs() < 1e-9, "{i}: {o} {want}");
        }
    }
}
