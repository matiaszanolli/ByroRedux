//! `.spt` parameter-section tag dictionary.
//!
//! Maps each known tag value to its on-wire payload kind. Derived
//! from `spt_transitions` runs over the FNV / FO3 / Oblivion BSAs
//! (133 files, see `docs/format-notes.md` 2026-05-09 entry).
//!
//! Conservative-by-design: any tag not in this table returns
//! [`SptTagKind::Unknown`], and the parser surfaces that as a
//! diagnostic without aborting (mod content / DLC may carry tags
//! we haven't observed yet).

/// On-wire payload shape for a parameter-section tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SptTagKind {
    /// Tag has no payload — the next 4 bytes are another tag.
    /// Section / structure markers (e.g. `1001`, `1016`).
    Bare,
    /// 1-byte payload (`bool` or `u8`). E.g. `2002`, `3003`.
    U8,
    /// 4-byte payload (`u32` / `f32`). The interpretation as int vs
    /// float is per-tag and stays the consumer's responsibility —
    /// the walker stores raw bits.
    U32,
    /// 12-byte payload (`[f32; 3]` — colour tint or coord triple).
    Vec3,
    /// Fixed-size opaque byte payload of N bytes (4 ≤ N ≤ 255).
    /// Used for tags whose interior layout we haven't pinned but
    /// whose total payload size is constant across the corpus
    /// (e.g. tag `8003` = 52 bytes, tag `13013` = 7 bytes).
    FixedBytes(u8),
    /// `u32` length prefix followed by `length` raw bytes — almost
    /// always ASCII. Texture paths (`2000`, `4003`), BezierSpline
    /// curve text blobs (`6000-6007`), names (`13001`).
    String,
    /// `u32 count` prefix followed by `count * stride` raw bytes.
    /// Used by tags that ship variable-sized binary arrays
    /// (per `spt_transitions` histogram analysis):
    /// - `10002` ships an `(u32 N + N bytes)` blob (stride 1).
    /// - `10003` ships an `(u32 N + N × 8 bytes)` blob (stride 8).
    /// Stride is per-tag, encoded in the dispatch.
    ArrayBytes { stride: u8 },
    /// Tag is recognised but its layout is non-uniform or under
    /// further investigation. The walker bails the moment it hits
    /// one. Today this band only covers the false-tag confounders
    /// from `spt_transitions` (string-length values that happened
    /// to fall in the tag range — e.g. `4096`, `5376`).
    Unknown,
    /// Bimodal: tag carries an optional length-prefixed string
    /// payload. Some files emit the tag bare (0 payload bytes, next
    /// thing is another tag); others emit it followed by a `u32`
    /// length and `length` raw ASCII bytes (a BezierSpline curve
    /// blob).
    ///
    /// Disambiguation: the walker peeks the next `u32`. If that value
    /// is a known dictionary tag → current entry is `Bare`. Otherwise
    /// → treat as `String`. This is robust against the observed
    /// vanilla corpus where the only bimodal tag (`13005`) carries a
    /// 104-byte curve blob whose length doesn't coincide with any
    /// dictionary tag value.
    ///
    /// Documented at `crates/spt/docs/format-notes.md` under "tag
    /// 13005 bimodal payload" (added with #999).
    MaybeStringElseBare,
}

