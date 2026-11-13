//! Inter-frame macroblocks (P20-P23, P25, P26): block types, motion vectors, motion compensation and
//! the INTER residual.

use crate::bits::{code, vlc, Bits, Code, Vlc};
use crate::context::{Scratch, SliceCtx};
use crate::filters::{filter_a_residual, EdgeDirs};
use crate::frame::{MbTrace, MbType, Plane, PlaneRows, Rows};
use crate::idct::{block_pixels, dequant_dc, Coef, NARROW};
use crate::intra::{self, median3, read_dc, read_dq, read_luma_cbp, AcList, ORDER4, ZORD};
use crate::simd;
use crate::tables::INTER_QUANT;
use crate::Error;

type Mv = [i32; 2];

/// P20: the block-type list each macroblock row starts from (move-to-front by one place per use).
pub(crate) const TYPE_LIST0: [MbType; 4] = [MbType::Motion, MbType::Inter, MbType::Skip, MbType::Intra];

const MV_ESC: i32 = i32::MIN;
/// P21: MV component codes.
const MV_CODES: [Code; 16] = [
    code("1", 0),
    code("011", 1),
    code("00110", 2),
    code("00111", 3),
    code("0001100", 4),
    code("0001110", 5),
    code("0001101", 6),
    code("0001111", 7),
    code("0001011", -7),
    code("0001001", -6),
    code("0001010", -5),
    code("0001000", -4),
    code("00101", -3),
    code("00100", -2),
    code("010", -1),
    code("0000", MV_ESC),
];

static MV_VLC: Vlc = vlc(&MV_CODES);

/// P20: 1, 01, 000, 001 select list entries 0, 1, 2, 3.
fn read_block_type(r: &mut Bits) -> usize {
    if r.bit() == 1 {
        0
    } else if r.bit() == 1 {
        1
    } else if r.bit() == 1 {
        3
    } else {
        2
    }
}

/// P21: MV component in half pixels.
fn read_mv_component(r: &mut Bits) -> Result<i32, Error> {
    let v = r.code(&MV_VLC)?;
    if v != MV_ESC {
        return Ok(v);
    }
    let u = r.zeros(12);
    let val = r.bits(u + 4) as i32;
    let mag = (8 << u) + (val >> 1);
    Ok(if val & 1 == 1 { mag } else { -mag })
}

#[inline]
fn f6r(p: [i32; 6]) -> i32 {
    (19 * (p[2] + p[3]) - 4 * (p[1] + p[4]) + (p[0] + p[5]) + 16 - ((p[2] + p[3]) & 1)) >> 5
}

#[inline]
fn f6(p: [i32; 6]) -> i32 {
    f6r(p).clamp(0, 255)
}

/// The `n` x `n` source pixels from (x, y) on, coordinates clamped to the plane per axis (what every
/// source read does, P20); `R` rows of `W` bytes (the bytes past `n` zero).
#[inline]
fn window<const R: usize, const W: usize>(src: &Plane, x: i32, y: i32, n: usize) -> [[u8; W]; R] {
    let (w, h) = (src.width as i32, src.height as i32);
    let mut win = [[0u8; W]; R];
    if x >= 0 && y >= 0 && x + n as i32 <= w && y + n as i32 <= h {
        for (r, row) in win.iter_mut().enumerate().take(n) {
            let o = (y as usize + r) * src.width + x as usize;
            row[..n].copy_from_slice(&src.px[o..o + n]);
        }
    } else {
        let cols: [usize; W] = std::array::from_fn(|c| (x + c as i32).clamp(0, w - 1) as usize);
        for (r, row) in win.iter_mut().enumerate().take(n) {
            let o = (y + r as i32).clamp(0, h - 1) as usize * src.width;
            for (v, &c) in row.iter_mut().zip(&cols).take(n) {
                *v = src.px[o + c];
            }
        }
    }
    win
}

