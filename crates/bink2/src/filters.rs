//! Post-filters (P15, P19, P24, P25, P29, P31). Both run on luma and chroma alike: filter B (threshold
//! = 8 x the frame flags' low byte) over DC-only intra blocks, then filter A on the block edges — the
//! vertical ones unless flag bit 14, the horizontal ones unless flag bit 15.

use crate::frame::Frame;
use crate::idct::Coef;

type Block = (usize, usize);

/// Which block edges filter A runs on (P31): vertical ones unless frame flag bit 14, horizontal ones
/// unless bit 15 (real files set both or neither).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct EdgeDirs {
    pub vertical: bool,
    pub horizontal: bool,
}

impl EdgeDirs {
    pub fn from_flags(flags: u32) -> Self {
        Self { vertical: flags & 0x4000 == 0, horizontal: flags & 0x8000 == 0 }
    }

    pub fn any(self) -> bool {
        self.vertical || self.horizontal
    }
}

/// P32: per block line, whether filter A runs on it (KB2j frames with flag bit 16 code a map; empty =
/// every line). Indexed by luma block column (vertical edges, `v`) and block row (horizontal, `h`).
#[derive(Clone, Debug, Default)]
pub(crate) struct EdgeLines {
    pub v: Vec<bool>,
    pub h: Vec<bool>,
}

impl EdgeLines {
    /// Whether the edge line at block index `i` of a plane (`scale` luma lines per plane line) is on.
    fn on(&self, vertical: bool, i: usize, scale: usize) -> bool {
        let m = if vertical { &self.v } else { &self.h };
        m.is_empty() || m.get(i * scale).copied().unwrap_or(false)
    }
}

/// Filter A over the 8x8 block edges of a plane `w` pixels wide whose pixel rows from `y0` on are `px`
/// (P19, P24; P32: only on the lines that are on): first the vertical edges of block rows `vrows`, then
/// the horizontal edges on block lines `hlines` (a line touches the 2 pixel rows either side of it).
/// `rule` decides per pair of blocks (left/top first) whether their edge is filtered and with which
/// coefficient counts; `edge` filters one block edge: 8 positions from q0 index `o`, `along` apart, the
/// pixels across it `across` apart. Inside a direction the edges touch disjoint pixels, so their order
/// does not matter; across directions it does.
#[allow(clippy::too_many_arguments)]
fn filter_edges<T>(
    px: &mut [T],
    (w, y0): (usize, usize),
    vrows: std::ops::Range<usize>,
    hlines: std::ops::Range<usize>,
    (lines, scale): (&EdgeLines, usize),
    rule: impl Fn(Block, Block) -> Option<(u32, u32)>,
    edge: impl Fn(&mut [T], usize, usize, usize, u32, u32),
) {
    let bw = w / 8;
    let on: Vec<usize> = (1..bw).filter(|&bx| lines.on(true, bx, scale)).collect();
    for by in vrows {
        for &bx in &on {
            if let Some((na, nb)) = rule((bx - 1, by), (bx, by)) {
                edge(px, (by * 8 - y0) * w + bx * 8, w, 1, na, nb);
            }
        }
    }
    for by in hlines.filter(|&by| lines.on(false, by, scale)) {
        for bx in 0..bw {
            if let Some((na, nb)) = rule((bx, by - 1), (bx, by)) {
                edge(px, (by * 8 - y0) * w + bx * 8, 1, w, na, nb);
            }
        }
    }
}

/// P19: with step = q0 - p0, a side with fewer than 8 AC coefficients moves its edge pixel by
/// (step + 2) >> 2 towards the other side, a side with at most 3 also the next pixel by (step + 4) >> 3.
#[inline]
fn moves<T: Coef>(step: T, na: u32, nb: u32, mut apply: impl FnMut(usize, T)) {
    let (d0, d1) = ((step + T::of(2)) >> 2, (step + T::of(4)) >> 3);
    if na < 8 {
        apply(1, d0);
    }
    if na <= 3 {
        apply(0, d1);
    }
    if nb < 8 {
        apply(2, T::default() - d0);
    }
    if nb <= 3 {
        apply(3, T::default() - d1);
    }
}

