//! SpeedTree variant dispatch.
//!
//! Three known wire variants ship in Bethesda games:
//!
//! - **SpeedTree 4.x** — Oblivion (2006). Distinct enough from 5.x
//!   that it likely needs its own parser; the recon harness will
//!   confirm.
//! - **SpeedTree 5.x (FO3 era)** — Fallout 3 (2008).
//! - **SpeedTree 5.x (FNV era)** — Fallout New Vegas (2010). Likely
//!   identical to FO3 on the wire (both ship the same SpeedTree
//!   middleware build per the games' release timeline) but kept as
//!   a separate variant so the format-notes corpus stats can pin or
//!   refute the assumption.
//!
//! Skyrim and later dropped `.spt` entirely. Any `.spt` showing up in
//! a Skyrim+ data path is mod content of unknown provenance and
//! lands as `Unknown` for safe rejection by the SpeedTree importer.
//!
//! Per the No-Guessing policy in project memory: this enum captures
//! only what we *know* from the games' release record. Concrete magic
//! bytes / signatures are populated by the recon harness as it
//! observes them in the corpus, not invented here.

/// On-wire SpeedTree binary variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpeedTreeVariant {
    /// SpeedTree 4.x — Oblivion. Distinct header layout from 5.x.
    V4Oblivion,
    /// SpeedTree 5.x as it ships in Fallout 3.
    V5Fo3,
    /// SpeedTree 5.x as it ships in Fallout New Vegas.
    V5Fnv,
    /// File didn't match any known signature. The recon harness
    /// reports these by-game-folder so we can investigate stragglers
    /// (mod content, FaceGen-tooled trees, etc.).
    Unknown,
}

impl SpeedTreeVariant {
    /// Human-readable variant tag for logs / format-notes tables.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::V4Oblivion => "V4_Oblivion",
            Self::V5Fo3 => "V5_FO3",
            Self::V5Fnv => "V5_FNV",
            Self::Unknown => "Unknown",
        }
    }
}

/// Magic prefix observed by the recon harness across **every** vanilla
/// `.spt` in Fallout 3 / FNV / Oblivion (133 files total, 2026-05-09):
///
/// ```text
/// offset 0  : u32 little-endian = 1000  (0xE8 0x03 0x00 0x00)
/// offset 4  : u32 little-endian =   12  (0x0C 0x00 0x00 0x00)
/// offset 8  : 12 ASCII bytes    = "__IdvSpt_02_"
/// ```
///
/// The `1000` u32 is presumed to be a SpeedTree wire-format version
/// code (no spec available — observation only). The 12-byte string
/// is the IDV SpeedTree Reference Application identifier; it appears
/// verbatim across all three games' BSAs, suggesting the on-disk
/// format is unified across the pre-Skyrim era and a single parser
/// can target all three. The body section layout downstream of this
/// header is *not* yet confirmed unified — that's the next recon pass.
///
/// See `crates/spt/docs/format-notes.md` for the full corpus stats.
pub const MAGIC_HEAD: &[u8] = &[
    0xE8, 0x03, 0x00, 0x00, // u32: 1000
    0x0C, 0x00, 0x00, 0x00, // u32: 12 (length of identifier)
    b'_', b'_', b'I', b'd', b'v', b'S', b'p', b't', b'_', b'0', b'2', b'_',
];

/// Detect a SpeedTree binary's variant from its leading bytes.
///
/// Today every vanilla `.spt` across Oblivion / FO3 / FNV ships the
/// same `__IdvSpt_02_` magic, so the matcher is conservative: any
/// file beginning with [`MAGIC_HEAD`] is recognised as a SpeedTree
/// binary, but we can't yet tell Oblivion's body from FO3/FNV's body
/// at the magic-prefix level. Caller-provided context (which BSA the
/// file came from, or the TREE record's ESM `GameKind`) is the right
/// way to pick V4Oblivion vs V5Fnv until the body-section recon pass
/// proves them unified or splits them.
///
/// `Unknown` is returned for every input that doesn't begin with
/// `MAGIC_HEAD` — a defensive gate the SpeedTree importer uses to
/// reject mod content of unknown provenance and fall back to the
/// placeholder billboard.
pub fn detect_variant(bytes: &[u8]) -> SpeedTreeVariant {
    if bytes.starts_with(MAGIC_HEAD) {
        // Without body-level disambiguation, we can't tell V4 from V5
        // here. Default to V5Fnv (the modal case in the planned
        // Phase 1 ship — FNV is Tier-1) and let the caller override
        // via game-context if needed.
        SpeedTreeVariant::V5Fnv
    } else {
        SpeedTreeVariant::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_variant_unknown_for_non_speedtree_inputs() {
        assert_eq!(detect_variant(&[]), SpeedTreeVariant::Unknown);
        assert_eq!(detect_variant(&[0]), SpeedTreeVariant::Unknown);
        assert_eq!(
            detect_variant(&[0xDE, 0xAD, 0xBE, 0xEF]),
            SpeedTreeVariant::Unknown
        );
        let long = vec![0xAA; 4096];
        assert_eq!(detect_variant(&long), SpeedTreeVariant::Unknown);
    }

    /// Recon-pinned regression: any file beginning with the observed
    /// `__IdvSpt_02_` magic must round-trip through `detect_variant`
    /// as a recognized SpeedTree binary. Vanilla 133 files across
    /// FNV/FO3/Oblivion all share this prefix.
    #[test]
    fn detect_variant_recognises_idvspt_magic() {
        // Bare magic — exactly 20 bytes, no body. Still recognized.
        assert_eq!(detect_variant(MAGIC_HEAD), SpeedTreeVariant::V5Fnv);

        // Magic + arbitrary trailing bytes. Recognized.
        let mut bytes = MAGIC_HEAD.to_vec();
        bytes.extend_from_slice(&[0u8; 64]);
        assert_eq!(detect_variant(&bytes), SpeedTreeVariant::V5Fnv);

        // Magic with one byte flipped — `0xE8 03 00 01` instead of
        // `0xE8 03 00 00`. Must reject so the SpeedTree importer
        // falls back to the placeholder billboard.
        let mut tampered = MAGIC_HEAD.to_vec();
        tampered[3] = 0x01;
        assert_eq!(detect_variant(&tampered), SpeedTreeVariant::Unknown);

        // Magic prefix is exactly 20 bytes — anything shorter must
        // be rejected (no partial-match leakage).
        assert_eq!(
            detect_variant(&MAGIC_HEAD[..MAGIC_HEAD.len() - 1]),
            SpeedTreeVariant::Unknown
        );
    }

    #[test]
    fn variant_tag_round_trips() {
        for v in [
            SpeedTreeVariant::V4Oblivion,
            SpeedTreeVariant::V5Fo3,
            SpeedTreeVariant::V5Fnv,
            SpeedTreeVariant::Unknown,
        ] {
            // Tag must be non-empty for log readability.
            assert!(!v.tag().is_empty(), "{:?}", v);
        }
    }
}