/// P20/P26: a 16x16 luma prediction at half-pel fractions (fx, fy) into `dst` (rows `dstride` apart)
/// from the 21 x 21 source pixels at `src` (rows `sstride` apart) that start 2 pixels up and left of it
/// (the 6-tap filter reads 2 pixels before and 3 after). With both fractional the horizontal pass is
/// rounded but not clamped.
pub(crate) fn luma6(src: &[u8], sstride: usize, dst: &mut [u8], dstride: usize, fx: i32, fy: i32) {
    let at = |x: usize, y: usize| src[y * sstride + x] as i32;
    let mut put = |x: usize, y: usize, v: i32| dst[y * dstride + x] = v as u8;
    match (fx, fy) {
        (0, 0) => {
            for y in 0..16 {
                for x in 0..16 {
                    put(x, y, at(x + 2, y + 2));
                }
            }
        }
        (_, 0) => {
            for y in 0..16 {
                for x in 0..16 {
                    put(x, y, f6(std::array::from_fn(|i| at(x + i, y + 2))));
                }
            }
        }
        (0, _) => {
            for y in 0..16 {
                for x in 0..16 {
                    put(x, y, f6(std::array::from_fn(|j| at(x + 2, y + j))));
                }
            }
        }
        _ => {
            // the horizontal pass per column, 21 rows each
            let mut hp = [[0i32; 21]; 16];
            for (x, col) in hp.iter_mut().enumerate() {
                for (y, v) in col.iter_mut().enumerate() {
                    *v = f6r(std::array::from_fn(|i| at(x + i, y)));
                }
            }
            for (x, col) in hp.iter().enumerate() {
                for (y, taps) in col.windows(6).enumerate() {
                    put(x, y, f6(taps.try_into().unwrap()));
                }
            }
        }
    }
}

/// The source of an `n` x `n` window from (x, y): the plane itself where the window lies inside it,
/// else `buf` filled by `window` (clamped coordinates). Returns the pixels and their row stride.
#[inline]
fn source<'a, const R: usize, const W: usize>(
    src: &'a Plane,
    (x, y): (i32, i32),
    n: usize,
    buf: &'a mut [[u8; W]; R],
) -> (&'a [u8], usize) {
    if x >= 0 && y >= 0 && x + n as i32 <= src.width as i32 && y + n as i32 <= src.height as i32 {
        (&src.px[y as usize * src.width + x as usize..], src.width)
    } else {
        *buf = window::<R, W>(src, x, y, n);
        (buf.as_flattened(), W)
    }
}

/// P20/P26: one 16x16 luma quadrant from `src` at half-pel `mv`; source pixels clamp to the plane.
fn mc_luma(dst: &mut PlaneRows, src: &Plane, x0: usize, y0: usize, [mvx, mvy]: Mv) {
    let (ix, fx, iy, fy) = (mvx >> 1, mvx & 1, mvy >> 1, mvy & 1);
    let mut buf = [[0u8; 32]; 21];
    let (s, stride) = source(src, (x0 as i32 + ix - 2, y0 as i32 + iy - 2), 21, &mut buf);
    let (o, w) = (dst.at(x0, y0), dst.width);
    if !simd::luma6(s, stride, &mut dst.px[o..], w, fx, fy) {
        luma6(s, stride, &mut dst.px[o..], w, fx, fy);
    }
}

/// P20: an 8x8 chroma prediction, bilinear at quarter-pel fractions (fx, fy), into `dst` from the
/// 9 x 9 source pixels at its top left (strides as `luma6`).
pub(crate) fn chroma_bilinear(src: &[u8], sstride: usize, dst: &mut [u8], dstride: usize, fx: i32, fy: i32) {
    let at = |x: usize, y: usize| src[y * sstride + x] as i32;
    let (w00, w10, w01, w11) = ((4 - fx) * (4 - fy), fx * (4 - fy), (4 - fx) * fy, fx * fy);
    for y in 0..8 {
        for x in 0..8 {
            let v = w00 * at(x, y) + w10 * at(x + 1, y) + w01 * at(x, y + 1) + w11 * at(x + 1, y + 1);
            dst[y * dstride + x] = ((v + 8) >> 4) as u8;
        }
    }
}

