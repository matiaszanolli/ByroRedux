//! Clean-room decoder for Bink Video 2 (`KB2g`, `KB2j`): the games' Bink 2 movies (PRD
//! `bink-video-playback`, BIK-DEC-003).
//!
//! Read `CLEANROOM.md` before changing anything here: every rule keeps the tag of the probe it comes
//! from (`docs/spec/probes.md`), and no other Bink decoder's source may be read. The rules were first
//! written in a Python reference that, with the oracle harness and the probing tools, lives in the
//! research repository (`README.md`).
//!
//! [`Bk2`] reads the container, [`Decoder`] turns its video packets into [`Frame`]s: Y, Cr, Cb (the
//! chroma planes at half size; P1c, P33), padded to whole macroblocks; [`Frame::to_rgba`] converts one
//! for display.

mod bits;
mod container;
mod context;
mod filters;
mod frame;
mod idct;
mod inter;
mod intra;
mod rgb;
mod simd;
mod tables;

pub use container::Bk2;
pub use frame::{Frame, MbTrace, MbType, Plane};
pub use simd::set_simd;

use bits::Bits;
use context::SliceCtx;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// No codeword matched within 12 bits; the bit position (from packet byte 8) after the attempt.
    BadCodeword(usize),
    /// A quantiser below 0 reached a dequantisation (its `q >> 2` is a shift count).
    NegativeQuantiser,
    /// A value left the range the decoder keeps exact (desynchronised or unprobed extreme streams).
    Overflow(&'static str),
    /// The packet is shorter than its flags word.
    ShortPacket,
    /// An inter frame with no previous frame.
    NoReference,
    /// A syntax element whose meaning has not been established yet.
    Unsupported(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::BadCodeword(p) => write!(f, "bad codeword at bit {p}"),
            Error::NegativeQuantiser => write!(f, "negative quantiser"),
            Error::Overflow(what) => write!(f, "{what} out of range"),
            Error::ShortPacket => write!(f, "packet shorter than its header"),
            Error::NoReference => write!(f, "inter frame without a reference frame"),
            Error::Unsupported(what) => write!(f, "unsupported: {what}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Run the post-filters (diagnostics turn them off; the residual filter inside INTER macroblocks
    /// goes with them).
    pub filters: bool,
    /// Slices per frame (2, or 4 — P30); the packet starts with that many words (flags, then the
    /// offsets of slices 2..n), the bitstream after them.
    pub slices: usize,
    /// Format revision, the magic's last letter (`b'g'` = KB2g, `b'j'` = KB2j).
    pub rev: u8,
    /// Record what this macroblock (x, y) decoded in `Frame::trace`.
    pub trace: Option<(usize, usize)>,
    /// Slices decoded at the same time, on threads of their own (1: one after another). Each starts
    /// where its packet offset word says, then checked against the 32-bit boundary after the slice
    /// before it (P22; the slices after a mismatch are decoded again from there): the frame is the one
    /// a sequential decode gives. After an error a frame may hold more decoded slices than sequentially.
    pub threads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self { filters: true, slices: 2, rev: b'g', trace: None, threads: 1 }
    }
}

impl Options {
    /// The options a file's header implies: slice layout and revision.
    pub fn for_file(bk: &Bk2) -> Self {
        Self { slices: bk.slices(), rev: bk.magic[3], ..Self::default() }
    }
}

/// Macroblock rows of each slice. P22, two slices: slice 2 starts at half the padded rows; under 4 rows
/// there is one slice. P30, four slices: as even as possible, the first slices taking the extra rows.
pub fn slice_rows(mbh: usize, slices: usize) -> Vec<(usize, usize)> {
    let starts: Vec<usize> = match slices {
        4 => (0..4)
            .scan(0, |y, k| {
                let y0 = *y;
                *y += (mbh + 3 - k) / 4;
                Some(y0)
            })
            .collect(),
        _ => vec![0, if mbh >= 4 { mbh / 2 } else { mbh }],
    };
    let ends = starts.iter().skip(1).copied().chain([mbh]);
    starts.iter().copied().zip(ends).collect()
}

/// P32: the block lines each filter line map covers, (horizontal, vertical): the motion bounds of P22 in
/// 8-pixel lines — the height rounded up to 16, the width padded to whole macroblocks. Lines past them
/// are not filtered.
pub fn line_map_sizes(width: usize, height: usize) -> (usize, usize) {
    (height.next_multiple_of(16) / 8, width.next_multiple_of(32) / 8)
}

/// P32: one direction's line map over its n block lines (`line_map_sizes`), returned as "filter A runs
/// on this line" (line 0, the frame edge, has no entry in the stream). A mode bit, then:
/// - mode 0: one flag per inner line 1..n-1, set = the line is left alone;
/// - mode 1: items until every inner line is coded. Item bit 0: an explicit group of the next 5 lines,
///   one flag each. Item bit 1: a run of equal lines — a value bit, then a w-bit length c, the run
///   covering b + c lines, where w is the bit length of the lines remaining (this one included) clamped
///   to 4..5 and b = w + 2. The line after a run has the opposite value without a bit and opens the next
///   item — unless the run's c was all ones (a longest run, which may go on with either value): a group
///   then codes only its other 4 lines, a run has no value bit and covers b - 1 + c lines counting that
///   line. A run with no more lines left than its shortest length (b, or b - 1 after a run) takes them
///   all and has no length.
fn read_line_map(r: &mut Bits, n: usize) -> Result<Vec<bool>, Error> {
    let m = n - 1;
    let mut skip = vec![false; n];
    if r.bit() == 0 {
        for s in skip.iter_mut().skip(1) {
            *s = r.bit() == 1;
        }
        return Ok(skip.iter().map(|&s| !s).collect());
    }
    let (mut pos, mut implied) = (1usize, None::<bool>);
    while pos <= m {
        let rem = m - pos + 1;
        let w = (usize::BITS - rem.leading_zeros()).clamp(4, 5);
        let b = w as usize + 2;
        if r.bit() == 0 {
            let end = (pos + 5).min(m + 1);
            for (line, s) in skip.iter_mut().enumerate().take(end).skip(pos) {
                *s = match implied {
                    Some(v) if line == pos => v,
                    _ => r.bit() == 1,
                };
            }
            pos = end;
            implied = None;
            continue;
        }
        let (v, base) = match implied {
            Some(v) => (v, b - 1),
            None => (r.bit() == 1, b),
        };
        let (len, longest) = if rem <= base {
            (rem, false)
        } else {
            let c = r.bits(w);
            (base + c as usize, c == (1 << w) - 1)
        };
        if len > rem {
            return Err(Error::Overflow("filter line map run"));
        }
        skip[pos..pos + len].fill(v);
        pos += len;
        implied = (pos <= m && !longest).then_some(!v);
    }
    Ok(skip.iter().map(|&s| !s).collect())
}

/// Decodes one video packet: a keyframe when `refs` is `None`, else an inter frame predicted from
/// `refs`. On an error the frame holds what was decoded up to it (unfiltered).
pub fn decode_frame(
    packet: &[u8],
    width: usize,
    height: usize,
    refs: Option<&[Plane; 3]>,
    opts: &Options,
) -> (Frame, Result<(), Error>) {
    let mut frame = Frame::new(width, height);
    let res = decode_into(&mut frame, packet, refs, opts, None);
    (frame, res)
}

/// Diagnostics (P32): a frame's filter line maps given rather than read. The stream's maps are taken to
/// be `bits` long and skipped; `v` and `h` hold, per luma block column and row, whether filter A runs on
/// that line.
#[derive(Clone, Debug, Default)]
pub struct LineMaps {
    pub bits: usize,
    pub v: Vec<bool>,
    pub h: Vec<bool>,
}

/// Diagnostics (P32): one line map over `n` lines read from bit `start` of `data`: the lines filter A
/// runs on and the bit position after the map.
pub fn parse_line_map(data: &[u8], start: usize, n: usize) -> (Result<Vec<bool>, Error>, usize) {
    let mut r = Bits::new(data);
    r.pos = start;
    (read_line_map(&mut r, n), r.pos)
}

/// `decode_frame` with the filter line maps of a KB2j flag-bit-16 frame given (`LineMaps`).
pub fn decode_frame_with_maps(
    packet: &[u8],
    width: usize,
    height: usize,
    refs: Option<&[Plane; 3]>,
    opts: &Options,
    maps: &LineMaps,
) -> (Frame, Result<(), Error>) {
    let mut frame = Frame::new(width, height);
    let res = decode_into(&mut frame, packet, refs, opts, Some(maps));
    (frame, res)
}

fn decode_into(
    f: &mut Frame,
    packet: &[u8],
    refs: Option<&[Plane; 3]>,
    opts: &Options,
    maps: Option<&LineMaps>,
) -> Result<(), Error> {
    let flags = u32::from_le_bytes(packet.get(..4).ok_or(Error::ShortPacket)?.try_into().unwrap());
    // P22: each slice after the first starts at the next 32-bit boundary; the DLL does not consult the
    // offset words (threads use them as a guess that is checked)
    let data = packet.get(4 * opts.slices..).unwrap_or(&[]);
    let mut r = Bits::new(data);
    r.rev = opts.rev;
    // P32: KB2j frames with flag bit 16, keyframes and inter frames, start with a map of the block lines
    // filter A runs on, per direction that is on: horizontal edges (one entry per block row), then
    // vertical (per block column)
    if let Some(m) = maps {
        r.pos = m.bits;
        f.lines = filters::EdgeLines { v: m.v.clone(), h: m.h.clone() };
    } else if opts.rev >= b'j' && flags & 0x10000 != 0 {
        let dirs = filters::EdgeDirs::from_flags(flags);
        let (nh, nv) = line_map_sizes(f.width, f.height);
        if dirs.horizontal {
            f.lines.h = read_line_map(&mut r, nh)?;
        }
        if dirs.vertical {
            f.lines.v = read_line_map(&mut r, nv)?;
        }
    }
    let slices = slice_rows(f.mbh, opts.slices);
    let setup = SliceSetup {
        refs,
        flags,
        opts: *opts,
        mbw: f.mbw,
        bound: (f.planes[0].width as i32, ((f.height + 15) & !15) as i32), // P22: padded width; height to 16
    };
    let starts = slice_starts(packet, opts.slices, &slices, r.pos).filter(|_| opts.threads > 1);
    let mut views = f.rows(&slices);
    let res = match starts {
        Some(starts) => decode_parallel(data, &mut views, &slices, &starts, &setup),
        None => decode_sequential(&mut r, &mut views, &slices, 0, &setup),
    };
    let trace = views.iter_mut().find_map(|v| v.trace.take());
    f.trace = trace;
    res?;
    if opts.filters {
        filters::apply(f, flags, opts.threads);
    }
    Ok(())
}

/// What every slice of a frame decodes with.
struct SliceSetup<'a> {
    refs: Option<&'a [Plane; 3]>,
    flags: u32,
    opts: Options,
    mbw: usize,
    /// P22: the MV edge bounds.
    bound: (i32, i32),
}