/// Look up a tag's payload kind in the recovered dictionary.
///
/// Tags this function recognises are listed in the
/// `docs/format-notes.md` 2026-05-09 table. Anything else returns
/// [`SptTagKind::Unknown`] — the parser's caller decides whether
/// that's a fatal condition or a soft warning.
pub fn dispatch_tag(tag: u32) -> SptTagKind {
    match tag {
        // ── Bare markers (0-byte payload) ─────────────────────────
        // Section / structure markers. Each consumes only its own
        // 4 bytes; the next thing in the stream is another tag.
        1001 | 1002 | 1003 | 1004 | 1005 | 1007 | 1008 | 1009 | 1010
        | 1011 | 1012 | 1015 | 1016 | 1017 | 5644
        | 8000 | 8001
        | 9000 | 9001 | 9005 | 9006
        | 10000 | 10001
        | 11000 | 11001
        | 12000 | 12001
        | 13000 => SptTagKind::Bare,

        // #999 — bimodal. 109 vanilla Oblivion files emit 13005 bare;
        // 4 outliers (treems14canvasfreesu, treecottonwoodsu,
        // shrubms14boxwood, treems14willowoakyoungsu) emit it with an
        // optional 104-byte BezierSpline curve payload. The walker
        // peeks the next u32 and decides per-instance.
        13005 => SptTagKind::MaybeStringElseBare,

        // ── 1-byte payload (u8 / bool) ────────────────────────────
        2002 | 3003 | 3006 | 3009
        | 4000  // missed in the first dispatch pass — every vanilla
                // `.spt` carries it at ~offset 4500-5800 and the
                // walker bailed when it hit Unknown there. modal=1B,
                // conf=100% per spt_transitions.
        | 5006
        | 6015 | 6016
        | 13007 => SptTagKind::U8,

        // ── 4-byte payload (u32 / f32) ────────────────────────────
        1006 | 1014
        | 2001 | 2003 | 2005 | 2006 | 2007
        | 3000 | 3001 | 3002 | 3004 | 3005 | 3007 | 3008 | 3010
        | 4002 | 4007
        | 5005
        | 6008 | 6009 | 6010 | 6011 | 6012 | 6013 | 6014
        | 8002 | 8004 | 8006 | 8007 | 8008
        | 9002 | 9003 | 9004 | 9007 | 9008 | 9009 | 9010 | 9011 | 9012 | 9013 | 9014
        | 10004
        | 11002
        | 13002 | 13003 | 13004 | 13006 | 13009 | 13010 | 13011 | 13012 => SptTagKind::U32,

        // ── 12-byte payload (vec3) ────────────────────────────────
        4001 | 4004 | 4005 | 4006
        | 5000 | 5001 | 5002 | 5003 | 5004 => SptTagKind::Vec3,

        // ── Fixed-size opaque structs ─────────────────────────────
        // 52 bytes — leaf-billboard descriptors per `spt_transitions`.
        8003 | 8005 | 8009 => SptTagKind::FixedBytes(52),
        // 11 bytes — observed at 92 % confidence on tag 13008.
        13008 => SptTagKind::FixedBytes(11),
        // 7 bytes — tag 13013 ships a 7-byte struct (likely u32 + u16 + u8).
        13013 => SptTagKind::FixedBytes(7),
        // 16 bytes — tag 12002 (4 × f32 = matrix row?).
        12002 => SptTagKind::FixedBytes(16),
        // 20 bytes — tag 12003.
        12003 => SptTagKind::FixedBytes(20),

        // ── String payload (u32 length + body) ────────────────────
        2000        // bark texture path
        | 4003      // leaf texture path
        | 6000 | 6001 | 6002 | 6003 | 6004 | 6005 | 6006 | 6007  // BezierSpline curves
        | 6017      // (string-prefix at 30 % confidence; treat as string)
        | 13001     // variable-length payload (62-525 B per spt_transitions)
        => SptTagKind::String,

        // ── Length-prefixed binary arrays ─────────────────────────
        // Bimodal payload-size histograms in `spt_transitions` decode
        // cleanly when read as `u32 count + count × stride` blobs.
        // 10002 — stride 1 (length value = byte count): histogram
        //   buckets 4 / 68 / 100 / 132 / 164 / 196 = 4 + N×64 with
        //   the count u32 = N×64.
        10002 => SptTagKind::ArrayBytes { stride: 1 },
        // 10003 — stride 8: 4B (count=0) and 36B (count=4) modes.
        // 4 + 4×8 = 36.
        10003 => SptTagKind::ArrayBytes { stride: 8 },

        _ => SptTagKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_markers_round_trip() {
        for tag in [1001, 1002, 1016, 1017, 5644, 8000, 9000, 13000] {
            assert_eq!(dispatch_tag(tag), SptTagKind::Bare, "tag {} bare", tag);
        }
    }

    /// #999 — tag 13005 was previously `Bare`, but the 4 Oblivion
    /// outliers (`treems14canvasfreesu`, `treecottonwoodsu`,
    /// `shrubms14boxwood`, `treems14willowoakyoungsu`) emit it with an
    /// optional 104-byte BezierSpline curve payload. The walker now
    /// peeks the next u32 to disambiguate.
    #[test]
    fn tag_13005_is_maybe_string_else_bare() {
        assert_eq!(dispatch_tag(13005), SptTagKind::MaybeStringElseBare);
    }

    #[test]
    fn u8_payload_tags() {
        for tag in [2002, 3003, 3006, 3009, 4000, 5006, 6015, 6016, 13007] {
            assert_eq!(dispatch_tag(tag), SptTagKind::U8, "tag {} u8", tag);
        }
    }

    #[test]
    fn u32_payload_tags() {
        for tag in [
            1006, 1014, 2001, 2003, 2005, 2006, 2007, 6008, 6014, 9002, 13006,
        ] {
            assert_eq!(dispatch_tag(tag), SptTagKind::U32, "tag {} u32", tag);
        }
    }

    #[test]
    fn vec3_payload_tags() {
        for tag in [4001, 4004, 4005, 4006, 5000, 5001, 5002, 5003, 5004] {
            assert_eq!(dispatch_tag(tag), SptTagKind::Vec3, "tag {} vec3", tag);
        }
    }

    #[test]
    fn fixed_byte_payload_tags() {
        assert_eq!(dispatch_tag(8003), SptTagKind::FixedBytes(52));
        assert_eq!(dispatch_tag(8005), SptTagKind::FixedBytes(52));
        assert_eq!(dispatch_tag(8009), SptTagKind::FixedBytes(52));
        assert_eq!(dispatch_tag(13008), SptTagKind::FixedBytes(11));
        assert_eq!(dispatch_tag(13013), SptTagKind::FixedBytes(7));
        assert_eq!(dispatch_tag(12002), SptTagKind::FixedBytes(16));
        assert_eq!(dispatch_tag(12003), SptTagKind::FixedBytes(20));
    }

    #[test]
    fn string_payload_tags() {
        for tag in [
            2000, 4003, 6000, 6001, 6002, 6003, 6004, 6005, 6006, 6007, 6017, 13001,
        ] {
            assert_eq!(dispatch_tag(tag), SptTagKind::String, "tag {} string", tag);
        }
    }

    #[test]
    fn unknown_for_out_of_dictionary_tags() {
        // String-length confounders (curve body lengths in the
        // `[100, 200]` band that the analyser misclassified as tags)
        // and hex-aligned multiples — both false-tag confounders.
        for tag in [100, 110, 4096, 5376, 11776, 13568] {
            assert_eq!(dispatch_tag(tag), SptTagKind::Unknown);
        }
        // Far-out-of-range values.
        for tag in [0, 1, 50, 19_985, u32::MAX] {
            assert_eq!(dispatch_tag(tag), SptTagKind::Unknown);
        }
    }
}