/// P20: one 8x8 chroma block at quarter-pel `mv` (the luma MV in chroma pixels).
fn mc_chroma(dst: &mut PlaneRows, src: &Plane, x0: usize, y0: usize, [mvx, mvy]: Mv) {
    let (ix, fx, iy, fy) = (mvx >> 2, mvx & 3, mvy >> 2, mvy & 3);
    let mut buf = [[0u8; 16]; 9];
    let (s, stride) = source(src, (x0 as i32 + ix, y0 as i32 + iy), 9, &mut buf);
    let (o, w) = (dst.at(x0, y0), dst.width);
    if !simd::chroma_bilinear(s, stride, &mut dst.px[o..], w, fx, fy) {
        chroma_bilinear(s, stride, &mut dst.px[o..], w, fx, fy);
    }
}

#[inline]
fn med(a: Mv, b: Mv, c: Mv) -> Mv {
    let m = |a: i32, b: i32, c: i32| a.min(b).max(a.max(b).min(c));
    [m(a[0], b[0], c[0]), m(a[1], b[1], c[1])]
}

/// P21/P22: median of three final quadrant MVs. Inside: left, top, top-left. On the slice's top
/// quadrant row the missing top pair is replaced by the quadrant two to the left and the one
/// below-left; in the frame's left quadrant column by the quadrant two above and the one above-right.
/// The slice's first quadrant predicts 0; its right and lower neighbours copy it.
fn predict_mv(ctx: &SliceCtx, qx: usize, qy: usize) -> Mv {
    let top = ctx.y0 * 2;
    let at = |x: usize, y: usize| ctx.mv[ctx.qi(x, y)];
    if qy > top && qx > 0 {
        med(at(qx - 1, qy), at(qx, qy - 1), at(qx - 1, qy - 1))
    } else if qy == top && qx > 0 {
        if qx == 1 {
            at(0, qy)
        } else {
            med(at(qx - 1, qy), at(qx - 2, qy), at(qx - 1, qy + 1))
        }
    } else if qx == 0 && qy > top {
        if qy == top + 1 {
            at(0, qy - 1)
        } else {
            med(at(0, qy - 1), at(0, qy - 2), at(1, qy - 1))
        }
    } else {
        [0, 0]
    }
}

/// P22: quadrant evaluation order — on the slice's top row quadrant 1 needs quadrant 2.
fn mv_order(qy0: usize, top: usize) -> [usize; 4] {
    if qy0 == top {
        [0, 2, 1, 3]
    } else {
        [0, 1, 2, 3]
    }
}

/// P22: one axis of a 16x16 luma quadrant's MV at the frame edge; `p0` = quadrant start, `n` = bound.
/// Whole-pel: valid when 0 <= s <= n-16, else s = n-16 (an unsigned compare: negative s lands at the far
/// side). Half-pel: valid when 2 <= s <= n-20, else a whole-pel copy from 2 (s = 0, 1) or from n-21.
/// P30, frame flag bit 17 (`clamp`): s is clamped instead, keeping the half pel, to 0..n-16 (whole) or
/// 2..n-19 (half: where the 6-tap filter stays inside the plane).
/// Chroma follows the adjusted luma MV.
fn edge_mv(p0: i32, mv: i32, n: i32, clamp: bool) -> i32 {
    let (s, p0, n) = (p0 as i64 + (mv >> 1) as i64, p0 as i64, n as i64);
    if clamp {
        let (lo, hi, frac) = if mv & 1 == 0 { (0, n - 16, 0) } else { (2, n - 19, 1) };
        return (2 * (s.clamp(lo, hi.max(lo)) - p0) + frac) as i32;
    }
    if mv & 1 == 0 {
        if (0..=n - 16).contains(&s) {
            mv
        } else {
            (2 * (n - 16 - p0)) as i32
        }
    } else if (2..=n - 20).contains(&s) {
        mv
    } else {
        (2 * (if (0..2).contains(&s) { 2 } else { n - 21 } - p0)) as i32
    }
}