/// One block edge of pixels (`filter_edges`), clamped to 0..255.
pub(crate) fn edge_u8(px: &mut [u8], o: usize, along: usize, across: usize, na: u32, nb: u32) {
    if along == 1 && crate::simd::hedge_u8(px, o, across, na, nb) {
        return;
    }
    for q0 in (o..).step_by(along).take(8) {
        let e = [q0 - 2 * across, q0 - across, q0, q0 + across];
        let step = px[e[2]] as i32 - px[e[1]] as i32;
        moves(step, na, nb, |k, d| px[e[k]] = (px[e[k]] as i32 + d).clamp(0, 255) as u8);
    }
}

/// P25: filter A on the residual of one INTER macroblock (`w` x `w`, `w` = 8 x blocks per side), inner
/// edges only, before the prediction is added; nothing is clamped. Two blocks without AC are filtered
/// only when their residual DCs differ by more than 32.
pub(crate) fn filter_a_residual<T: Coef>(
    g: &mut [T],
    w: usize,
    dirs: EdgeDirs,
    ncoef: impl Fn(Block) -> u32,
    dc: impl Fn(Block) -> i64,
) {
    let rule = |a, b| {
        let (na, nb) = (ncoef(a), ncoef(b));
        (na != 0 || nb != 0 || (dc(b) - dc(a)).abs() > 32).then_some((na, nb))
    };
    let n = w / 8;
    let vrows = if dirs.vertical { 0..n } else { 0..0 };
    let hlines = if dirs.horizontal { 1..n } else { 0..0 };
    filter_edges(
        g,
        (w, 0),
        vrows,
        hlines,
        (&EdgeLines::default(), 1),
        rule,
        |g: &mut [T], o, along, across, na, nb| {
            for q0 in (o..).step_by(along).take(8) {
                let e = [q0 - 2 * across, q0 - across, q0, q0 + across];
                let step = g[e[2]] - g[e[1]];
                moves(step, na, nb, |k, d| g[e[k]] = g[e[k]] + d);
            }
        },
    );
}

/// What the frame filters read of one plane: its blocks' state and the frame's MVs.
struct PlaneInfo<'a> {
    w: usize,
    h: usize,
    bw: usize,
    ncoef: &'a [u8],
    intra: &'a [bool],
    dc: &'a [i32],
    qmv: &'a [[i32; 2]],
    qw: usize,
    luma: bool,
}

impl PlaneInfo<'_> {
    #[inline]
    fn ncoef(&self, (x, y): Block) -> u32 {
        self.ncoef[y * self.bw + x] as u32
    }

    #[inline]
    fn intra(&self, (x, y): Block) -> bool {
        self.intra[y * self.bw + x]
    }

    /// Luma blocks take their quadrant's MV, chroma blocks the co-located quadrant's.
    #[inline]
    fn mv(&self, (x, y): Block) -> [i32; 2] {
        if self.luma {
            self.qmv[(y / 2) * self.qw + x / 2]
        } else {
            self.qmv[y * self.qw + x]
        }
    }

    /// P19/P25: filter A on a plane. Edges between two intra blocks are filtered unless neither has
    /// AC; between an intra and a non-intra block only the intra side moves (by its own rule); between
    /// two non-intra blocks the edge is filtered at full strength on both sides when their MVs differ
    /// by 2 or more half pixels in either component, and left alone otherwise. Keyframes are all intra.
    fn rule(&self, a: Block, b: Block) -> Option<(u32, u32)> {
        match (self.intra(a), self.intra(b)) {
            (true, true) => {
                let (na, nb) = (self.ncoef(a), self.ncoef(b));
                (na != 0 || nb != 0).then_some((na, nb))
            }
            (ia, ib) if ia || ib => {
                Some((if ia { self.ncoef(a) } else { 64 }, if ib { self.ncoef(b) } else { 64 }))
            }
            _ => {
                let (ma, mb) = (self.mv(a), self.mv(b));
                let far = |k: usize| (ma[k] as i64 - mb[k] as i64).abs() >= 2;
                (far(0) || far(1)).then_some((0, 0))
            }
        }
    }

    /// Filter B's smoothable blocks: intra and DC-only.
    #[inline]
    fn smooth(&self, k: Block) -> bool {
        self.intra(k) && self.ncoef(k) == 0
    }
}

