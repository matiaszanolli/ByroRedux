//! Fallout 4 / Skyrim SE / FO76 external material file (BGSM v1–v22 / BGEM v1–v22) parser.
//!
//! Not supported: Starfield uses `.mat` JSON descriptors and a binary
//! `materialsbeta.cdb` component database — a different format entirely.
//! Neither is handled here. See the tracking issue for the Starfield `.mat`/`.cdb` parser.
//!
//! Fallout 4 replaced the inline NIF shader-flag block with *external*
//! material files referenced by `BSLightingShaderProperty.net.name`
//! (lit meshes) or `BSEffectShaderProperty.net.name` (effect meshes).
//! Parsing these is required to drive the material pipeline — without
//! it every FO4 surface renders as the NIF's fallback defaults.
//!
//! Format reference: [ousnius/Material-Editor](
//! https://github.com/ousnius/Material-Editor) (C#, authoritative) —
//! cloned to `/mnt/data/src/reference/Material-Editor/`.
//!
//! Supported:
//! - BGSM v1–v22 (lit material)
//! - BGEM v1–v22 (effect material)
//!
//! Template inheritance via `root_material_path` is implemented in
//! [`template::resolve`] with an LRU cache so chain-walks don't dominate
//! cell-load time.
//!
//! # Example
//!
//! ```no_run
//! use byroredux_bgsm::{parse, MaterialFile};
//!
//! let bytes: Vec<u8> = std::fs::read("material.bgsm").unwrap();
//! match parse(&bytes).unwrap() {
//!     MaterialFile::Bgsm(m) => println!("lit — diffuse={:?}", m.diffuse_texture),
//!     MaterialFile::Bgem(m) => println!("effect — base={:?}", m.base_texture),
//! }
//! ```
//!
//! See issue #490 for the crate scope + follow-ups (#491 corpus test,
//! #493 asset_provider integration).

#![allow(clippy::too_many_lines)] // BGSM deserialize branches are inherently long

pub mod base;
pub mod bgem;
pub mod bgsm;
mod reader;
pub mod template;

pub use base::{AlphaBlendMode, BaseMaterial, ColorRgb, MaskWriteFlags};
pub use bgem::BgemFile;
pub use bgsm::BgsmFile;
pub use template::{TemplateCache, TemplateResolver};

/// File kind inferred from the 4-byte leading magic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialKind {
    /// `"BGSM"` — lit material with optional template chain.
    Bgsm,
    /// `"BGEM"` — effect material, no inheritance.
    Bgem,
}

/// Inspect the leading 4 bytes to determine whether a buffer is a BGSM or
/// BGEM file. Returns `None` when the buffer is too short or carries an
/// unrecognised magic.
pub fn detect_kind(bytes: &[u8]) -> Option<MaterialKind> {
    match bytes.get(..4)? {
        b"BGSM" => Some(MaterialKind::Bgsm),
        b"BGEM" => Some(MaterialKind::Bgem),
        _ => None,
    }
}

