//! TLV-stream parser for the `.spt` parameter section.
//!
//! Walks the file from offset 20 (right after the magic header) as
//! a sequence of `(u32 tag, payload)` pairs, dispatching on tag via
//! [`crate::tag::dispatch_tag`] and decoding the payload per
//! [`crate::tag::SptTagKind`]. Stops cleanly when the next tag is
//! out of range — that's the binary geometry tail (Phase 1.3
//! follow-up).
//!
//! ## Tail detection
//!
//! The walker stops without error when:
//!
//! - `is_eof()` — reached end of file. Sets `SptScene::reached_eof = true`.
//! - The peeked u32 isn't in `[TAG_MIN, TAG_MAX]` — geometry tail.
//!   Records `tail_offset = current position`.
//!
//! ## Unknown tags
//!
//! When the walker encounters an in-range tag that isn't in the
//! dictionary, it's recorded into `SptScene::unknown_tags` and the
//! walker stops at the same offset as if it hit the geometry tail.
//! This is how the parser stays defensive against mod content with
//! tags we haven't observed yet — the placeholder fallback path
//! kicks in cleanly without aborting.

use crate::scene::{SptScene, SptValue, TagEntry};
use crate::stream::SptStream;
use crate::tag::{dispatch_tag, SptTagKind};
use crate::version::MAGIC_HEAD;
use std::io;

/// Lower bound of plausible parameter-section tag values. Below this,
/// the walker assumes it's reading binary noise (geometry tail).
pub const TAG_MIN: u32 = 100;
/// Upper bound of plausible parameter-section tag values. Above this,
/// same assumption.
pub const TAG_MAX: u32 = 13_999;

