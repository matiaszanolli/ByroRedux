//! Element syntax shared by every macroblock type, and intra macroblocks (P10-P18, P23, P26).

use crate::bits::{code, vlc, Bits, Code, Vlc};
use crate::context::{Scratch, SliceCtx};
use crate::frame::{BlockRows, MbTrace, MbType, PlaneRows, Rows};
use crate::idct::{block_pixels, dequant_ac, dequant_dc, Coef, NARROW, ZIGZAG};
use crate::tables::{CHROMA_QUANT, INTRA_QUANT};
use crate::Error;

/// Wiki: order of the 16 luma blocks of a macroblock, as (row, column).
pub(crate) const ZORD: [(usize, usize); 16] = [
    (0, 0),
    (0, 1),
    (1, 0),
    (1, 1),
    (0, 2),
    (0, 3),
    (1, 2),
    (1, 3),
    (2, 0),
    (2, 1),
    (3, 0),
    (3, 1),
    (2, 2),
    (2, 3),
    (3, 2),
    (3, 3),
];
/// The 4 chroma blocks of a macroblock, as (row, column).
pub(crate) const ORDER4: [(usize, usize); 4] = [(0, 0), (0, 1), (1, 0), (1, 1)];

/// Wiki KB2g AC skip codebooks; the CBP's upper half selects one per block (P11).
const CB_VLC0: [Code; 14] = [
    code("1", 0),
    code("000", 1),
    code("0010", 2),
    code("001101000", 3),
    code("001101100", 4),
    code("0011000", 5),
    code("0011001", 6),
    code("001101010", 7),
    code("00110111", 8),
    code("001101001", 9),
    code("001101101", 10),
    code("001101011", 11),
    code("01", 12),
    code("00111", 13),
];
const CB_VLC1: [Code; 14] = [
    code("1", 0),
    code("001", 1),
    code("0000", 2),
    code("0001", 3),
    code("01000", 4),
    code("0100110", 5),
    code("01010", 6),
    code("010010", 7),
    code("0101110", 8),
    code("01011110", 9),
    code("01011111", 10),
    code("0100111", 11),
    code("011", 12),
    code("010110", 13),
];
static AC_VLC: [Vlc; 2] = [vlc(&CB_VLC0), vlc(&CB_VLC1)];
/// Wiki: chroma CBP pattern when the "same as before" bit is set.
const CHROMA_PATTERN: [u32; 16] = [0, 0, 0, 0xF, 0, 0xF, 0xF, 0xF, 0, 0xF, 0xF, 0xF, 0xF, 0xF, 0xF, 0xF];

/// Quantiser delta (P10).
pub(crate) fn read_dq(r: &mut Bits) -> i32 {
    let mut dq = r.zeros(4) as i32;
    if dq == 3 {
        dq += r.bit() as i32;
    } else if dq == 4 {
        dq += r.bits(5) as i32 + 1;
    }
    if dq != 0 && r.bit() == 1 {
        dq = -dq;
    }
    dq
}

/// DC delta (P3/P6). P27: after the 11th prefix one the next bit is read and ignored.
pub(crate) fn read_dc(r: &mut Bits) -> i32 {
    // at most 11 + 1 + 8 + 1 bits: one peek
    let x = r.peek();
    let n = x.trailing_ones().min(11);
    let mut used = n + 1; // the zero ending the prefix, or the ignored bit after 11 ones
    let mut v = n as i32;
    if n >= 4 {
        v = (1 << (n - 3)) + ((x >> used) & ((1 << (n - 3)) - 1)) as i32 + 2;
        used += n - 3;
    }
    if v != 0 {
        if (x >> used) & 1 == 1 {
            v = -v;
        }
        used += 1;
    }
    r.skip(used);
    v
}

/// Wiki KB2g luma CBP, predicted from `prev` (P10).
pub(crate) fn read_luma_cbp(r: &mut Bits, prev: u32) -> u32 {
    let mut ones = (prev & 0xFFFF).count_ones();
    let mut mask = 0;
    if ones >= 8 {
        ones = 16 - ones;
        mask = 0xFFFF;
    }
    let mut cbp = 0;
    if r.bit() == 0 {
        if ones > 3 {
            cbp = r.bits(16);
        } else {
            for i in (0..16).step_by(4) {
                if r.bit() == 0 {
                    cbp |= r.bits(4) << i;
                }
            }
        }
    }
    cbp ^= mask;
    // P30: KB2j has no duplicate bit when the CBP is 0
    if !(r.rev >= b'j' && cbp == 0) && r.bit() == 1 {
        cbp |= cbp << 16;
    }
    cbp
}

/// Wiki KB2g chroma CBP, predicted from `prev` (P10).
pub(crate) fn read_chroma_cbp(r: &mut Bits, prev: u32) -> u32 {
    if r.bit() == 1 {
        return (prev & 0xF0000) | CHROMA_PATTERN[(prev & 0xF) as usize];
    }
    let mut cbp = r.bits(4);
    if r.bit() == 1 {
        cbp |= cbp << 16;
    }
    cbp
}

