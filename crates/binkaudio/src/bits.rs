//! LSB-first bit reader (A2). Past the end of the data every bit reads 0.

pub(crate) struct Bits<'a> {
    data: &'a [u8],
    pub pos: usize,
}

impl<'a> Bits<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// `n` bits (at most 32), the first read least significant.
    #[inline]
    pub fn bits(&mut self, n: u32) -> u32 {
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
        let v = (w >> (self.pos & 7)) & ((1u64 << n) - 1);
        self.pos += n as usize;
        v as u32
    }

    #[inline]
    pub fn bit(&mut self) -> bool {
        self.bits(1) == 1
    }

    pub fn align32(&mut self) {
        self.pos = (self.pos + 31) & !31;
    }
}