/// Error returned by the parser.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unexpected EOF at offset {offset} (need {need} bytes, have {have})")]
    UnexpectedEof {
        offset: usize,
        need: usize,
        have: usize,
    },

    #[error("bad magic: got {got:#010x}, expected BGSM (0x4d534742) or BGEM (0x4d454742)")]
    BadMagic { got: u32 },

    #[error("invalid utf-8 in string at offset {offset}: {source}")]
    InvalidString {
        offset: usize,
        #[source]
        source: std::string::FromUtf8Error,
    },

    #[error("implausible string length {len} at offset {offset} (remaining: {remaining})")]
    StringTooLong {
        offset: usize,
        len: u32,
        remaining: usize,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

/// One parsed material file — either a BGSM (lit) or BGEM (effect).
#[derive(Debug, Clone)]
pub enum MaterialFile {
    Bgsm(BgsmFile),
    Bgem(BgemFile),
}

impl MaterialFile {
    /// Common prefix fields shared by both variants.
    pub fn base(&self) -> &BaseMaterial {
        match self {
            Self::Bgsm(m) => &m.base,
            Self::Bgem(m) => &m.base,
        }
    }

    /// Optional `root_material_path` (template parent) — only BGSM has
    /// this field. BGEM has no template inheritance.
    pub fn root_material_path(&self) -> Option<&str> {
        match self {
            Self::Bgsm(m) => m.root_material_path.as_deref().filter(|s| !s.is_empty()),
            Self::Bgem(_) => None,
        }
    }
}

/// Parse a BGSM or BGEM file, dispatching on the leading magic.
///
/// Call this when you don't know the type upfront — the magic is in
/// the first 4 bytes. Use [`parse_bgsm`] / [`parse_bgem`] directly
/// when the variant is known (skips the magic peek).
pub fn parse(bytes: &[u8]) -> Result<MaterialFile> {
    if bytes.len() < 4 {
        return Err(Error::UnexpectedEof {
            offset: 0,
            need: 4,
            have: bytes.len(),
        });
    }
    let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    match magic {
        bgsm::SIGNATURE => parse_bgsm(bytes).map(MaterialFile::Bgsm),
        bgem::SIGNATURE => parse_bgem(bytes).map(MaterialFile::Bgem),
        got => Err(Error::BadMagic { got }),
    }
}

/// Parse a file whose magic is known to be `"BGSM"` (0x4d534742).
pub fn parse_bgsm(bytes: &[u8]) -> Result<BgsmFile> {
    parse_bgsm_diag(bytes).0
}

/// The newest BGSM/BGEM layout this crate decodes: 22 (FO76). Vanilla
/// uses only v2 (FO4) and v22 (FO76); a higher version means a layout
/// this crate has never seen and silently decoded wrong (#4664).
pub const NEWEST_KNOWN_VERSION: u32 = 22;

/// Post-parse diagnostics (#4672, #4664) — everything the caller may
/// want to warn about once, with the path it owns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseDiagnostics {
    /// A string field decoded with UTF-8 replacement (#4672).
    pub lossy_strings: bool,
    /// Bytes left over after the parse finished — layout drift, or a
    /// version this crate decoded against the wrong shape (#4664).
    pub unconsumed_bytes: usize,
    /// The file's version when it exceeds the newest layout this crate
    /// knows (22, FO76). `None` at or below the ceiling.
    pub version_over_ceiling: Option<u32>,
}

/// Parse a file whose magic is known to be `"BGSM"` (0x4d534742).
pub fn parse_bgsm_diag(bytes: &[u8]) -> (Result<BgsmFile>, ParseDiagnostics) {
    let mut r = reader::Reader::new(bytes);
    let file = BgsmFile::parse(&mut r);
    let version = file.as_ref().ok().map(|f| f.base.version);
    (
        file,
        ParseDiagnostics {
            lossy_strings: r.had_replacement(),
            unconsumed_bytes: r.remaining(),
            version_over_ceiling: version.filter(|&v| v > NEWEST_KNOWN_VERSION),
        },
    )
}

/// Parse a file whose magic is known to be `"BGEM"` (0x4d454742).
pub fn parse_bgem(bytes: &[u8]) -> Result<BgemFile> {
    parse_bgem_diag(bytes).0
}

