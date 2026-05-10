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
/// Returns `Err(io::Error)` only on truly fatal conditions —
/// magic-header mismatch or stream underflow during a partially-read
/// payload. In-range-but-unknown tags surface non-fatally via
/// `SptScene::unknown_tags`; the walker stops cleanly at the offset
/// where it bailed.
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

/// Read the payload for a tag of a known [`SptTagKind`].
fn read_payload(stream: &mut SptStream, kind: SptTagKind, tag_offset: usize) -> io::Result<SptValue> {
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
            SptValue::ArrayBytes { stride, count, bytes }
        }
        SptTagKind::Unknown => {
            // The walker dispatches on `Unknown` before reaching
            // here — this arm is unreachable in normal operation.
            // Defensive bail just in case the dictionary grows in
            // a way that violates the precondition.
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown tag at offset {} — should have bailed in walker", tag_offset),
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
}