/// P24: weight /128 of the left (top) neighbour at x (y) = 0..7.
pub(crate) const W_B: [i32; 8] = [52, 36, 24, 14, 8, 4, 1, 0];

/// P24: one 1-D step: own value plus floored corrections towards both neighbours (DC units).
#[inline]
fn blend(c: i32, lo: i32, hi: i32, k: usize) -> i32 {
    c + ((W_B[k] * (lo - c)) >> 7) + ((W_B[7 - k] * (hi - c)) >> 7)
}

/// P24: a smoothed block's pixels from the values of its 3x3 neighbourhood (`g[row][column]`, DC units):
/// a horizontal pass on each of the three rows, then a vertical pass on the three results; pixel =
/// (v + 4) >> 3, clamped.
pub(crate) fn smooth_block(g: &[[i32; 3]; 3], out: &mut [[u8; 8]; 8]) {
    if crate::simd::smooth_block(g, out) {
        return;
    }
    let rows: [[i32; 8]; 3] =
        std::array::from_fn(|j| std::array::from_fn(|x| blend(g[j][1], g[j][0], g[j][2], x)));
    for (y, row) in out.iter_mut().enumerate() {
        for (x, v) in row.iter_mut().enumerate() {
            *v = ((blend(rows[1][x], rows[0][x], rows[2][x], y) + 4) >> 3).clamp(0, 255) as u8;
        }
    }
}

/// Filter B, exact (P24), on block rows `rows` of a plane whose pixel rows from `y0` on are `px`.
/// Rewrites every DC-only intra block from the DCs (1/8 pixel) of its 3x3 block neighbourhood. A
/// neighbour is usable when it is inside the plane, DC-only and its DC differs from the block's by
/// less than `thr` (P29: 8 x the flags' low byte). An unusable left or right neighbour counts as the
/// block's own DC; an unusable neighbour in the row above or below takes the (substituted) value of the
/// middle row in its column.
fn filter_b(px: &mut [u8], y0: usize, info: &PlaneInfo, rows: std::ops::Range<usize>, thr: i32) {
    let (bw, bh, w) = (info.bw, info.h / 8, info.w);
    let dc = |(x, y): Block| info.dc[y * bw + x];
    for by in rows {
        for bx in 0..bw {
            if !info.smooth((bx, by)) {
                continue;
            }
            let c = dc((bx, by));
            let usable = |i: isize, j: isize| -> Option<i32> {
                let (nx, ny) = (bx.checked_add_signed(i)?, by.checked_add_signed(j)?);
                if nx >= bw || ny >= bh || !info.smooth((nx, ny)) {
                    return None;
                }
                let v = dc((nx, ny));
                ((v - c).abs() < thr).then_some(v)
            };
            let mut mid = [c; 3];
            for i in [-1, 1] {
                if let Some(v) = usable(i, 0) {
                    mid[(i + 1) as usize] = v;
                }
            }
            let mut g = [mid; 3];
            for j in [-1, 1] {
                for i in [-1, 0, 1] {
                    if let Some(v) = usable(i, j) {
                        g[(j + 1) as usize][(i + 1) as usize] = v;
                    }
                }
            }
            let mut out = [[0u8; 8]; 8];
            smooth_block(&g, &mut out);
            for (y, row) in out.iter().enumerate() {
                let o = (by * 8 + y - y0) * w + bx * 8;
                px[o..o + 8].copy_from_slice(row);
            }
        }
    }
}

/// One piece of post-filter work: a band of a plane's pixel rows from `y0` on, and the block rows (pass
/// 1: filter B and vertical edges) or block lines (pass 2: horizontal edges) it covers.
struct Band<'a> {
    plane: usize,
    px: &'a mut [u8],
    y0: usize,
    blocks: std::ops::Range<usize>,
}

/// `jobs` on up to `threads` threads, each taking the next job as it finishes one.
fn run<J: Send>(jobs: Vec<J>, threads: usize, work: impl Fn(J) + Sync) {
    if threads <= 1 || jobs.len() <= 1 {
        jobs.into_iter().for_each(work);
        return;
    }
    let queue = std::sync::Mutex::new(jobs.into_iter());
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| loop {
                let Some(job) = queue.lock().unwrap().next() else { break };
                work(job);
            });
        }
    });
}