/// Slices `from..` one after another, the first from `r`'s position, each later one from the next
/// 32-bit boundary (P22); empty slices take no bits.
fn decode_sequential(
    r: &mut Bits,
    views: &mut [frame::Rows],
    slices: &[(usize, usize)],
    from: usize,
    setup: &SliceSetup,
) -> Result<(), Error> {
    let mut scratch = context::Scratch::new();
    for (k, (view, &(y0, y1))) in views.iter_mut().zip(slices).enumerate().skip(from) {
        if y0 >= y1 {
            continue;
        }
        if k > from {
            r.align32();
        }
        decode_slice(r, view, &mut scratch, (y0, y1), setup)?;
    }
    Ok(())
}

/// The bit positions the packet's offset words give for each slice (bytes from the start of the
/// packet; slice 1 starts at `first`), when they are in order and inside the packet.
fn slice_starts(packet: &[u8], n: usize, slices: &[(usize, usize)], first: usize) -> Option<Vec<usize>> {
    let mut starts = vec![first];
    for k in 1..slices.len() {
        let word = u32::from_le_bytes(packet.get(4 * k..4 * k + 4)?.try_into().unwrap()) as usize;
        let bit = word.checked_sub(4 * n)? * 8;
        if bit < starts[k - 1] || bit > 8 * packet.len() {
            return None;
        }
        starts.push(bit);
    }
    (slices.iter().filter(|(y0, y1)| y0 < y1).count() > 1).then_some(starts)
}

