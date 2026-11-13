//! Decoded planes and the per-block state the post-filters and the next macroblocks read.

/// One 8-bit plane at its padded size (luma: width and height rounded up to 32; chroma: half that).
#[derive(Clone)]
pub struct Plane {
    pub width: usize,
    pub height: usize,
    pub px: Vec<u8>,
}

impl Plane {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, px: vec![0; width * height] }
    }
}

/// Marks a block whose DC has not been decoded (yet) in this frame.
pub(crate) const NO_DC: i32 = i32::MIN;

/// Per 8x8 block of one plane.
pub(crate) struct Blocks {
    pub bw: usize,
    /// The DC intra neighbours predict from: decoded (intra, 8 x mean, 0..2047) or the P23 column
    /// cascade of the block's pixels (non-intra).
    pub dc: Vec<i32>,
    /// Non-zero AC coefficients (intra: of the block; INTER: of its residual; MOTION/SKIP: 0).
    pub ncoef: Vec<u8>,
    pub intra: Vec<bool>,
}

impl Blocks {
    fn new(bw: usize, bh: usize) -> Self {
        let n = bw * bh;
        Self { bw, dc: vec![NO_DC; n], ncoef: vec![0; n], intra: vec![true; n] }
    }

    fn reset(&mut self) {
        self.dc.fill(NO_DC);
        self.ncoef.fill(0);
        self.intra.fill(true);
    }
}

/// One plane's pixel rows from row `y0` on, addressed in frame coordinates (`at`).
pub(crate) struct PlaneRows<'a> {
    pub width: usize,
    y0: usize,
    pub px: &'a mut [u8],
}

impl PlaneRows<'_> {
    /// The index in `px` of pixel (x, y).
    #[inline]
    pub fn at(&self, x: usize, y: usize) -> usize {
        (y - self.y0) * self.width + x
    }
}

/// One plane's per-block state from block row `y0` on, addressed in frame block coordinates (`at`).
pub(crate) struct BlockRows<'a> {
    pub bw: usize,
    y0: usize,
    pub dc: &'a mut [i32],
    pub ncoef: &'a mut [u8],
    pub intra: &'a mut [bool],
}

impl BlockRows<'_> {
    /// The index of block (bx, by) in the per-block arrays.
    #[inline]
    pub fn at(&self, bx: usize, by: usize) -> usize {
        (by - self.y0) * self.bw + bx
    }

    #[inline]
    pub fn dc_at(&self, bx: usize, by: usize) -> Option<i64> {
        match self.dc[self.at(bx, by)] {
            NO_DC => None,
            v => Some(v as i64),
        }
    }
}

/// The macroblock rows of a frame that one slice decodes: every per-row array of the frame cut to
/// them, addressed in frame coordinates. Slices write nothing outside their rows (P17/P22: no
/// prediction crosses a slice's top), so they can be decoded at the same time.
pub(crate) struct Rows<'a> {
    mbw: usize,
    /// First macroblock row.
    mb0: usize,
    pub planes: [PlaneRows<'a>; 3],
    pub blocks: [BlockRows<'a>; 3],
    qmv: &'a mut [[i32; 2]],
    mb_types: &'a mut [Option<MbType>],
    mb_bits: &'a mut [usize],
    mb_q: &'a mut [i32],
    pub trace: Option<MbTrace>,
}

impl Rows<'_> {
    #[inline]
    fn mi(&self, mbx: usize, mby: usize) -> usize {
        (mby - self.mb0) * self.mbw + mbx
    }

    /// A decoded macroblock: its type and the bit position after it.
    pub fn set_mb(&mut self, mbx: usize, mby: usize, typ: MbType, bits: usize) {
        let i = self.mi(mbx, mby);
        (self.mb_types[i], self.mb_bits[i]) = (Some(typ), bits);
    }

    pub fn set_q(&mut self, mbx: usize, mby: usize, q: i32) {
        let i = self.mi(mbx, mby);
        self.mb_q[i] = q;
    }

    /// The final MV of luma quadrant (qx, qy).
    pub fn set_qmv(&mut self, qx: usize, qy: usize, mv: [i32; 2]) {
        self.qmv[(qy - 2 * self.mb0) * 2 * self.mbw + qx] = mv;
    }
}