/// The coded AC of the blocks of one plane of a macroblock: (scan index, level — never 0) per
/// coefficient, all blocks in one list, reused from one macroblock to the next.
#[derive(Default)]
pub(crate) struct AcList {
    list: Vec<(u8, i32)>,
    /// `list` dequantised (`dequant`): (raster index, value).
    deq: Vec<(u8, i64)>,
    span: [(usize, usize); 16],
}

impl AcList {
    /// The AC of `blocks` blocks: those with a CBP bit, each with the codebook its upper bit selects
    /// (P11).
    pub fn read(&mut self, r: &mut Bits, cbp: u32, blocks: usize) -> Result<(), Error> {
        self.list.clear();
        for i in 0..blocks {
            let start = self.list.len();
            if cbp & (1 << i) != 0 {
                read_ac(r, (cbp >> (16 + i)) & 1 != 0, &mut self.list)?;
            }
            self.span[i] = (start, self.list.len());
        }
        Ok(())
    }

    /// Block `i`'s coefficients.
    pub fn block(&self, i: usize) -> &[(u8, i32)] {
        &self.list[self.span[i].0..self.span[i].1]
    }

    /// P16: every coefficient dequantised with `mult` at `q`, in coding order (the first error is that
    /// of the first bad coefficient). Returns the largest magnitude.
    pub fn dequant(&mut self, mult: &[i32; 64], q: i32) -> Result<u64, Error> {
        self.deq.clear();
        let mut big = 0;
        for &(k, lv) in &self.list {
            let z = ZIGZAG[k as usize];
            let v = dequant_ac(lv, mult[z], q)?;
            big = big.max(v.unsigned_abs());
            self.deq.push((z as u8, v));
        }
        Ok(big)
    }

    /// Block `i`'s dequantised coefficients (after `dequant`).
    pub fn coefs(&self, i: usize) -> &[(u8, i64)] {
        &self.deq[self.span[i].0..self.span[i].1]
    }

    /// For traces: every block's coefficients.
    pub fn blocks(&self, n: usize) -> Vec<Vec<(u8, i32)>> {
        (0..n).map(|i| self.block(i).to_vec()).collect()
    }
}

/// AC coefficients of one block (P11, P16), appended to `out`.
fn read_ac(r: &mut Bits, alt: bool, out: &mut Vec<(u8, i32)>) -> Result<(), Error> {
    let cb = &AC_VLC[alt as usize];
    let (mut idx, mut esc) = (1usize, 0);
    while idx < 64 {
        let skip = if esc > 0 {
            esc -= 1;
            0
        } else {
            match r.code(cb)? {
                12 => break,
                11 => r.bits(6) as usize,
                13 => {
                    esc = 6; // P16: 7 coded values in a row (this + 6), no skip codes
                    0
                }
                s => s as usize,
            }
        };
        idx += skip;
        if idx >= 64 {
            break;
        }
        // the level: up to 12 ones, for 3 or more of them that many minus 2 bits, a sign (at most 23
        // bits: one peek)
        let x = r.peek();
        let p = x.trailing_ones().min(12);
        let mut used = p + (p < 12) as u32;
        let mut v = p as i32 + 1;
        if v >= 4 {
            let m = v as u32 - 3;
            v = (1 << m) + ((x >> used) & ((1 << m) - 1)) as i32 + 2;
            used += m;
        }
        if (x >> used) & 1 == 1 {
            v = -v;
        }
        r.skip(used + 1);
        out.push((idx as u8, v));
        idx += 1;
    }
    Ok(())
}

/// Wiki, P14: gradient prediction clamped to the range of the three neighbours.
#[inline]
pub(crate) fn median3(a: i64, b: i64, c: i64) -> i64 {
    (a + b - c).max(a.min(b).min(c)).min(a.max(b).max(c))
}

fn predict_dc(a: Option<i64>, b: Option<i64>, c: Option<i64>) -> i64 {
    match (a, b, c) {
        (None, None, _) => 1024, // P12 grey
        (Some(a), None, _) => a,
        (None, Some(b), _) => b,
        (Some(a), Some(b), None) => a.max(b).min(a.min(b).max(2 * a - b)), // wiki form (never reached)
        (Some(a), Some(b), Some(c)) => median3(a, b, c),
    }
}

/// P17: DC neighbours on the frame's block grid, nothing above the slice's first block row.
fn dc_neighbours(blocks: &BlockRows, bx: usize, by: usize, top_by: usize) -> [Option<i64>; 3] {
    let a = if bx > 0 { blocks.dc_at(bx - 1, by) } else { None };
    if by <= top_by {
        return [a, None, None];
    }
    let c = if bx > 0 { blocks.dc_at(bx - 1, by - 1) } else { None };
    [a, blocks.dc_at(bx, by - 1), c]
}

