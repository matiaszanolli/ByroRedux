//! A Bink Audio track: packets to interleaved 16-bit PCM (A2-A7).

use crate::bits::Bits;
use crate::fft::Idct;
use std::fmt;

/// Coefficients per channel and block, and the block length (A4, A5).
const N: usize = 2048;
/// Samples per channel each block overlaps the next by (A3, A7; `[wiki:Bink Audio]` frame size / 16).
const OVERLAP: usize = N / 16;
/// `[wiki:Bink Audio]` critical frequencies.
const CRIT: [u32; 25] = [
    100, 200, 300, 400, 510, 630, 770, 920, 1080, 1270, 1480, 1720, 2000, 2320, 2700, 3150, 3700, 4400, 5300,
    6400, 7700, 9500, 12000, 15500, 24500,
];
/// `[wiki:Bink Audio]` run lengths, in groups of 8 coefficients (A4).
const RLE: [usize; 16] = [2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 32, 64];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A track this decoder has not been measured on (A0: every game track is 48 kHz stereo DCT).
    Unsupported(String),
    /// The packet's stated size is not a whole number of sample frames.
    BadSize(u32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Unsupported(what) => write!(f, "unsupported: {what}"),
            Error::BadSize(n) => write!(f, "packet of {n} decoded bytes is not a whole number of sample frames"),
        }
    }
}

impl std::error::Error for Error {}

pub struct Decoder {
    channels: usize,
    /// First coefficient of each band, then N (A6).
    edges: Vec<usize>,
    /// 10^(q * 0.0664) for q = 0..95 (A5).
    quant: [f64; 96],
    idct: Idct,
    /// The previous block's last OVERLAP x channels values, interleaved (A7); `None` before the first.
    tail: Option<Vec<i16>>,
    coefs: Vec<f64>,
    samples: Vec<f64>,
    block: Vec<i16>,
}

impl Decoder {
    /// A decoder for a track of `rate` Hz, stereo or mono, coded with the DCT or the RDFT
    /// (`[wiki:Bink Container]` track flags bits 13 and 12).
    pub fn new(rate: u32, stereo: bool, dct: bool) -> Result<Self, Error> {
        if rate != 48000 || !stereo || !dct {
            return Err(Error::Unsupported(format!(
                "{rate} Hz, {}, {} (only 48 kHz stereo DCT tracks are measured)",
                if stereo { "stereo" } else { "mono" },
                if dct { "DCT" } else { "RDFT" }
            )));
        }
        // A6: band edges 2 floor(f (N/2) / (rate/2)) below N
        let half = rate.div_ceil(2);
        let mut edges = vec![2];
        edges.extend(
            CRIT.iter().map(|&f| 2 * (f as usize * (N / 2) / half as usize)).take_while(|&e| e < N),
        );
        edges.push(N);
        Ok(Self {
            channels: 1 + stereo as usize,
            edges,
            quant: std::array::from_fn(|q| 10f64.powf(q as f64 * 0.0664)),
            idct: Idct::new(N),
            tail: None,
            coefs: vec![0.0; N],
            samples: vec![0.0; N],
            block: Vec::with_capacity(N * 2),
        })
    }

    /// Samples per channel one block adds to the output.
    pub fn block_samples(&self) -> usize {
        N - OVERLAP
    }

    /// One packet (its container-stated decoded size and data), appended to `out` as interleaved
    /// 16-bit samples. A8: the last packet of a stream states a size short of whole blocks; its last
    /// block is decoded and only the stated samples are kept.
    pub fn decode(&mut self, decoded_bytes: u32, data: &[u8], out: &mut Vec<i16>) -> Result<(), Error> {
        let per_block = 2 * self.channels * self.block_samples();
        let want = out.len() + decoded_bytes as usize / 2;
        if !decoded_bytes.is_multiple_of(2 * self.channels as u32) {
            return Err(Error::BadSize(decoded_bytes));
        }
        let mut r = Bits::new(data);
        for _ in 0..(decoded_bytes as usize).div_ceil(per_block) {
            self.block(&mut r, out);
        }
        out.truncate(want);
        Ok(())
    }

    /// One block (A7): 2 unused bits, every channel, padding to 32 bits.
    fn block(&mut self, r: &mut Bits, out: &mut Vec<i16>) {
        r.bits(2);
        self.block.clear();
        self.block.resize(N * self.channels, 0);
        for c in 0..self.channels {
            self.channel(r);
            let scale = 2.0 / (N as f64).sqrt(); // A5
            self.idct.run(&self.coefs, &mut self.samples);
            for (n, &v) in self.samples.iter().enumerate() {
                self.block[n * self.channels + c] = (v * scale).round().clamp(-32768.0, 32767.0) as i16;
            }
        }
        r.align32();
        let w = OVERLAP * self.channels;
        if let Some(tail) = &self.tail {
            // A7: value j of the head faded in by j / w, the 16-bit difference wrapping, floored, saturated
            for (j, (h, &t)) in self.block[..w].iter_mut().zip(tail).enumerate() {
                let d = h.wrapping_sub(t) as i32;
                *h = (t as i32 + (d * j as i32).div_euclid(w as i32)).clamp(-32768, 32767) as i16;
            }
        }
        let keep = self.block.len() - w;
        self.tail = Some(self.block[keep..].to_vec());
        out.extend_from_slice(&self.block[..keep]);
    }

    /// One channel's coefficients (A2-A6) into `self.coefs`.
    fn channel(&mut self, r: &mut Bits) {
        for k in 0..2 {
            let (e, m, neg) = (r.bits(5), r.bits(23), r.bit());
            let v = if e <= 23 { m as f64 * 2f64.powi(e as i32 - 23) } else { 0.0 }; // A3
            self.coefs[k] = if neg { -v } else { v };
        }
        let bands = self.edges.len() - 1;
        let mut q = [0f64; 32];
        for v in q.iter_mut().take(bands) {
            *v = self.quant[(r.bits(8) as usize).min(95)];
        }
        let (mut i, mut band) = (2, 0);
        while i < N {
            let len = if r.bit() { 8 * RLE[r.bits(4) as usize] } else { 8 };
            let width = r.bits(4);
            for j in i..(i + len).min(N) {
                let mut v = if width > 0 { r.bits(width) as f64 } else { 0.0 };
                if v != 0.0 && r.bit() {
                    v = -v;
                }
                while j >= self.edges[band + 1] {
                    band += 1;
                }
                self.coefs[j] = v * q[band];
            }
            i += len;
        }
    }
}