fn add(p: Mv, d: [i32; 2]) -> Result<Mv, Error> {
    match (p[0].checked_add(d[0]), p[1].checked_add(d[1])) {
        (Some(x), Some(y)) => Ok([x, y]),
        _ => Err(Error::Overflow("motion vector")),
    }
}

/// The DCs of one plane of an INTER residual, per block in coding order: final, coded level,
/// dequantised delta; whether the plane's residual is in the wide grid (a coefficient beyond `NARROW`).
#[derive(Default)]
struct ResidualDc {
    dc: [i64; 16],
    level: [i32; 16],
    delta: [i64; 16],
    wide: bool,
}

fn blocks_of(p: usize) -> (usize, &'static [(usize, usize)]) {
    if p == 0 {
        (4, &ZORD)
    } else {
        (2, &ORDER4)
    }
}

/// P21: plane `p` of an INTER macroblock's residual: DCs predicted inside the macroblock from 0, then AC
/// (inter matrices for luma and chroma — the chroma choice is untested but holds on every file so far),
/// inverse transformed into the plane's scratch grid (w x w, w = 8 x blocks per side).
fn residual_plane(
    r: &mut Bits,
    s: &mut Scratch,
    p: usize,
    cbp: u32,
    dc_flag: bool,
    q: i32,
) -> Result<ResidualDc, Error> {
    let (n, order) = blocks_of(p);
    let mut res = ResidualDc::default();
    if dc_flag {
        for d in res.level[..order.len()].iter_mut() {
            *d = read_dc(r);
        }
    }
    let qd = q.max(8);
    let mut g = [[None::<i64>; 4]; 4];
    for (i, &(dy, dx)) in order.iter().enumerate() {
        let a = if dx > 0 { g[dy][dx - 1] } else { None };
        let b = if dy > 0 { g[dy - 1][dx] } else { None };
        let p = match (a, b) {
            (None, None) => 0,
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (Some(a), Some(b)) => {
                median3(a, b, g[dy - 1][dx - 1].expect("Z order decodes the top-left first"))
            }
        };
        let d = dequant_dc(res.level[i], qd)?;
        res.delta[i] = d;
        let lim = if r.rev >= b'j' { i64::MAX } else { 1023 };
        let v = (p + d).clamp(-lim, lim); // P27
        g[dy][dx] = Some(v);
        res.dc[i] = v;
    }
    let ac = &mut s.ac[p];
    ac.read(r, cbp, order.len())?;
    let big = ac.dequant(&INTER_QUANT[(q & 3) as usize], q)?;
    let dc_big = res.dc[..order.len()].iter().map(|d| (d * 8 + 32).unsigned_abs()).max().unwrap_or(0);
    res.wide = big.max(dc_big) >= NARROW;
    if res.wide {
        fill_grid(&mut s.grid_wide[p], n, order, &res.dc, ac);
    } else {
        fill_grid(&mut s.grid[p], n, order, &res.dc, ac);
    }
    Ok(res)
}

/// The blocks' pixels of a residual plane, at their places in the macroblock's grid.
fn fill_grid<T: Coef>(grid: &mut [T], n: usize, order: &[(usize, usize)], dc: &[i64; 16], ac: &AcList) {
    let w = 8 * n;
    let mut px = [T::default(); 64];
    for (i, &(dy, dx)) in order.iter().enumerate() {
        let coefs = ac.coefs(i);
        if coefs.is_empty() {
            // a DC-only block is flat (P12)
            let v = T::of((dc[i] * 8 + 32) >> 6);
            for yy in 0..8 {
                let o = (dy * 8 + yy) * w + dx * 8;
                grid[o..o + 8].fill(v);
            }
            continue;
        }
        block_pixels(dc[i] * 8 + 32, coefs, &mut px);
        for (yy, row) in px.chunks_exact(8).enumerate() {
            let o = (dy * 8 + yy) * w + dx * 8;
            grid[o..o + 8].copy_from_slice(row);
        }
    }
}