/// `v` (rows of `per_row` items) cut at the slices' row ranges, which run consecutively from row 0.
fn cut<'a, T>(mut v: &'a mut [T], per_row: usize, slices: &[(usize, usize)]) -> Vec<&'a mut [T]> {
    let mut out = Vec::with_capacity(slices.len());
    for &(y0, y1) in slices {
        let (a, b) = std::mem::take(&mut v).split_at_mut((y1 - y0) * per_row);
        out.push(a);
        v = b;
    }
    out
}

/// Macroblock types (P20). Keyframe macroblocks are `Key`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MbType {
    Key,
    Motion,
    Inter,
    Skip,
    Intra,
}

impl MbType {
    pub fn name(self) -> &'static str {
        match self {
            MbType::Key => "KEY",
            MbType::Motion => "MOTION",
            MbType::Inter => "INTER",
            MbType::Skip => "SKIP",
            MbType::Intra => "INTRA",
        }
    }
}

/// One decoded frame: Y, U, V at padded size, plus what the decoder recorded on the way.
pub struct Frame {
    /// Visible size.
    pub width: usize,
    pub height: usize,
    /// Macroblocks (32x32 luma) per row and column.
    pub mbw: usize,
    pub mbh: usize,
    pub planes: [Plane; 3],
    pub(crate) blocks: [Blocks; 3],
    /// Final MV of every 16x16 luma quadrant, (mbw * 2) per row (inter frames; zero in keyframes).
    pub(crate) qmv: Vec<[i32; 2]>,
    /// P32: the block lines filter A runs on (KB2j flag bit 16).
    pub(crate) lines: crate::filters::EdgeLines,
    /// Type of every macroblock decoded; `None` where decoding stopped before it.
    pub mb_types: Vec<Option<MbType>>,
    /// Bit position (from packet byte 8) after each decoded macroblock.
    pub mb_bits: Vec<usize>,
    /// Quantiser each macroblock coded: intra q, or the residual q of INTER (`i32::MIN` for none).
    pub mb_q: Vec<i32>,
    /// What the macroblock named in `Options::trace` decoded (inter frames).
    pub trace: Option<MbTrace>,
}

/// One chroma plane of a traced INTER residual: CBP, DC levels, AC (scan index, level) per block.
pub type ChromaTrace = (u32, Vec<i32>, Vec<Vec<(u8, i32)>>);

/// Diagnostics for one macroblock. Intra macroblocks fill `bit_start` (after the CBP and DC flag), `q`,
/// `cbp`, `dc_flag`, `res_dc` (the blocks' final DCs), `res_dc_level` and `res_ac`.
#[derive(Clone, Debug, Default)]
pub struct MbTrace {
    pub typ: Option<MbType>,
    /// Bit position where the macroblock starts.
    pub bit_start: usize,
    /// Coded MVs per quadrant (before the P22 edge adjustment).
    pub mvs: [[i32; 2]; 4],
    /// INTER: residual quantiser, luma CBP, luma DC flag.
    pub q: i32,
    pub cbp: u32,
    pub dc_flag: bool,
    /// Luma prediction (32x32) before the residual.
    pub pred: Vec<u8>,
    /// U and V predictions (16x16 each) before the residual.
    pub pred_c: [Vec<u8>; 2],
    /// Luma residual per block in coding order (ZORD): DC, AC as (scan index, level), pixels.
    pub res_dc: Vec<i64>,
    /// Coded DC levels (before dequantisation) and their dequantised values.
    pub res_dc_level: Vec<i32>,
    pub res_dc_delta: Vec<i64>,
    pub res_ac: Vec<Vec<(u8, i32)>>,
    pub res_px: Vec<[i64; 64]>,
    /// INTER chroma residual, U then V (empty without the chroma flag): CBP, DC levels, AC per block.
    pub chroma: Vec<ChromaTrace>,
}

