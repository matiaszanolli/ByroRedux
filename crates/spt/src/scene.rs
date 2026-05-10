//! Output type for the `.spt` parameter-section walker.
//!
//! The parser emits an ordered list of `(tag, value)` pairs plus a
//! `tail_offset` marking where the binary geometry tail begins (still
//! out-of-scope for the parameter walker; covered by a future
//! sub-phase). Consumers query the typed accessors (`bark_textures`,
//! `leaf_textures`, `curves`, …) when they want a specific section.

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

/// A parsed `.spt` parameter section. The geometry tail past
/// `tail_offset` is intentionally not decoded — that's a separate
/// future phase once the parameter section is fully understood.
#[derive(Debug, Clone, Default)]
pub struct SptScene {
    /// Every `(tag, value)` entry in stream order. The parser
    /// preserves authoring order so two trees with semantically
    /// identical parameters but different tag-emit order still
    /// round-trip distinctly.
    pub entries: Vec<TagEntry>,
    /// Byte offset where the parameter walker stopped (start of the
    /// binary geometry tail, or end-of-file for tail-less files).
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
        let mut counts = [Bare, U8, U32, Vec3, FixedBytes(0), String, ArrayBytes { stride: 0 }]
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
                TagEntry { tag: 2000, value: SptValue::String("bark.dds".into()), offset: 0 },
                TagEntry { tag: 4003, value: SptValue::String("leaf.dds".into()), offset: 0 },
                TagEntry { tag: 6000, value: SptValue::String("BezierSpline 0".into()), offset: 0 },
                TagEntry { tag: 6001, value: SptValue::String("BezierSpline 1".into()), offset: 0 },
                TagEntry { tag: 2001, value: SptValue::U32(0x44898000), offset: 0 },
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
                TagEntry { tag: 6000, value: SptValue::String("a".into()), offset: 0 },
                TagEntry { tag: 6000, value: SptValue::String("b".into()), offset: 0 },
                TagEntry { tag: 6001, value: SptValue::String("c".into()), offset: 0 },
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
