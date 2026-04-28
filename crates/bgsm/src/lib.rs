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
    let mut r = reader::Reader::new(bytes);
    BgsmFile::parse(&mut r)
}

/// Parse a file whose magic is known to be `"BGEM"` (0x4d454742).
pub fn parse_bgem(bytes: &[u8]) -> Result<BgemFile> {
    let mut r = reader::Reader::new(bytes);
    BgemFile::parse(&mut r)
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
}