/// The slices at once (at most `threads` at a time), each from its offset word; then, in order, each
/// start is checked against the 32-bit boundary after the slice before it. At the first that differs
/// the remaining slices are decoded again one after another from the right place.
fn decode_parallel(
    data: &[u8],
    views: &mut [frame::Rows],
    slices: &[(usize, usize)],
    starts: &[usize],
    setup: &SliceSetup,
) -> Result<(), Error> {
    let rev = setup.opts.rev;
    let jobs: Vec<usize> = (0..slices.len()).filter(|&k| slices[k].0 < slices[k].1).collect();
    let mut done: Vec<Option<(Result<(), Error>, usize)>> = vec![None; slices.len()];
    let mut rest = &mut views[..];
    let mut taken = 0;
    let mut by_slice: Vec<&mut frame::Rows> = Vec::with_capacity(slices.len());
    while let Some((v, tail)) = std::mem::take(&mut rest).split_first_mut() {
        by_slice.push(v);
        rest = tail;
        taken += 1;
    }
    debug_assert_eq!(taken, slices.len());
    for wave in jobs.chunks(setup.opts.threads.max(1)) {
        let mut mine: Vec<(usize, &mut frame::Rows)> = Vec::new();
        for (k, v) in by_slice.iter_mut().enumerate() {
            if wave.contains(&k) {
                mine.push((k, &mut **v));
            }
        }
        let results: Vec<(usize, Result<(), Error>, usize)> = std::thread::scope(|sc| {
            let handles: Vec<_> = mine
                .into_iter()
                .map(|(k, v)| {
                    let start = starts[k];
                    sc.spawn(move || {
                        let mut r = Bits::new(data);
                        (r.rev, r.pos) = (rev, start);
                        let res = decode_slice(&mut r, v, &mut context::Scratch::new(), slices[k], setup);
                        (k, res, r.pos)
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("slice thread panicked")).collect()
        });
        for (k, res, end) in results {
            done[k] = Some((res, end));
        }
    }
    let mut prev_end: Option<usize> = None;
    for &k in &jobs {
        let (res, end) = done[k].take().expect("every slice decoded");
        if let Some(e) = prev_end {
            let expected = (e + 31) & !31;
            if expected != starts[k] {
                let mut r = Bits::new(data);
                (r.rev, r.pos) = (rev, expected);
                for v in views[k..].iter_mut() {
                    v.trace = None;
                }
                return decode_sequential(&mut r, views, slices, k, setup);
            }
        }
        res?;
        prev_end = Some(end);
    }
    Ok(())
}

/// One slice's macroblocks (P22: every map starts empty again at its first row).
fn decode_slice(
    r: &mut Bits,
    f: &mut frame::Rows,
    scratch: &mut context::Scratch,
    (y0, y1): (usize, usize),
    setup: &SliceSetup,
) -> Result<(), Error> {
    let (opts, flags) = (&setup.opts, setup.flags);
    let mut ctx = SliceCtx::new(setup.mbw, y0, y1);
    (ctx.bound_w, ctx.bound_h) = setup.bound;
    // P30: KB2j frame flags bit 17 (MVs clamped at the frame edge) and bit 18 (no in-loop residual
    // filter); KB2g ignores both
    let (clamp, no_resid) =
        if opts.rev >= b'j' { (flags & 0x20000 != 0, flags & 0x40000 != 0) } else { (false, false) };
    ctx.filter_a =
        if opts.filters && !no_resid { filters::EdgeDirs::from_flags(flags) } else { Default::default() };
    ctx.mv_clamp = clamp;
    ctx.trace = opts.trace;
    for mby in y0..y1 {
        let mut list = inter::TYPE_LIST0;
        for mbx in 0..setup.mbw {
            let typ = match setup.refs {
                None => {
                    intra::decode_mb(r, &mut ctx, scratch, f, mbx, mby)?;
                    MbType::Key
                }
                Some(refs) => {
                    let t = inter::decode_mb(r, &mut ctx, scratch, f, refs, mbx, mby, &mut list)?;
                    ctx.hold(mbx, mby, t);
                    for k in 0..4 {
                        let (qx, qy) = (2 * mbx + k % 2, 2 * mby + k / 2);
                        f.set_qmv(qx, qy, ctx.mv[ctx.qi(qx, qy)]);
                    }
                    t
                }
            };
            f.set_mb(mbx, mby, typ, r.pos);
        }
    }
    Ok(())
}

/// Decodes a stream frame by frame, each inter frame from the previous output.
pub struct Decoder {
    width: usize,
    height: usize,
    pub options: Options,
    prev: Option<Frame>,
    /// The buffer the next frame is decoded into (the frame before `prev`, or one that failed).
    spare: Option<Frame>,
}

impl Decoder {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, options: Options::default(), prev: None, spare: None }
    }

    /// A decoder for a file's video: its size and slice layout.
    pub fn for_file(bk: &Bk2) -> Self {
        let mut d = Self::new(bk.width, bk.height);
        d.options = Options::for_file(bk);
        d
    }

    /// The next frame. On an error the previous frame stays the reference.
    pub fn decode(&mut self, packet: &[u8], keyframe: bool) -> Result<&Frame, Error> {
        let refs = match (&self.prev, keyframe) {
            (_, true) => None,
            (Some(prev), false) => Some(&prev.planes),
            (None, false) => return Err(Error::NoReference),
        };
        let mut frame = match self.spare.take() {
            Some(mut f) => {
                f.reset();
                f
            }
            None => Frame::new(self.width, self.height),
        };
        if let Err(e) = decode_into(&mut frame, packet, refs, &self.options, None) {
            self.spare = Some(frame);
            return Err(e);
        }
        self.spare = self.prev.replace(frame);
        Ok(self.prev.as_ref().unwrap())
    }
}