impl Frame {
    /// The final MV (half pixels) of luma quadrant (qx, qy), 16x16 pixels each (inter frames).
    pub fn quadrant_mv(&self, qx: usize, qy: usize) -> [i32; 2] {
        self.qmv[qy * self.mbw * 2 + qx]
    }

    pub fn new(width: usize, height: usize) -> Self {
        let (pw, ph) = (width.div_ceil(32) * 32, height.div_ceil(32) * 32);
        let (mbw, mbh) = (pw / 32, ph / 32);
        Self {
            width,
            height,
            mbw,
            mbh,
            planes: [Plane::new(pw, ph), Plane::new(pw / 2, ph / 2), Plane::new(pw / 2, ph / 2)],
            blocks: [
                Blocks::new(pw / 8, ph / 8),
                Blocks::new(pw / 16, ph / 16),
                Blocks::new(pw / 16, ph / 16),
            ],
            qmv: vec![[0, 0]; mbw * 2 * mbh * 2],
            lines: Default::default(),
            mb_types: vec![None; mbw * mbh],
            mb_bits: vec![0; mbw * mbh],
            mb_q: vec![i32::MIN; mbw * mbh],
            trace: None,
        }
    }

    /// The frame cut into the macroblock rows of each slice (`slices`: consecutive row ranges from 0).
    pub(crate) fn rows(&mut self, slices: &[(usize, usize)]) -> Vec<Rows<'_>> {
        let Frame { mbw, planes, blocks, qmv, mb_types, mb_bits, mb_q, .. } = self;
        let mbw = *mbw;
        let mut pr: Vec<_> = planes
            .iter_mut()
            .zip([32, 16, 16])
            .map(|(p, n)| {
                let width = p.width;
                cut(&mut p.px, n * width, slices)
                    .into_iter()
                    .zip(slices)
                    .map(|(px, &(y0, _))| PlaneRows { width, y0: y0 * n, px })
                    .collect::<Vec<_>>()
                    .into_iter()
            })
            .collect();
        let mut br: Vec<_> = blocks
            .iter_mut()
            .zip([4, 2, 2])
            .map(|(b, n)| {
                let bw = b.bw;
                let (dc, ncoef) = (cut(&mut b.dc, n * bw, slices), cut(&mut b.ncoef, n * bw, slices));
                dc.into_iter()
                    .zip(ncoef)
                    .zip(cut(&mut b.intra, n * bw, slices))
                    .zip(slices)
                    .map(|(((dc, ncoef), intra), &(y0, _))| BlockRows { bw, y0: y0 * n, dc, ncoef, intra })
                    .collect::<Vec<_>>()
                    .into_iter()
            })
            .collect();
        let (types, bits) = (cut(mb_types, mbw, slices), cut(mb_bits, mbw, slices));
        cut(qmv, 4 * mbw, slices)
            .into_iter()
            .zip(types)
            .zip(bits)
            .zip(cut(mb_q, mbw, slices))
            .zip(slices)
            .map(|((((qmv, mb_types), mb_bits), mb_q), &(mb0, _))| Rows {
                mbw,
                mb0,
                planes: std::array::from_fn(|k| pr[k].next().unwrap()),
                blocks: std::array::from_fn(|k| br[k].next().unwrap()),
                qmv,
                mb_types,
                mb_bits,
                mb_q,
                trace: None,
            })
            .collect()
    }

    /// Everything as `new` leaves it except the planes, which every decoded frame overwrites whole (each
    /// macroblock writes all its pixels): a frame buffer reused for the next frame.
    pub(crate) fn reset(&mut self) {
        self.blocks.iter_mut().for_each(Blocks::reset);
        self.qmv.fill([0, 0]);
        self.lines = Default::default();
        self.mb_types.fill(None);
        self.mb_bits.fill(0);
        self.mb_q.fill(i32::MIN);
        self.trace = None;
    }
}