/// Like [`parse_bgem`]; see [`parse_bgsm_diag`] and
/// [`ParseDiagnostics`] (#4672, #4664).
pub fn parse_bgem_diag(bytes: &[u8]) -> (Result<BgemFile>, ParseDiagnostics) {
    let mut r = reader::Reader::new(bytes);
    let file = BgemFile::parse(&mut r);
    let version = file.as_ref().ok().map(|f| f.base.version);
    (
        file,
        ParseDiagnostics {
            lossy_strings: r.had_replacement(),
            unconsumed_bytes: r.remaining(),
            version_over_ceiling: version.filter(|&v| v > NEWEST_KNOWN_VERSION),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dispatches_on_magic() {
        // Minimum FO4 BGSM v2: 4-byte magic + 4-byte version + the
        // common-prefix scalars + four empty texture strings + the
        // BGSM-specific v2 trailer up to SkewSpecularAlpha. Hand-rolled
        // in the `bgsm::tests` module; smoke-tested here via `parse`
        // dispatch on magic alone.
        let bgsm_bytes = bgsm::tests::minimal_v2_bytes();
        match parse(&bgsm_bytes).expect("parse BGSM") {
            MaterialFile::Bgsm(m) => assert_eq!(m.base.version, 2),
            MaterialFile::Bgem(_) => panic!("dispatched to BGEM for BGSM magic"),
        }

        let bgem_bytes = bgem::tests::minimal_v2_bytes();
        match parse(&bgem_bytes).expect("parse BGEM") {
            MaterialFile::Bgem(m) => assert_eq!(m.base.version, 2),
            MaterialFile::Bgsm(_) => panic!("dispatched to BGSM for BGEM magic"),
        }
    }

    /// #4664 (PAR-D3-2026-09-21-01) — both silent-drift signals: a file
    /// whose version exceeds the newest known layout (22) reports it, and
    /// a file that leaves bytes unconsumed reports the count. The
    /// vanilla sweep (0/36,888 leaving bytes, only v2/v22 in use) makes
    /// both signals zero-noise on real content.
    #[test]
    fn parse_diagnostics_flag_unknown_version_and_unconsumed_bytes() {
        // Trailing junk: fully-consumed v2 plus extra bytes — the
        // unconsumed half of the signal, driven through the real parse.
        let mut bytes = bgsm::tests::minimal_v2_bytes();
        bytes.extend_from_slice(&[0xAB; 7]);
        let (_, diag) = parse_bgsm_diag(&bytes);
        assert_eq!(diag.version_over_ceiling, None, "v2 is at the ceiling");
        assert_eq!(diag.unconsumed_bytes, 7);

        // Version past the ceiling: a post-v22 layout is unconstructable
        // here without reimplementing the format's forks, but a FAILED
        // parse must not claim a ceiling breach either way — the signal
        // only fires for a file that actually decoded to the end.
        let mut bytes = bgsm::tests::minimal_v2_bytes();
        bytes[4..8].copy_from_slice(&23u32.to_le_bytes());
        let (result, diag) = parse_bgsm_diag(&bytes);
        assert!(result.is_err(), "v23 bytes do not fit the v>2 layout");
        assert_eq!(diag.version_over_ceiling, None);
    }

    /// #4664 — the ceiling value itself, pinned so a future "bump the
    /// constant" edit is a reviewed decision (22 = FO76; vanilla uses
    /// only v2 and v22).
    #[test]
    fn newest_known_version_is_fo76_22() {
        assert_eq!(NEWEST_KNOWN_VERSION, 22);
        let d = ParseDiagnostics {
            lossy_strings: false,
            unconsumed_bytes: 3,
            version_over_ceiling: Some(23),
        };
        assert_eq!(d.version_over_ceiling, Some(23));
    }

    #[test]
    fn parse_rejects_bad_magic() {
        let bytes = [0xAAu8, 0xBB, 0xCC, 0xDD, 0, 0, 0, 0];
        match parse(&bytes) {
            Err(Error::BadMagic { got: 0xDDCCBBAA }) => {}
            other => panic!("expected BadMagic, got {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_short_input() {
        let bytes = [0x42, 0x47];
        match parse(&bytes) {
            Err(Error::UnexpectedEof { .. }) => {}
            other => panic!("expected UnexpectedEof, got {other:?}"),
        }
    }

    #[test]
    fn detect_kind_identifies_bgsm_and_bgem() {
        assert_eq!(detect_kind(b"BGSM\x00"), Some(MaterialKind::Bgsm));
        assert_eq!(detect_kind(b"BGEM\x00"), Some(MaterialKind::Bgem));
    }

    #[test]
    fn detect_kind_returns_none_on_bad_magic_or_short_buf() {
        assert_eq!(detect_kind(b""), None);
        assert_eq!(detect_kind(b"BGS"), None);
        assert_eq!(detect_kind(b"NOPE"), None);
    }
}