/// Adds plane `p`'s residual (assembled per macroblock in the scratch grid) to the prediction:
/// filtered inside the macroblock (P25) when filter A is on, then added with clamping.
fn add_residual(
    f: &mut Rows,
    p: usize,
    mbx: usize,
    mby: usize,
    res: &ResidualDc,
    s: &mut Scratch,
    filter_a: EdgeDirs,
) {
    let (n, order) = blocks_of(p);
    let (bx0, by0) = (mbx * n, mby * n);
    let blocks = &mut f.blocks[p];
    let (mut nc, mut dc) = ([0u32; 16], [0i64; 16]);
    for (i, &(dy, dx)) in order.iter().enumerate() {
        let k = s.ac[p].block(i).len();
        let b = blocks.at(bx0 + dx, by0 + dy);
        blocks.ncoef[b] = k as u8;
        nc[dy * n + dx] = k as u32;
        dc[dy * n + dx] = res.dc[i];
    }
    let (plane, at) = (&mut f.planes[p], (bx0 * 8, by0 * 8));
    if res.wide {
        add_grid(plane, at, n, &mut s.grid_wide[p], filter_a, &nc, &dc);
    } else {
        add_grid(plane, at, n, &mut s.grid[p], filter_a, &nc, &dc);
    }
}

fn add_grid<T: Coef>(
    plane: &mut PlaneRows,
    (x0, y0): (usize, usize),
    n: usize,
    grid: &mut [T],
    filter_a: EdgeDirs,
    nc: &[u32; 16],
    dc: &[i64; 16],
) {
    let w = 8 * n;
    if filter_a.any() {
        filter_a_residual(grid, w, filter_a, |(x, y)| nc[y * n + x], |(x, y)| dc[y * n + x]);
    }
    for (yy, g) in grid.chunks_exact(w).enumerate() {
        let row = plane.at(x0, y0 + yy);
        T::add_clamped(&mut plane.px[row..row + w], g);
    }
}

/// For traces: the `n` x `n` pixels from (x0, y0).
fn pred(p: &PlaneRows, x0: usize, y0: usize, n: usize) -> Vec<u8> {
    (0..n * n).map(|k| p.px[p.at(x0 + k % n, y0 + k / n)]).collect()
}

/// P23: the DC a non-intra block offers intra neighbours. Each column's 8 pixels are halved pairwise
/// three times with round-half-up, and the 8 column values are summed (8 x mean). For `n` blocks side
/// by side whose 8 pixel rows are `rows`.
pub(crate) fn avg_dc_row(rows: [&[u8]; 8], n: usize, out: &mut [i32]) {
    for (b, o) in out.iter_mut().enumerate().take(n) {
        *o = (0..8)
            .map(|xx| {
                let mut col: [i32; 8] = std::array::from_fn(|yy| rows[yy][b * 8 + xx] as i32);
                let mut m = 8;
                while m > 1 {
                    for i in 0..m / 2 {
                        col[i] = (col[2 * i] + col[2 * i + 1] + 1) >> 1;
                    }
                    m /= 2;
                }
                col[0]
            })
            .sum();
    }
}