/// `px` (rows of `w`) cut at the given pixel rows (ascending, inside the plane) into consecutive bands.
fn cut_rows<'a>(mut px: &'a mut [u8], w: usize, at: &[usize]) -> Vec<&'a mut [u8]> {
    let (mut out, mut row) = (vec![], 0);
    for &y in at {
        let (a, b) = std::mem::take(&mut px).split_at_mut((y - row) * w);
        out.push(a);
        (px, row) = (b, y);
    }
    out.push(px);
    out
}

/// The frame-level post-filters (P24, P25; the residual part of filter A already ran per macroblock),
/// on up to `threads` threads. Pass 1 runs filter B and the vertical edges of filter A per band of
/// block rows; pass 2 the horizontal edges per band of block lines, each band's pixels starting half a
/// block above its first line — no two bands touch the same pixel in either pass.
pub(crate) fn apply(f: &mut Frame, flags: u32, threads: usize) {
    let thr = 8 * (flags & 0xff) as i32; // P29: the low byte, in pixels; 0 = no filter B
    let dirs = EdgeDirs::from_flags(flags);
    if thr == 0 && !dirs.any() {
        return;
    }
    let qw = f.mbw * 2;
    let Frame { planes, blocks, qmv, lines, .. } = f;
    let infos: Vec<PlaneInfo> = planes
        .iter()
        .zip(blocks.iter())
        .enumerate()
        .map(|(p, (plane, b))| PlaneInfo {
            w: plane.width,
            h: plane.height,
            bw: b.bw,
            ncoef: &b.ncoef,
            intra: &b.intra,
            dc: &b.dc,
            qmv,
            qw,
            luma: p == 0,
        })
        .collect();
    let lines = &*lines;
    let bands = threads.max(1);
    // block-row boundaries of each plane's bands
    let splits = |bh: usize| -> Vec<usize> {
        (1..bands).map(|k| k * bh / bands).filter(|&b| b > 0 && b < bh).collect()
    };
    let mut pass1 = vec![];
    for (p, plane) in planes.iter_mut().enumerate() {
        let (w, bh) = (plane.width, plane.height / 8);
        let at = splits(bh);
        let bounds: Vec<usize> = std::iter::once(0).chain(at.iter().copied()).chain([bh]).collect();
        let rows: Vec<usize> = at.iter().map(|b| b * 8).collect();
        for (k, px) in cut_rows(&mut plane.px, w, &rows).into_iter().enumerate() {
            pass1.push(Band { plane: p, px, y0: bounds[k] * 8, blocks: bounds[k]..bounds[k + 1] });
        }
    }
    run(pass1, threads, |b| {
        let info = &infos[b.plane];
        if thr != 0 {
            filter_b(b.px, b.y0, info, b.blocks.clone(), thr);
        }
        if dirs.vertical {
            let scale = if info.luma { 1 } else { 2 };
            filter_edges(
                b.px,
                (info.w, b.y0),
                b.blocks,
                0..0,
                (lines, scale),
                |a, c| info.rule(a, c),
                edge_u8,
            );
        }
    });
    if !dirs.horizontal {
        return;
    }
    let mut pass2 = vec![];
    for (p, plane) in planes.iter_mut().enumerate() {
        let (w, bh) = (plane.width, plane.height / 8);
        let at = splits(bh);
        let bounds: Vec<usize> = std::iter::once(1).chain(at.iter().copied()).chain([bh]).collect();
        let rows: Vec<usize> = at.iter().map(|b| b * 8 - 4).collect();
        for (k, px) in cut_rows(&mut plane.px, w, &rows).into_iter().enumerate() {
            let y0 = if k == 0 { 0 } else { bounds[k] * 8 - 4 };
            pass2.push(Band { plane: p, px, y0, blocks: bounds[k]..bounds[k + 1].max(bounds[k]) });
        }
    }
    run(pass2, threads, |b| {
        let info = &infos[b.plane];
        let scale = if info.luma { 1 } else { 2 };
        filter_edges(b.px, (info.w, b.y0), 0..0, b.blocks, (lines, scale), |a, c| info.rule(a, c), edge_u8);
    });
}
