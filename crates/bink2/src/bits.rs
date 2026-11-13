//! Bit reader. Bits are taken from the least significant end of each byte first (P3).

use crate::Error;

pub(crate) struct Bits<'a> {
    data: &'a [u8],
    /// Position in bits from the start of `data`.
    pub pos: usize,
    /// Format revision, the last letter of the magic (`b'g'`, `b'j'`): element syntax differs (P30).
    pub rev: u8,
}

/// One codeword of a prefix code: `len` bits, `bits` holding them first-read-bit-highest.
#[derive(Clone, Copy)]
pub(crate) struct Code {
    len: u8,
    bits: u16,
    pub value: i32,
}

/// A codeword written as the bits in reading order, e.g. `"0010"`.
pub(crate) const fn code(s: &str, value: i32) -> Code {
    let b = s.as_bytes();
    let mut v = 0u16;
    let mut i = 0;
    while i < b.len() {
        v = (v << 1) | (b[i] - b'0') as u16;
        i += 1;
    }
    Code { len: b.len() as u8, bits: v, value }
}

/// Codewords are looked up by this many upcoming bits; no codebook has a longer codeword.
const VLC_BITS: u32 = 9;

#[derive(Clone, Copy)]
struct Entry {
    /// 0: no codeword starts with these bits.
    len: u8,
    value: i32,
}

/// A codebook as a lookup table indexed by the next `VLC_BITS` bits of the stream.
pub(crate) struct Vlc([Entry; 1 << VLC_BITS]);

/// The lookup table of a codebook. Where several codewords match the stream, the shortest wins, as a
/// reader taking one bit at a time would decide.
pub(crate) const fn vlc(cb: &[Code]) -> Vlc {
    let mut t = [Entry { len: 0, value: 0 }; 1 << VLC_BITS];
    let mut len = VLC_BITS as u8;
    while len >= 1 {
        let mut i = 0;
        while i < cb.len() {
            let c = cb[i];
            assert!(c.len as u32 <= VLC_BITS, "codeword longer than the lookup");
            if c.len == len {
                // the stream's bits come least significant first
                let (mut p, mut k) = (0usize, 0);
                while k < len {
                    p |= (((c.bits >> (len - 1 - k)) & 1) as usize) << k;
                    k += 1;
                }
                let mut hi = 0usize;
                while hi < 1 << (VLC_BITS - len as u32) {
                    t[p | (hi << len)] = Entry { len, value: c.value };
                    hi += 1;
                }
            }
            i += 1;
        }
        len -= 1;
    }
    Vlc(t)
}

impl<'a> Bits<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, rev: b'g' }
    }

    /// The next 57 or more bits, the first in bit 0. Past the end of the packet every bit reads 0
    /// (ours, not probed). For elements read in one go (`skip` then moves past them).
    #[inline]
    pub fn peek(&self) -> u64 {
        let i = self.pos >> 3;
        let w = match self.data.get(i..i + 8) {
            Some(b) => u64::from_le_bytes(b.try_into().unwrap()),
            None => {
                let mut b = [0u8; 8];
                let rest = self.data.get(i..).unwrap_or(&[]);
                b[..rest.len()].copy_from_slice(rest);
                u64::from_le_bytes(b)
            }
        };
        w >> (self.pos & 7)
    }

    /// One bit.
    #[inline]
    pub fn bit(&mut self) -> u32 {
        let b = match self.data.get(self.pos >> 3) {
            Some(&v) => (v >> (self.pos & 7)) as u32 & 1,
            None => 0,
        };
        self.pos += 1;
        b
    }

    /// `n` bits (at most 32) as an integer, first bit least significant.
    #[inline]
    pub fn bits(&mut self, n: u32) -> u32 {
        debug_assert!(n <= 32);
        let v = (self.peek() & ((1u64 << n) - 1)) as u32;
        self.pos += n as usize;
        v
    }

    /// A run of zeros read up to `max` of them (at most 32), and the one that ends it if it comes
    /// before `max`: the run's length.
    #[inline]
    pub fn zeros(&mut self, max: u32) -> u32 {
        let n = self.peek().trailing_zeros().min(max);
        self.pos += (n + (n < max) as u32) as usize;
        n
    }

    /// Moves past `n` bits.
    #[inline]
    pub fn skip(&mut self, n: u32) {
        self.pos += n as usize;
    }

    /// Skip to the next 32-bit boundary (P22: where slice 2 starts).
    pub fn align32(&mut self) {
        self.pos = (self.pos + 31) & !31;
    }

    /// One codeword. None matching: an error after 13 bits (no codeword is longer than 12).
    #[inline]
    pub fn code(&mut self, t: &Vlc) -> Result<i32, Error> {
        let e = t.0[(self.peek() & ((1 << VLC_BITS) - 1)) as usize];
        if e.len == 0 {
            self.pos += 13;
            return Err(Error::BadCodeword(self.pos));
        }
        self.pos += e.len as usize;
        Ok(e.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_are_read_lsb_first() {
        let mut r = Bits::new(&[0b1010_0110, 0x01]);
        assert_eq!(r.bits(3), 0b110);
        assert_eq!(r.bits(5), 0b10100);
        assert_eq!(r.bit(), 1);
        assert_eq!(r.bits(8), 0, "past the end reads zeros");
    }

    #[test]
    fn codewords_match_in_reading_order() {
        const CB: Vlc = vlc(&[code("1", 0), code("01", 1), code("00", 2)]);
        // bits in reading order: 0,1 | 1 | 0,0
        let mut r = Bits::new(&[0b0_0110]);
        assert_eq!(r.code(&CB).unwrap(), 1);
        assert_eq!(r.code(&CB).unwrap(), 0);
        assert_eq!(r.code(&CB).unwrap(), 2);
    }

    #[test]
    fn runs_stop_at_their_limit() {
        // zeros: 0,0,1 | 0,0,0 (limit 3, nothing more taken) | 1,1 ...
        let mut r = Bits::new(&[0b1100_0100]);
        assert_eq!(r.zeros(3), 2);
        assert_eq!(r.pos, 3);
        assert_eq!(r.zeros(3), 3);
        assert_eq!(r.pos, 6);
        r.skip(2);
        assert_eq!(r.zeros(4), 4, "bits past the end read 0");
        assert_eq!(r.pos, 12);
        let mut r = Bits::new(&[0b1000_0000]);
        assert_eq!(r.zeros(12), 7);
        assert_eq!(r.pos, 8);
    }
}