/// One inter-frame macroblock. `list` is the row's block-type list.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_mb(
    r: &mut Bits,
    ctx: &mut SliceCtx,
    s: &mut Scratch,
    f: &mut Rows,
    refs: &[Plane; 3],
    mbx: usize,
    mby: usize,
    list: &mut [MbType; 4],
) -> Result<MbType, Error> {
    let tracing = ctx.trace == Some((mbx, mby));
    let bit_start = r.pos;
    let i = read_block_type(r);
    let typ = list[i];
    if i > 0 {
        list.swap(i - 1, i);
    }
    let (qx0, qy0, top) = (mbx * 2, mby * 2, ctx.y0 * 2);
    let quads: [usize; 4] = std::array::from_fn(|k| ctx.qi(qx0 + k % 2, qy0 + k / 2));
    let qi = |k: usize| quads[k];
    if typ == MbType::Intra {
        // P23: an intra macroblock keeps its predicted MVs
        for k in mv_order(qy0, top) {
            ctx.mv[qi(k)] = predict_mv(ctx, qx0 + k % 2, qy0 + k / 2);
        }
        intra::decode_mb(r, ctx, s, f, mbx, mby)?;
        return Ok(typ);
    }
    let mvs: [Mv; 4] = if typ == MbType::Skip {
        [[0, 0]; 4]
    } else if r.bit() == 1 {
        let d = [read_mv_component(r)?, read_mv_component(r)?];
        [add(predict_mv(ctx, qx0, qy0), d)?; 4]
    } else {
        let mut xs = [0; 4];
        for x in xs.iter_mut() {
            *x = read_mv_component(r)?;
        }
        let mut ys = [0; 4];
        for y in ys.iter_mut() {
            *y = read_mv_component(r)?;
        }
        for k in mv_order(qy0, top) {
            ctx.mv[qi(k)] = add(predict_mv(ctx, qx0 + k % 2, qy0 + k / 2), [xs[k], ys[k]])?;
        }
        std::array::from_fn(|k| ctx.mv[qi(k)])
    };
    for (k, &mv) in mvs.iter().enumerate() {
        ctx.mv[qi(k)] = mv;
    }
    // the coded MV predicts later MVs; the edge-adjusted one moves the pixels (P22)
    for (k, mv) in mvs.iter().enumerate() {
        let (qx, qy) = (k % 2, k / 2);
        let (x0, y0) = (mbx * 32 + qx * 16, mby * 32 + qy * 16);
        let m = [
            edge_mv(x0 as i32, mv[0], ctx.bound_w, ctx.mv_clamp),
            edge_mv(y0 as i32, mv[1], ctx.bound_h, ctx.mv_clamp),
        ];
        let [y, u, v] = &mut f.planes;
        mc_luma(y, &refs[0], x0, y0, m);
        for (dst, src) in [(u, &refs[1]), (v, &refs[2])] {
            mc_chroma(dst, src, mbx * 16 + qx * 8, mby * 16 + qy * 8, m);
        }
    }
    for (p, n) in [(0, 4), (1, 2), (2, 2)] {
        let b = &mut f.blocks[p];
        for by in mby * n..(mby + 1) * n {
            for bx in mbx * n..(mbx + 1) * n {
                let i = b.at(bx, by);
                (b.ncoef[i], b.intra[i]) = (0, false);
            }
        }
    }
    if tracing && typ != MbType::Inter {
        let (y0, x0) = (mby * 32, mbx * 32);
        f.trace = Some(MbTrace {
            typ: Some(typ),
            bit_start,
            mvs,
            pred: pred(&f.planes[0], x0, y0, 32),
            pred_c: [1, 2].map(|p| pred(&f.planes[p], mbx * 16, mby * 16, 16)),
            res_dc: vec![0; 16],
            res_ac: vec![vec![]; 16],
            res_px: vec![[0; 64]; 16],
            ..MbTrace::default()
        });
    }
    if typ == MbType::Inter {
        // P20/P21: dq, luma CBP, luma DC flag, luma blocks, chroma flag, [U, V]; added once all are read
        let mi = ctx.mi(mbx, mby);
        let q = ctx.rq_pred(mbx, mby) + read_dq(r);
        ctx.rq[mi] = q;
        f.set_q(mbx, mby, q);
        let cbp = read_luma_cbp(r, ctx.left(&ctx.icbp, mbx, mby));
        ctx.icbp[mi] = cbp;
        let dc_flag = r.bit() == 1;
        let luma = residual_plane(r, s, 0, cbp, dc_flag, q)?;
        let mut chroma = None;
        if r.bit() == 1 {
            let prev = ctx.left(&ctx.iccbp, mbx, mby);
            let mut cc = [0u32; 2];
            let mut dcs: [ResidualDc; 2] = Default::default();
            for k in 0..2 {
                cc[k] = intra::read_chroma_cbp(r, prev[k]);
                let cdc = r.bit() == 1;
                dcs[k] = residual_plane(r, s, k + 1, cc[k], cdc, q)?;
            }
            ctx.iccbp[mi] = cc;
            chroma = Some(dcs);
        }
        if tracing {
            let (y0, x0) = (mby * 32, mbx * 32);
            let g = |k: usize| if luma.wide { s.grid_wide[0][k] } else { s.grid[0][k] as i64 };
            f.trace = Some(MbTrace {
                typ: Some(typ),
                bit_start,
                mvs,
                q,
                cbp,
                dc_flag,
                pred: pred(&f.planes[0], x0, y0, 32),
                pred_c: [1, 2].map(|p| pred(&f.planes[p], mbx * 16, mby * 16, 16)),
                res_dc: luma.dc.to_vec(),
                res_dc_level: luma.level.to_vec(),
                res_dc_delta: luma.delta.to_vec(),
                res_ac: s.ac[0].blocks(16),
                res_px: ZORD
                    .iter()
                    .map(|&(dy, dx)| std::array::from_fn(|k| g((dy * 8 + k / 8) * 32 + dx * 8 + k % 8)))
                    .collect(),
                chroma: chroma
                    .iter()
                    .flatten()
                    .enumerate()
                    .map(|(k, res)| (ctx.iccbp[mi][k], res.level[..4].to_vec(), s.ac[k + 1].blocks(4)))
                    .collect(),
            });
        }
        add_residual(f, 0, mbx, mby, &luma, s, ctx.filter_a);
        for (k, res) in chroma.iter().flatten().enumerate() {
            add_residual(f, k + 1, mbx, mby, res, s, ctx.filter_a);
        }
    }
    // non-intra blocks: the DC later intra neighbours predict from (P23)
    for (p, n) in [(0, 4), (1, 2), (2, 2)] {
        let (plane, b) = (&f.planes[p], &mut f.blocks[p]);
        for by in mby * n..(mby + 1) * n {
            let rows: [&[u8]; 8] = std::array::from_fn(|yy| {
                let o = plane.at(mbx * n * 8, by * 8 + yy);
                &plane.px[o..o + 8 * n]
            });
            let i = b.at(mbx * n, by);
            if !simd::avg_dc_row(rows, n, &mut b.dc[i..i + n]) {
                avg_dc_row(rows, n, &mut b.dc[i..i + n]);
            }
        }
    }
    Ok(typ)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_mv_follows_p22() {
        // whole-pel, inside / beyond the far edge / negative start
        assert_eq!(edge_mv(32, 8, 128, false), 8);
        assert_eq!(edge_mv(96, 32, 128, false), 32, "s = n - 16 is still inside");
        assert_eq!(edge_mv(96, 34, 128, false), 2 * (128 - 16 - 96));
        assert_eq!(edge_mv(0, -4, 128, false), 2 * (128 - 16));
        // half-pel: inside, near the start, at and beyond the end
        assert_eq!(edge_mv(32, 9, 128, false), 9);
        assert_eq!(edge_mv(0, 1, 128, false), 4);
        assert_eq!(edge_mv(96, 25, 128, false), 25);
        assert_eq!(edge_mv(96, 27, 128, false), 2 * (128 - 21 - 96));
    }
}