/// Parse a `.spt` byte stream into an [`SptScene`].
///
/// Returns `Err(io::Error)` (`InvalidData`) on five fatal conditions:
/// magic-header mismatch, stream underflow during a partially-read
/// payload, `read_string_lp`'s > 64 KiB length-prefix cap, `read_payload`'s
/// `count × stride` > 64 KiB array-size cap, and an unrecognized
/// context-sensitive-kind arm. All five discard the whole `SptScene`,
/// including every `TagEntry` already decoded — unlike an in-range but
/// unknown tag, which surfaces non-fatally via `SptScene::unknown_tags`
/// and returns everything decoded so far, with the walker stopping
/// cleanly at the offset where it bailed.
pub fn parse_spt(bytes: &[u8]) -> io::Result<SptScene> {
    if !bytes.starts_with(MAGIC_HEAD) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "spt magic header missing — file does not begin with `__IdvSpt_02_` signature",
        ));
    }

    let mut stream = SptStream::new(bytes);
    // Skip the 20-byte magic. We've already validated it via
    // `starts_with(MAGIC_HEAD)`, so a positional skip is sufficient.
    stream.read_bytes(MAGIC_HEAD.len())?;

    let mut scene = SptScene::default();

    while !stream.is_eof() {
        let tag_offset = stream.position();
        let Some(tag) = stream.peek_u32_le() else {
            break;
        };

        // Tail detection: out-of-range tag stops the walker.
        if !(TAG_MIN..=TAG_MAX).contains(&tag) {
            scene.tail_offset = tag_offset;
            return Ok(scene);
        }

        match dispatch_tag(tag) {
            SptTagKind::Unknown => {
                // Tag is in-range but not in the dictionary — record
                // diagnostically and stop cleanly. This matches the
                // tail-detection semantics: the placeholder fallback
                // takes over from here.
                scene.unknown_tags.push((tag, tag_offset));
                scene.tail_offset = tag_offset;
                return Ok(scene);
            }
            SptTagKind::MaybeStringElseBare => {
                // #999 — tag 13005 is bimodal across the Oblivion
                // corpus: 109 vanilla files emit it bare, 4 outliers
                // emit it with an optional length-prefixed string
                // (BezierSpline curve text). Peek the next `u32` to
                // disambiguate: if it's a known dictionary tag, the
                // current entry has no payload (`Bare`). Otherwise
                // assume it's a string length and consume length +
                // bytes.
                //
                // Robust against observed vanilla content where the
                // optional curve is 104 bytes long — `dispatch_tag(104)`
                // returns `Unknown` so the heuristic correctly routes
                // to the String branch. A pathological mod file
                // where the string length happens to coincide with a
                // dictionary tag value (e.g. length = 1002) would
                // misparse as Bare; not observed in the 133-file
                // corpus and not worth more complex lookahead today.
                let _ = stream.read_u32_le()?;
                let next_is_known_tag = match stream.peek_u32_le() {
                    Some(next) => {
                        (TAG_MIN..=TAG_MAX).contains(&next)
                            && !matches!(dispatch_tag(next), SptTagKind::Unknown)
                    }
                    None => false,
                };
                let value = if next_is_known_tag {
                    SptValue::Bare
                } else {
                    // #1822 — "not a known tag" alone isn't enough to
                    // conclude String: a bare 13005 sitting immediately
                    // before the binary geometry tail looks identical
                    // from here, since the tail's leading u32 is just as
                    // likely to fall outside the dictionary as a genuine
                    // curve-string length. Every observed curve string is
                    // printable-ASCII BezierSpline text (see the fixture
                    // in `tag_13005_followed_by_string_length_resolves_
                    // as_string` below); geometry-tail bytes essentially
                    // never satisfy that uniformly by chance. Peek the
                    // candidate string's bytes and only take the String
                    // branch if they pass — otherwise fall back to
                    // `Bare` without consuming anything, so tail
                    // detection sees the real tail value next iteration
                    // instead of a slab of it having been swallowed as a
                    // bogus string.
                    match stream
                        .peek_string_lp_bytes()
                        .filter(|bytes| is_plausible_spt_curve_string(bytes))
                    {
                        Some(_) => SptValue::String(stream.read_string_lp()?),
                        None => SptValue::Bare,
                    }
                };
                scene.entries.push(TagEntry {
                    tag,
                    value,
                    offset: tag_offset,
                });
            }
            kind => {
                // Consume the tag, then the kind-specific payload.
                let _ = stream.read_u32_le()?;
                let value = read_payload(&mut stream, kind, tag_offset)?;
                scene.entries.push(TagEntry {
                    tag,
                    value,
                    offset: tag_offset,
                });
            }
        }
    }

    scene.tail_offset = stream.position();
    scene.reached_eof = true;
    Ok(scene)
}

/// Whether `bytes` plausibly hold BezierSpline curve text — the only
/// content ever observed behind tag 13005's optional String branch (see
/// `tag_13005_followed_by_string_length_resolves_as_string`'s fixture:
/// `"BezierSpline 0\t1\t0\n{\n\n\t2\n\t0 1 0.714831 ...\n\n}\n"`). Every
/// byte must be printable ASCII (0x20..=0x7E) or one of the whitespace
/// control codes that format actually uses (tab, LF, CR). Binary geometry-
/// tail bytes essentially never satisfy this uniformly across a candidate
/// length, which is what makes it a robust discriminator for #1822 — unlike
/// a bare length-range check, it doesn't get fooled by a tail u32 that
/// happens to decode to a small, in-bounds "length".
fn is_plausible_spt_curve_string(bytes: &[u8]) -> bool {
    // #3531 — the emptiness check is load-bearing, not defensive tidying.
    // `Iterator::all` is vacuously true on an empty slice and
    // `peek_string_lp_bytes` returns `Some(&[])` for a declared length of
    // `0`, so without this a bare 13005 sitting before a geometry tail whose
    // leading `u32` is `0` still took the String arm, consumed 4 bytes and
    // shifted `tail_offset` past the true tail start — the exact #1822
    // failure mode, for the one candidate length the printable-ASCII
    // discriminator cannot discriminate. A leading `0` (a zero count or
    // index) is an ordinary way for a binary tail to begin.
    //
    // Which arm is right for a zero-length candidate is a format question,
    // and the corpus does not answer it directly: instrumenting this arm over
    // all 133 vanilla `.spt` files reaches it exactly 4 times, every one the
    // known 104-byte `BezierSpline` blob, never a zero length. The risk is
    // asymmetric, which is what settles it — a zero-length curve string
    // carries no curve, so reading it as `Bare` loses nothing, while reading
    // a tail's leading `0` as a string desynchronises every byte after it.
    // See `docs/format-notes.md`, "Tag 13005 bimodal payload".
    !bytes.is_empty()
        && bytes
            .iter()
            .all(|&b| matches!(b, 0x20..=0x7E | b'\t' | b'\n' | b'\r'))
}

