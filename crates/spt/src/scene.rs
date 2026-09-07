//! Output type for the `.spt` parameter-section walker.
//!
//! The parser emits an ordered list of `(tag, value)` pairs plus a
//! `tail_offset` marking where the walk stopped. Consumers query the
//! typed accessors (`bark_textures`, `leaf_textures`, `curves`, …) when
//! they want a specific section.
//!
//! `tail_offset` used to be documented as "where the binary geometry tail
//! begins". The 2026-09-07 dissection (#3808) measured that claim and it
//! does not hold — see [`SptScene::tail_offset`].

use crate::tag::SptTagKind;

/// One decoded `(tag, payload)` entry from a `.spt` parameter-section
/// stream.
#[derive(Debug, Clone, PartialEq)]
pub struct TagEntry {
    /// Tag value as it appeared on the wire.
    pub tag: u32,
    /// Decoded payload, dispatched per [`SptTagKind`].
    pub value: SptValue,
    /// Byte offset at which the tag was read (for diagnostics).
    pub offset: usize,
}

/// Decoded payload carried by a tag.
#[derive(Debug, Clone, PartialEq)]
pub enum SptValue {
    /// Tag has no payload — section / structure marker.
    Bare,
    /// 1-byte payload (`u8` / `bool`).
    U8(u8),
    /// 4-byte payload as raw bits. Consumer reinterprets as `u32` /
    /// `f32` per tag semantics (see `format-notes.md`).
    U32(u32),
    /// 12-byte payload — three little-endian f32 values.
    Vec3([f32; 3]),
    /// Fixed-size opaque byte payload of `bytes.len()` bytes.
    /// Layout-specific decode is downstream work.
    Fixed(Vec<u8>),
    /// Length-prefixed string payload.
    String(String),
    /// Length-prefixed binary array — `count` records of `stride`
    /// bytes each. Layout-specific decode is downstream work.
    ArrayBytes {
        stride: u8,
        count: u32,
        bytes: Vec<u8>,
    },
}

impl SptValue {
    /// Convenience: reinterpret a `U32` payload as f32. Returns
    /// `None` for any other variant.
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Self::U32(raw) => Some(f32::from_bits(*raw)),
            _ => None,
        }
    }

    /// Convenience: reinterpret a `U32` payload as raw u32.
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Self::U32(raw) => Some(*raw),
            _ => None,
        }
    }

    /// Convenience: extract a string payload's contents.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Convenience: kind tag for assertion / debugging.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Bare => "Bare",
            Self::U8(_) => "U8",
            Self::U32(_) => "U32",
            Self::Vec3(_) => "Vec3",
            Self::Fixed(_) => "Fixed",
            Self::String(_) => "String",
            Self::ArrayBytes { .. } => "ArrayBytes",
        }
    }
}

/// A parsed `.spt` parameter section, up to wherever the walker stopped.
///
/// What lies past [`tail_offset`](Self::tail_offset) is *more of this same
/// stream*, not a different kind of data — see that field's docs.
#[derive(Debug, Clone, Default)]
pub struct SptScene {
    /// Every `(tag, value)` entry in stream order. The parser
    /// preserves authoring order so two trees with semantically
    /// identical parameters but different tag-emit order still
    /// round-trip distinctly.
    pub entries: Vec<TagEntry>,
    /// Byte offset where the parameter walker stopped, or end-of-file for
    /// files it walked to completion.
    ///
    /// **Not a section boundary, and not the start of a geometry tail** —
    /// both of which this field's documentation used to claim. The
    /// 2026-09-07 corpus dissection (#3808,
    /// `crates/spt/docs/format-notes.md`) measured three things that
    /// together rule that reading out:
    ///
    /// - All 159 files in the FNV + FO3 + Oblivion corpus carry values
    ///   past this offset that the *existing* parameter dictionary already
    ///   classifies (10001, 10003, 10004, 13000, 13002-13007), at 4-byte
    ///   alignment. The stream continues; the walker merely stops, because
    ///   [`TAG_MAX`](crate::parser::TAG_MAX) caps it at 13 999 and the next
    ///   tag bands start at 14 000.
    /// - In 46 % of files the resync needs a 1-3 byte shift, meaning the
    ///   walker stopped *inside* a payload it mis-sized rather than at any
    ///   boundary.
    /// - No `.spt` in the corpus exceeds 8 793 bytes — below the cost of
    ///   274 vertices of position + normal + UV, for the entire file. There
    ///   is no geometry here to mark the start of.
    ///
    /// Treat it as "where parsing gave up", which is what it measures.
    pub tail_offset: usize,
    /// True when the walker stopped because it ran out of bytes
    /// (`is_eof`) rather than because it hit a non-tag value (the
    /// geometry tail).
    pub reached_eof: bool,
    /// Tags the walker encountered that aren't in the dictionary
    /// (`SptTagKind::Unknown`). Empty on a clean parse. Bumped at
    /// the bail-out site without aborting; the parser surfaces them
    /// as a non-fatal diagnostic so the placeholder fallback can
    /// kick in.
    pub unknown_tags: Vec<(u32, usize)>,
}

