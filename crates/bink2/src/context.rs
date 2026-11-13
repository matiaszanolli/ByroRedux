//! Cross-macroblock state of one slice (P16/P17/P21-P23/P26). A slice starts from nothing: every map
//! is empty again at its first macroblock row.

use crate::filters::EdgeDirs;
use crate::frame::MbType;
use crate::intra::AcList;

/// Buffers a macroblock decodes into, reused from one macroblock to the next: the coded AC and the
/// INTER residual (32 x 32 luma, 16 x 16 per chroma plane) of each plane, in i32 or, for planes with a
/// coefficient beyond `idct::NARROW`, i64.
pub(crate) struct Scratch {
    pub ac: [AcList; 3],
    pub grid: [Vec<i32>; 3],
    pub grid_wide: [Vec<i64>; 3],
}

impl Scratch {
    pub fn new() -> Self {
        let sizes = [32 * 32, 16 * 16, 16 * 16];
        Self { ac: Default::default(), grid: sizes.map(|n| vec![0; n]), grid_wide: sizes.map(|n| vec![0; n]) }
    }
}

/// Marks a macroblock that has no quantiser yet (above the slice, or not decoded).
const NONE: i32 = i32::MIN;

pub(crate) struct SliceCtx {
    pub mbw: usize,
    /// First macroblock row of the slice.
    pub y0: usize,
    /// Intra quantiser per macroblock.
    pub q: Vec<i32>,
    /// Intra luma / chroma CBP histories.
    pub cbp: Vec<u32>,
    pub ccbp: Vec<[u32; 2]>,
    /// Residual (INTER) quantiser and CBP histories (P23).
    pub rq: Vec<i32>,
    pub icbp: Vec<u32>,
    pub iccbp: Vec<[u32; 2]>,
    /// Final MV per 16x16 luma quadrant of the slice (`qi`).
    pub mv: Vec<[i32; 2]>,
    qw: usize,
    /// P22: MV edge bounds — the padded width, the height rounded up to 16.
    pub bound_w: i32,
    pub bound_h: i32,
    /// P25/P31: the edges filter A runs on inside each INTER residual.
    pub filter_a: EdgeDirs,
    /// P30: frame flag bit 17 — MVs leaving the frame are clamped (sub-pel kept) instead of P22's rule.
    pub mv_clamp: bool,
    /// Macroblock to record in `Frame::trace`.
    pub trace: Option<(usize, usize)>,
}

#[inline]
fn median(a: i32, b: i32, c: i32) -> i32 {
    a.min(b).max(a.max(b).min(c))
}

impl SliceCtx {
    /// The state of a slice over macroblock rows `y0..y1` (its maps cover only them).
    pub fn new(mbw: usize, y0: usize, y1: usize) -> Self {
        let n = mbw * (y1 - y0);
        Self {
            mbw,
            y0,
            q: vec![NONE; n],
            cbp: vec![0; n],
            ccbp: vec![[0; 2]; n],
            rq: vec![NONE; n],
            icbp: vec![0; n],
            iccbp: vec![[0; 2]; n],
            mv: vec![[0; 2]; n * 4],
            qw: mbw * 2,
            bound_w: 0,
            bound_h: 0,
            filter_a: EdgeDirs::default(),
            mv_clamp: false,
            trace: None,
        }
    }

    /// The index of macroblock (mbx, mby) in the per-macroblock maps.
    #[inline]
    pub fn mi(&self, mbx: usize, mby: usize) -> usize {
        (mby - self.y0) * self.mbw + mbx
    }

    /// The index of luma quadrant (qx, qy) in `mv`.
    #[inline]
    pub fn qi(&self, qx: usize, qy: usize) -> usize {
        (qy - 2 * self.y0) * self.qw + qx
    }

    /// P17: median of left, top and top-left; 16 for the slice's first macroblock; one neighbour alone
    /// is taken as is.
    fn median_pred(&self, m: &[i32], mbx: usize, mby: usize) -> i32 {
        let i = self.mi(mbx, mby);
        let l = if mbx > 0 { m[i - 1] } else { NONE };
        let t = if mby > self.y0 { m[i - self.mbw] } else { NONE };
        match (l, t) {
            (NONE, NONE) => 16,
            (l, NONE) => l,
            (NONE, t) => t,
            (l, t) => median(l, t, m[i - self.mbw - 1]),
        }
    }

    pub fn q_pred(&self, mbx: usize, mby: usize) -> i32 {
        self.median_pred(&self.q, mbx, mby)
    }

    /// P23: the residual quantiser predicts the same way from its own map.
    pub fn rq_pred(&self, mbx: usize, mby: usize) -> i32 {
        self.median_pred(&self.rq, mbx, mby)
    }

    /// P17/P23: CBP histories are the left macroblock's; nothing at the start of a row.
    #[inline]
    pub fn left<T: Copy + Default>(&self, v: &[T], mbx: usize, mby: usize) -> T {
        if mbx > 0 {
            v[self.mi(mbx, mby) - 1]
        } else {
            T::default()
        }
    }

    /// P23/P26/P28, inter frames: a macroblock that codes no quantiser of a kind keeps the prediction it
    /// would have used; one that codes no CBP of a kind keeps its left neighbour's — intra luma and
    /// chroma (non-INTRA), residual luma and chroma (non-INTER). An INTER macroblock without chroma
    /// resets the residual chroma CBP.
    pub fn hold(&mut self, mbx: usize, mby: usize, typ: MbType) {
        let i = self.mi(mbx, mby);
        if typ != MbType::Intra {
            self.q[i] = self.q_pred(mbx, mby);
            self.cbp[i] = self.left(&self.cbp, mbx, mby);
            self.ccbp[i] = self.left(&self.ccbp, mbx, mby);
        }
        if typ != MbType::Inter {
            self.rq[i] = self.rq_pred(mbx, mby);
            self.icbp[i] = self.left(&self.icbp, mbx, mby);
            self.iccbp[i] = self.left(&self.iccbp, mbx, mby);
        }
    }
}
