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
    /// The packet's stated size is not a whole number of sample frames, or is more than its data
    /// can hold.
    BadSize(u32),
    /// A block's bits run past the packet's data (`bits_read` of `bits`): the bits past the end
    /// would read as zeros and decode as made-up samples (INC4, `docs/spec/probes.md` A10).
    Overrun { bits_read: usize, bits: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Unsupported(what) => write!(f, "unsupported: {what}"),
            Error::BadSize(n) => write!(f, "packet of {n} decoded bytes is not a whole number of sample frames"),
            Error::Overrun { bits_read, bits } => {
                write!(f, "a block reads {bits_read} bits from a packet of {bits}")
            }
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
    /// block is decoded and only the stated samples are kept. On an error nothing is appended, and
    /// the overlap carried into the next block is the refused one's: the track ends there.
    pub fn decode(&mut self, decoded_bytes: u32, data: &[u8], out: &mut Vec<i16>) -> Result<(), Error> {
        let per_block = 2 * self.channels * self.block_samples();
        let want = out.len() + decoded_bytes as usize / 2;
        if !decoded_bytes.is_multiple_of(2 * self.channels as u32) {
            return Err(Error::BadSize(decoded_bytes));
        }
        // A stated size needing more blocks than the packet's bits can encode is not this packet's,
        // and decoding it would grow `out` from the container's word alone: past the data every bit
        // reads 0, and zeros decode as silent blocks `[ours]`.
        let blocks = (decoded_bytes as usize).div_ceil(per_block);
        if blocks.saturating_mul(self.min_block_bits()) > data.len().saturating_mul(8) {
            return Err(Error::BadSize(decoded_bytes));
        }
        let mut r = Bits::new(data);
        let start = out.len();
        for _ in 0..blocks {
            // A10: no block of the 236,690 packets in the installed corpus reads past its packet, so
            // one that does is cut or not this packet's; its samples (and the packet's) are dropped
            // rather than decoded from the zeros past the end.
            let bits_read = self.block(&mut r, out);
            if bits_read > data.len() * 8 {
                out.truncate(start);
                return Err(Error::Overrun { bits_read, bits: data.len() * 8 });
            }
        }
        out.truncate(want);
        Ok(())
    }

    /// The fewest bits any block can occupy, from what `block` and `channel` read unconditionally:
    /// 2 unused bits, then per channel the two 29-bit leading coefficients (A3), 8 bits per band's
    /// quantiser, and the runs covering coefficients 2..N, each at least a flag, a 4-bit length
    /// index and a 4-bit width, and at most `8 * 64` coefficients long (the largest `RLE` entry).
    /// Alignment padding is not counted, so a real block always meets it; a packet whose stated
    /// size needs more blocks than this allows is refused (INC3-1: the earlier bound, 64 bits a
    /// block, let a zero-filled packet expand ~100x further than a real stream does).
    fn min_block_bits(&self) -> usize {
        let bands = self.edges.len() - 1;
        let runs = (N - 2).div_ceil(8 * RLE[RLE.len() - 1]);
        2 + self.channels * (2 * 29 + 8 * bands + 9 * runs)
    }

    /// One block (A7): 2 unused bits, every channel, padding to 32 bits. Answers where the block's
    /// bits ended, before the padding.
    fn block(&mut self, r: &mut Bits, out: &mut Vec<i16>) -> usize {
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
        let end = r.pos;
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
        end
    }

    /// One channel's coefficients (A2-A6) into `self.coefs`.
    fn channel(&mut self, r: &mut Bits) {
        for coef in self.coefs.iter_mut().take(2) {
            let (e, m, neg) = (r.bits(5), r.bits(23), r.bit());
            let v = if e <= 23 { m as f64 * 2f64.powi(e as i32 - 23) } else { 0.0 }; // A3
            *coef = if neg { -v } else { v };
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

#[cfg(test)]
mod tests {

    /// #7163 (A10): a packet whose stated size passes the lower bound but whose blocks read past its
    /// data is refused, with nothing appended. All-zero bits code the shortest runs, so a zero packet
    /// cut to the minimum the bound allows runs past its end in its first block.
    #[test]
    fn a_block_reading_past_its_packet_is_refused() {
        let mut decoder = Decoder::new(48_000, true, true).unwrap();
        let per_block = (2 * decoder.channels * decoder.block_samples()) as u32;
        let data = vec![0u8; decoder.min_block_bits().div_ceil(8)];
        let mut out = vec![7i16; 3];
        match decoder.decode(per_block, &data, &mut out) {
            Err(Error::Overrun { bits_read, bits }) => assert!(bits_read > bits && bits == data.len() * 8),
            other => panic!("expected Overrun, got {other:?}"),
        }
        assert_eq!(out, [7, 7, 7], "nothing of the refused packet is appended");
    }
    use super::*;

    /// A packet's container-stated size is untrusted: one claiming more blocks than its bytes can
    /// hold is refused before `out` grows, instead of decoding gigabytes of samples from 16 bytes.
    #[test]
    fn a_stated_size_past_what_the_packet_holds_is_refused() {
        let mut decoder = Decoder::new(48000, true, true).unwrap();
        let mut out = Vec::new();
        let huge = u32::MAX - u32::MAX % 4;
        assert!(matches!(decoder.decode(huge, &[0u8; 16], &mut out), Err(Error::BadSize(n)) if n == huge));
        assert!(out.is_empty(), "nothing decoded");
        // One block's worth of size from bytes a zero block really fits in is accepted. (At the
        // bound's minimum bytes the zero block runs past them: #7163 refuses it as an overrun.)
        let one_block = vec![0u8; 4096];
        assert!(decoder.decode(4, &one_block, &mut out).is_ok());
        assert_eq!(out.len(), 2);
    }

    /// INC3-1: a zero-filled packet decodes as silent blocks, so the bound must be what a block
    /// really occupies, not 64 bits: 1 KiB claiming one stereo block per 8 bytes (the old bound) is
    /// refused, while the blocks those bits can really hold are not.
    #[test]
    fn the_block_bound_is_what_a_block_really_occupies() {
        let mut decoder = Decoder::new(48000, true, true).unwrap();
        let per_block = (2 * 2 * decoder.block_samples()) as u32;
        let data = [0u8; 1024];
        assert!(decoder.min_block_bits() > 8 * 64, "a stereo block is more than eight times the old 64-bit floor");
        let mut out = Vec::new();
        let old_bound = (data.len() / 8) as u32 * per_block;
        assert!(matches!(decoder.decode(old_bound, &data, &mut out), Err(Error::BadSize(_))));
        assert!(out.is_empty());
        // The bound admits the blocks those bits could hold. Zero bits code the shortest runs, so
        // these blocks really read more than the minimum and run past the 1 KiB: #7163 refuses
        // that as an overrun, not as a stated size the bound turned away.
        let fits = (data.len() * 8 / decoder.min_block_bits()) as u32 * per_block;
        assert!(matches!(decoder.decode(fits, &data, &mut out), Err(Error::Overrun { .. })));
        assert!(out.is_empty());
    }
}