/// Read the payload for a tag of a known [`SptTagKind`].
fn read_payload(
    stream: &mut SptStream,
    kind: SptTagKind,
    tag_offset: usize,
) -> io::Result<SptValue> {
    Ok(match kind {
        SptTagKind::Bare => SptValue::Bare,
        SptTagKind::U8 => SptValue::U8(stream.read_u8()?),
        SptTagKind::U32 => SptValue::U32(stream.read_u32_le()?),
        SptTagKind::Vec3 => SptValue::Vec3(stream.read_vec3_le()?),
        SptTagKind::FixedBytes(n) => {
            let bytes = stream.read_bytes(n as usize)?.to_vec();
            SptValue::Fixed(bytes)
        }
        SptTagKind::String => SptValue::String(stream.read_string_lp()?),
        SptTagKind::ArrayBytes { stride } => {
            let count = stream.read_u32_le()?;
            // Sanity-cap array byte length at 64 KiB to keep a
            // corrupt count value from allocating gigabytes — same
            // bound as `read_string_lp`.
            let total_bytes = (count as u64).saturating_mul(stride as u64);
            if total_bytes > 65_536 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "spt array at offset {tag_offset}: count {count} × stride {stride} = {total_bytes} bytes exceeds 64 KiB sanity cap",
                    ),
                ));
            }
            let bytes = stream.read_bytes(total_bytes as usize)?.to_vec();
            SptValue::ArrayBytes {
                stride,
                count,
                bytes,
            }
        }
        SptTagKind::Unknown | SptTagKind::MaybeStringElseBare => {
            // Walker dispatches both variants before reaching here —
            // `Unknown` records the diagnostic and bails;
            // `MaybeStringElseBare` peeks the next u32 and produces
            // the payload inline. Defensive bail in case a future
            // dispatch table grows a new context-sensitive kind that
            // leaks here without walker handling.
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "context-sensitive tag kind at offset {} — must be handled in walker",
                    tag_offset
                ),
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic `.spt` with a deliberately crafted parameter
    /// section. Every tag kind we know about gets exercised:
    /// bare / u8 / u32 / vec3 / fixed-52 / string. Followed by an
    /// out-of-range u32 so the walker terminates at the "geometry
    /// tail" boundary.
    fn build_synthetic_spt() -> Vec<u8> {
        let mut buf = Vec::new();
        // 20-byte magic.
        buf.extend_from_slice(MAGIC_HEAD);
        // Tag 1002 (bare).
        buf.extend_from_slice(&1002u32.to_le_bytes());
        // Tag 2000 (string) — bark texture path.
        buf.extend_from_slice(&2000u32.to_le_bytes());
        let bark = b"trees/oak/bark.dds";
        buf.extend_from_slice(&(bark.len() as u32).to_le_bytes());
        buf.extend_from_slice(bark);
        // Tag 2001 (u32 = f32 1100.0).
        buf.extend_from_slice(&2001u32.to_le_bytes());
        buf.extend_from_slice(&1100.0f32.to_le_bytes());
        // Tag 2002 (u8 = 0x42).
        buf.extend_from_slice(&2002u32.to_le_bytes());
        buf.push(0x42);
        // Tag 4001 (vec3).
        buf.extend_from_slice(&4001u32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&2.0f32.to_le_bytes());
        buf.extend_from_slice(&3.0f32.to_le_bytes());
        // Tag 6000 (string — curve text blob).
        let curve = b"BezierSpline 0\t1\t0\n{\n\n\t2\n\t0 0 1 0 1\n\t1 1 0 1 1\n\n}\n";
        buf.extend_from_slice(&6000u32.to_le_bytes());
        buf.extend_from_slice(&(curve.len() as u32).to_le_bytes());
        buf.extend_from_slice(curve);
        // Tag 8003 (fixed 52 bytes).
        buf.extend_from_slice(&8003u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 52]);
        // "Geometry tail" marker — out-of-range u32 = 0x4E25 (= 19 989).
        buf.extend_from_slice(&0x00004E25u32.to_le_bytes());
        buf.extend_from_slice(&[0xCAu8, 0xFE, 0xBA, 0xBE]); // body of the tail
        buf
    }

    #[test]
    fn parse_spt_round_trips_synthetic_fixture() {
        let bytes = build_synthetic_spt();
        let scene = parse_spt(&bytes).expect("synthetic .spt must parse");
        assert!(!scene.reached_eof, "tail boundary, not EOF");
        assert!(scene.unknown_tags.is_empty());
        assert_eq!(scene.entries.len(), 7, "every tag landed");

        // Spot-check a handful.
        assert_eq!(scene.entries[0].tag, 1002);
        assert_eq!(scene.entries[0].value, SptValue::Bare);

        assert_eq!(scene.bark_textures(), vec!["trees/oak/bark.dds"]);

        let v_2001 = &scene.entries[2];
        assert_eq!(v_2001.tag, 2001);
        assert_eq!(v_2001.value.as_f32(), Some(1100.0));

        assert_eq!(scene.entries[3].value, SptValue::U8(0x42));
        assert_eq!(scene.entries[4].value, SptValue::Vec3([1.0, 2.0, 3.0]));

        let curves = scene.curves();
        assert_eq!(curves.len(), 1);
        assert!(curves[0].1.starts_with("BezierSpline 0"));

        // Tail offset should point at the 0x4E25 sentinel — i.e.
        // right after the 52-byte fixed payload of tag 8003.
        let expected_tail =
            bytes.len() - 4 /* tail body */ - 4 /* tail marker u32 */;
        assert_eq!(scene.tail_offset, expected_tail);
    }

    #[test]
    fn parse_spt_rejects_missing_magic() {
        let bytes = vec![0xDEu8, 0xAD, 0xBE, 0xEF];
        let err = parse_spt(&bytes).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn parse_spt_records_unknown_tag_diagnostically() {
        // Magic + a single in-range-but-unknown tag (4096 — the
        // hex-aligned confounder from the recon analysis).
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_HEAD);
        bytes.extend_from_slice(&4096u32.to_le_bytes());
        // Some payload bytes that wouldn't be read.
        bytes.extend_from_slice(&[0u8; 32]);

        let scene = parse_spt(&bytes).expect("unknown tag is non-fatal");
        assert!(scene.entries.is_empty(), "no tag was decoded");
        assert_eq!(scene.unknown_tags.len(), 1);
        assert_eq!(scene.unknown_tags[0].0, 4096);
        assert_eq!(scene.tail_offset, MAGIC_HEAD.len());
    }

    #[test]
    fn parse_spt_handles_eof_immediately_after_magic() {
        let scene = parse_spt(MAGIC_HEAD).expect("bare magic parses");
        assert!(scene.reached_eof);
        assert!(scene.entries.is_empty());
        assert_eq!(scene.tail_offset, MAGIC_HEAD.len());
    }

    /// #999 — tag 13005 followed by another known tag must resolve as
    /// `Bare`. Models the 109 vanilla Oblivion files that work today.
    #[test]
    fn tag_13005_followed_by_known_tag_resolves_as_bare() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_HEAD);
        bytes.extend_from_slice(&13005u32.to_le_bytes());
        // Next: tag 13006 (U32). Walker peek must classify 13005 as Bare.
        bytes.extend_from_slice(&13006u32.to_le_bytes());
        bytes.extend_from_slice(&0x12345678u32.to_le_bytes());
        // Out-of-range sentinel to stop the walker.
        bytes.extend_from_slice(&0x4E25u32.to_le_bytes());

        let scene = parse_spt(&bytes).expect("must parse");
        assert!(scene.unknown_tags.is_empty(), "no unknown tags");
        assert_eq!(scene.entries.len(), 2);
        assert_eq!(scene.entries[0].tag, 13005);
        assert_eq!(scene.entries[0].value, SptValue::Bare);
        assert_eq!(scene.entries[1].tag, 13006);
        assert_eq!(scene.entries[1].value, SptValue::U32(0x12345678));
    }

    /// #999 — tag 13005 followed by `u32` length value (not a tag)
    /// must resolve as `String`. Models the 4 Oblivion outliers
    /// (`treems14canvasfreesu`, `treecottonwoodsu`, `shrubms14boxwood`,
    /// `treems14willowoakyoungsu`). Pre-fix the walker bailed on the
    /// length value (104) as an unknown tag.
    #[test]
    fn tag_13005_followed_by_string_length_resolves_as_string() {
        // Mirror the exact vanilla-Oblivion-outlier curve string.
        let curve_body: &[u8] = b"BezierSpline 0\t1\t0\n{\n\n\t2\n\t0 1 0.714831 -0.699297 0.079604\n\t1 0.000782381 0.699609 -0.714526 0.107006\n\n}\n";
        assert_eq!(
            curve_body.len(),
            104,
            "fixture mirrors the observed 104-byte curve"
        );

        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_HEAD);
        bytes.extend_from_slice(&13005u32.to_le_bytes());
        bytes.extend_from_slice(&(curve_body.len() as u32).to_le_bytes());
        bytes.extend_from_slice(curve_body);
        // Resume with tag 13006 to prove the walker re-syncs after the
        // optional payload.
        bytes.extend_from_slice(&13006u32.to_le_bytes());
        bytes.extend_from_slice(&0xCAFEBABEu32.to_le_bytes());
        bytes.extend_from_slice(&0x4E25u32.to_le_bytes());

        let scene = parse_spt(&bytes).expect("must parse");
        assert!(
            scene.unknown_tags.is_empty(),
            "no bail on tag-104 confounder"
        );
        assert_eq!(scene.entries.len(), 2);
        assert_eq!(scene.entries[0].tag, 13005);
        match &scene.entries[0].value {
            SptValue::String(s) => assert!(s.starts_with("BezierSpline 0\t1\t0")),
            other => panic!("expected String, got {other:?}"),
        }
        assert_eq!(scene.entries[1].tag, 13006);
        assert_eq!(scene.entries[1].value, SptValue::U32(0xCAFEBABE));
    }

    /// EOF immediately after tag 13005 (no peek bytes available) is
    /// safe and, since #1822, deterministic: `peek_string_lp_bytes`
    /// requires 4 bytes to even read a length prefix, has none, and the
    /// walker falls back to `Bare` cleanly rather than attempting a read
    /// that would `UnexpectedEof`. Pre-#1822 this always hit the `Err`
    /// path instead (the String branch's `read_string_lp` had no length
    /// bytes to consume); either way, must not panic.
    #[test]
    fn tag_13005_at_eof_does_not_panic() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_HEAD);
        bytes.extend_from_slice(&13005u32.to_le_bytes());
        // No payload bytes; peek returns None.
        let scene = parse_spt(&bytes).expect("must resolve as Bare, not error, since #1822");
        assert_eq!(scene.entries.len(), 1);
        assert_eq!(scene.entries[0].tag, 13005);
        assert_eq!(scene.entries[0].value, SptValue::Bare);
    }

    /// Regression: #1822 (SPT-NEW-07). A bare 13005 sitting immediately
    /// before the binary geometry tail must resolve as `Bare` with
    /// `tail_offset` at 13005's successor — not have the tail's leading
    /// u32 misread as a string length, swallowing tail bytes as a bogus
    /// string (and, since that string's declared length here exactly
    /// matches the bytes available, the pre-fix walker would have
    /// consumed the *entire* rest of the stream as "string payload"
    /// without erroring — silently losing the tail rather than crashing).
    /// The leading tail value (8) is deliberately `< TAG_MIN` so, once
    /// correctly left unconsumed, the *existing* out-of-range tail-
    /// detection guard is what stops the walker on the next iteration —
    /// proving the fix reinstates that guard's ability to see the tail at
    /// all, rather than merely swapping one swallow bug for another.
    #[test]
    fn tag_13005_before_geometry_tail_resolves_as_bare_not_swallowed_string() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_HEAD);
        bytes.extend_from_slice(&13005u32.to_le_bytes());
        let tail_start = bytes.len();
        // Geometry tail: leading u32 = 8 (out-of-range for
        // TAG_MIN..=TAG_MAX, *and* — pre-fix — a perfectly plausible
        // string length matching the 8 trailing bytes exactly), followed
        // by non-ASCII binary bytes a real curve string would never
        // contain.
        bytes.extend_from_slice(&8u32.to_le_bytes());
        bytes.extend_from_slice(&[0xFF, 0x00, 0xDE, 0xAD, 0xBE, 0xEF, 0x80, 0x01]);

        let scene = parse_spt(&bytes).expect("must parse");
        assert_eq!(scene.entries.len(), 1);
        assert_eq!(scene.entries[0].tag, 13005);
        assert_eq!(
            scene.entries[0].value,
            SptValue::Bare,
            "the tail's leading u32 must not be misread as a string length"
        );
        assert_eq!(
            scene.tail_offset, tail_start,
            "tail_offset must sit at 13005's successor, not past a swallowed string"
        );
        assert!(scene.unknown_tags.is_empty());
    }

    /// #3531 — the residue of #1822: the one candidate length the
    /// printable-ASCII discriminator cannot discriminate. `Iterator::all` is
    /// vacuously true on an empty slice, so a tail whose leading `u32` is `0`
    /// took the String arm, consumed 4 bytes as an empty string and shifted
    /// `tail_offset` past the true tail start. The #1822 guard above
    /// deliberately uses a leading tail value of `8`, so this case was
    /// unpinned in both directions.
    #[test]
    fn tag_13005_before_zero_leading_tail_resolves_as_bare() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_HEAD);
        bytes.extend_from_slice(&13005u32.to_le_bytes());
        let tail_start = bytes.len();
        // A geometry tail beginning with a zero count/index — an ordinary
        // shape for a binary tail, and a "declared length 0" string to the
        // peek. Follow it with binary bytes so nothing else can absorb them.
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0xFF, 0x00, 0xDE, 0xAD]);

        let scene = parse_spt(&bytes).expect("must parse");
        assert_eq!(scene.entries.len(), 1);
        assert_eq!(scene.entries[0].tag, 13005);
        assert_eq!(
            scene.entries[0].value,
            SptValue::Bare,
            "a zero-length candidate must not take the String arm — an empty \
             curve string carries no curve, but a swallowed tail u32 \
             desynchronises every byte after it"
        );
        assert_eq!(
            scene.tail_offset, tail_start,
            "tail_offset must sit at 13005's successor, not 4 bytes past it"
        );
        assert!(scene.unknown_tags.is_empty());
    }

    /// The discriminator itself, including the vacuous-`all` hole directly.
    #[test]
    fn empty_candidate_is_not_a_plausible_curve_string() {
        assert!(
            !is_plausible_spt_curve_string(b""),
            "`Iterator::all` is vacuously true on an empty slice (#3531)"
        );
        assert!(is_plausible_spt_curve_string(b"BezierSpline 0\t1\t0\n"));
        assert!(!is_plausible_spt_curve_string(&[0xFF, 0x00]));
    }
}