impl SptScene {
    /// Iterate entries by tag value. Useful for the typed accessors
    /// below.
    pub fn entries_with_tag(&self, tag: u32) -> impl Iterator<Item = &TagEntry> {
        self.entries.iter().filter(move |e| e.tag == tag)
    }

    /// All bark-texture paths (tag `2000`).
    pub fn bark_textures(&self) -> Vec<&str> {
        self.entries_with_tag(2000)
            .filter_map(|e| e.value.as_str())
            .collect()
    }

    /// All leaf-texture paths (tag `4003`).
    pub fn leaf_textures(&self) -> Vec<&str> {
        self.entries_with_tag(4003)
            .filter_map(|e| e.value.as_str())
            .collect()
    }

    /// All curve text blobs (tags `6000-6007`, `6017`). The text
    /// itself decodes via `parse_bezier_spline_text` (Phase 1.3
    /// follow-up).
    pub fn curves(&self) -> Vec<(u32, &str)> {
        self.entries
            .iter()
            .filter(|e| matches!(e.tag, 6000..=6007 | 6017))
            .filter_map(|e| e.value.as_str().map(|s| (e.tag, s)))
            .collect()
    }

    /// Helper: count entries by [`SptTagKind`] for diagnostics.
    pub fn count_by_kind(&self) -> [(SptTagKind, usize); 7] {
        use SptTagKind::*;
        let mut counts = [
            Bare,
            U8,
            U32,
            Vec3,
            FixedBytes(0),
            String,
            ArrayBytes { stride: 0 },
        ]
        .map(|k| (k, 0usize));
        for entry in &self.entries {
            let k = match &entry.value {
                SptValue::Bare => Bare,
                SptValue::U8(_) => U8,
                SptValue::U32(_) => U32,
                SptValue::Vec3(_) => Vec3,
                SptValue::Fixed(_) => FixedBytes(0),
                SptValue::String(_) => String,
                SptValue::ArrayBytes { .. } => ArrayBytes { stride: 0 },
            };
            for slot in &mut counts {
                if std::mem::discriminant(&slot.0) == std::mem::discriminant(&k) {
                    slot.1 += 1;
                }
            }
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_f32_decodes_u32_bits_correctly() {
        let v = SptValue::U32(0x44898000);
        assert_eq!(v.as_f32(), Some(1100.0));
        assert_eq!(v.as_u32(), Some(0x44898000));
        assert_eq!(v.as_str(), None);
    }

    #[test]
    fn as_str_returns_string_payload() {
        let v = SptValue::String("trees/oak.spt".to_string());
        assert_eq!(v.as_str(), Some("trees/oak.spt"));
        assert_eq!(v.as_f32(), None);
    }

    #[test]
    fn typed_accessors_filter_by_tag() {
        let scene = SptScene {
            entries: vec![
                TagEntry {
                    tag: 2000,
                    value: SptValue::String("bark.dds".into()),
                    offset: 0,
                },
                TagEntry {
                    tag: 4003,
                    value: SptValue::String("leaf.dds".into()),
                    offset: 0,
                },
                TagEntry {
                    tag: 6000,
                    value: SptValue::String("BezierSpline 0".into()),
                    offset: 0,
                },
                TagEntry {
                    tag: 6001,
                    value: SptValue::String("BezierSpline 1".into()),
                    offset: 0,
                },
                TagEntry {
                    tag: 2001,
                    value: SptValue::U32(0x44898000),
                    offset: 0,
                },
            ],
            tail_offset: 100,
            reached_eof: false,
            unknown_tags: Vec::new(),
        };
        assert_eq!(scene.bark_textures(), vec!["bark.dds"]);
        assert_eq!(scene.leaf_textures(), vec!["leaf.dds"]);
        assert_eq!(scene.curves().len(), 2);
        assert_eq!(scene.curves()[0], (6000, "BezierSpline 0"));
    }

    #[test]
    fn entries_with_tag_handles_repeats() {
        let scene = SptScene {
            entries: vec![
                TagEntry {
                    tag: 6000,
                    value: SptValue::String("a".into()),
                    offset: 0,
                },
                TagEntry {
                    tag: 6000,
                    value: SptValue::String("b".into()),
                    offset: 0,
                },
                TagEntry {
                    tag: 6001,
                    value: SptValue::String("c".into()),
                    offset: 0,
                },
            ],
            ..Default::default()
        };
        let curves: Vec<&str> = scene
            .entries_with_tag(6000)
            .filter_map(|e| e.value.as_str())
            .collect();
        assert_eq!(curves, vec!["a", "b"]);
    }
}

/// Guards the one thing that made #3808's blocking question unanswerable
/// for four months: a hex-to-decimal slip that then propagated verbatim
/// into two design docs and an issue body.
///
/// `0x4E25` and `0x4E21` were written up as 19 989 and 19 985 — both off
/// by exactly 16. Nobody could confirm or refute "the two candidate
/// markers" because the decimal values named were not the ones in the
/// files. Cheap to pin, and the pin is what stops the wrong pair coming
/// back the next time someone restates the pair from memory.
#[cfg(test)]
mod marker_value_pins {
    /// The arithmetic itself, so a restatement anywhere has something
    /// authoritative to check against.
    #[test]
    fn the_two_high_tag_markers_convert_to_20001_and_20005() {
        assert_eq!(0x4E21, 20_001, "0x4E21 is 20001, not 19985");
        assert_eq!(0x4E25, 20_005, "0x4E25 is 20005, not 19989");
    }

    /// The corrected values were measured present in 100 % of the corpus
    /// and the mis-converted ones in 0 %, so any doc still asserting the
    /// old pair as fact is asserting something the data contradicts.
    ///
    /// Scoped by *paragraph*, not by line: the docs deliberately keep the
    /// wrong numbers visible inside strikethroughs and "off by exactly 16"
    /// corrections, which is the record of the error rather than a
    /// restatement of it — and those corrections wrap across lines, so a
    /// line-scoped needle flags the correction itself. A paragraph that
    /// names the old pair must also carry its correction somewhere.
    #[test]
    fn no_design_doc_states_the_miscoverted_pair_as_a_live_value() {
        /// Markers that make a paragraph a correction rather than a claim.
        const CORRECTION_MARKERS: &[&str] = &[
            "~~",
            "wrong",
            "zero",
            "ANSWERED",
            "MOOT",
            "mis-conversion",
            "mis-conversions",
            "off by",
        ];
        for (label, src) in [
            (
                "docs/engine/exal-trees.md",
                include_str!("../../../docs/engine/exal-trees.md"),
            ),
            (
                "crates/spt/docs/format-notes.md",
                include_str!("../docs/format-notes.md"),
            ),
        ] {
            for paragraph in src.split("\n\n") {
                let mentions_old = paragraph.contains("19985")
                    || paragraph.contains("19989")
                    || paragraph.contains("19 985")
                    || paragraph.contains("19 989");
                if !mentions_old {
                    continue;
                }
                assert!(
                    CORRECTION_MARKERS.iter().any(|m| paragraph.contains(m)),
                    "{label} restates 19985/19989 as a live value; they are \
                     mis-conversions of 0x4E21/0x4E25 (= 20001/20005) and \
                     appear in 0 of 159 corpus files.\n\n{paragraph}"
                );
            }
        }
    }

    /// The same slip reached two source comments as well. Code carries no
    /// historical record worth preserving, so the rule there is strict
    /// absence rather than the docs' "must be accompanied by its
    /// correction".
    #[test]
    fn no_source_comment_carries_the_miscoverted_pair() {
        for (label, src) in [
            ("crates/spt/src/parser.rs", include_str!("parser.rs")),
            (
                "crates/spt/examples/spt_tagmap.rs",
                include_str!("../examples/spt_tagmap.rs"),
            ),
        ] {
            for needle in ["19985", "19989", "19 985", "19 989"] {
                assert!(
                    !src.contains(needle),
                    "{label} still carries {needle}; 0x4E21/0x4E25 are \
                     20001/20005"
                );
            }
        }
    }
}