/// The blocks of one plane of an intra macroblock: DCs (predicted across macroblocks), then AC. Returns
/// the coded DC levels.
#[allow(clippy::too_many_arguments)]
fn decode_blocks(
    r: &mut Bits,
    plane: &mut PlaneRows,
    blocks: &mut BlockRows,
    ac: &mut AcList,
    cbp: u32,
    dc_flag: bool,
    luma: bool,
    (bx0, by0): (usize, usize),
    q: i32,
    top_by: usize,
) -> Result<[i32; 16], Error> {
    let order: &[(usize, usize)] = if luma { &ZORD } else { &ORDER4 };
    let table = if luma { &INTRA_QUANT } else { &CHROMA_QUANT }; // P18: chroma has its own matrices
    let mut deltas = [0i32; 16];
    if dc_flag {
        for d in deltas[..order.len()].iter_mut() {
            *d = read_dc(r);
        }
    }
    let qd = q.max(8); // P16: DC quantiser clamped at 8
    for (i, &(dy, dx)) in order.iter().enumerate() {
        let (bx, by) = (bx0 + dx, by0 + dy);
        let [a, b, c] = dc_neighbours(blocks, bx, by, top_by);
        let v = predict_dc(a, b, c) + dequant_dc(deltas[i], qd)?;
        let k = blocks.at(bx, by);
        blocks.dc[k] = v.clamp(0, 2047) as i32; // P26
    }
    ac.read(r, cbp, order.len())?;
    for (i, &(dy, dx)) in order.iter().enumerate() {
        let k = blocks.at(bx0 + dx, by0 + dy);
        (blocks.ncoef[k], blocks.intra[k]) = (ac.block(i).len() as u8, true);
    }
    // the DC terms (8 x 0..2047 + 32) are narrow
    if ac.dequant(&table[(q & 3) as usize], q)? < NARROW {
        put_blocks::<i32>(plane, blocks, ac, order, (bx0, by0));
    } else {
        put_blocks::<i64>(plane, blocks, ac, order, (bx0, by0));
    }
    Ok(deltas)
}

/// The pixels of an intra macroblock's blocks of one plane, clamped.
fn put_blocks<T: Coef>(
    plane: &mut PlaneRows,
    blocks: &BlockRows,
    ac: &AcList,
    order: &[(usize, usize)],
    (bx0, by0): (usize, usize),
) {
    let mut px = [T::default(); 64];
    for (i, &(dy, dx)) in order.iter().enumerate() {
        let (bx, by) = (bx0 + dx, by0 + dy);
        let (dc_term, coefs) = (blocks.dc[blocks.at(bx, by)] as i64 * 8 + 32, ac.coefs(i));
        if coefs.is_empty() {
            // a DC-only block is flat (P12)
            let v = (dc_term >> 6).clamp(0, 255) as u8;
            for yy in 0..8 {
                let o = plane.at(bx * 8, by * 8 + yy);
                plane.px[o..o + 8].fill(v);
            }
            continue;
        }
        block_pixels(dc_term, coefs, &mut px);
        let out = T::clamp_u8(&px);
        for (yy, row) in out.chunks_exact(8).enumerate() {
            let o = plane.at(bx * 8, by * 8 + yy);
            plane.px[o..o + 8].copy_from_slice(row);
        }
    }
}

/// An intra macroblock: quantiser, luma CBP, luma blocks, then U and V (P10-P18).
pub(crate) fn decode_mb(
    r: &mut Bits,
    ctx: &mut SliceCtx,
    s: &mut Scratch,
    f: &mut Rows,
    mbx: usize,
    mby: usize,
) -> Result<(), Error> {
    let i = ctx.mi(mbx, mby);
    let q = ctx.q_pred(mbx, mby) + read_dq(r);
    ctx.q[i] = q;
    f.set_q(mbx, mby, q);
    let cbp = read_luma_cbp(r, ctx.left(&ctx.cbp, mbx, mby));
    ctx.cbp[i] = cbp;
    let dc_flag = r.bit() == 1;
    let top = ctx.y0;
    let [y, u, v] = &mut f.planes;
    let [by, bu, bv] = &mut f.blocks;
    let bit_start = r.pos;
    let ac = &mut s.ac[0];
    let levels = decode_blocks(r, y, by, ac, cbp, dc_flag, true, (mbx * 4, mby * 4), q, top * 4)?;
    if ctx.trace == Some((mbx, mby)) {
        f.trace = Some(MbTrace {
            typ: Some(MbType::Intra),
            bit_start,
            q,
            cbp,
            dc_flag,
            res_dc: ZORD.iter().map(|&(dy, dx)| by.dc[by.at(mbx * 4 + dx, mby * 4 + dy)] as i64).collect(),
            res_dc_level: levels.to_vec(),
            res_ac: ac.blocks(16),
            ..MbTrace::default()
        });
    }
    let prev = ctx.left(&ctx.ccbp, mbx, mby);
    let mut cc = [0u32; 2];
    for (k, (plane, blocks)) in [(u, bu), (v, bv)].into_iter().enumerate() {
        cc[k] = read_chroma_cbp(r, prev[k]);
        let cdc = r.bit() == 1;
        decode_blocks(r, plane, blocks, &mut s.ac[k + 1], cc[k], cdc, false, (mbx * 2, mby * 2), q, top * 2)?;
    }
    ctx.ccbp[i] = cc;
    Ok(())
}
