//! SpeedTree `.spt` binary parser — pre-Skyrim era.
//!
//! Oblivion (2006) ships SpeedTree 4.x, Fallout 3 (2008) and Fallout
//! New Vegas (2010) ship SpeedTree 5.x. Both eras serialise as
//! external `.spt` files referenced by TREE records' MODL field.
//! Skyrim and later dropped `.spt` entirely — tree geometry got baked
//! into NIFs rooted at `BSTreeNode`. The Skyrim+ path lives in
//! `crates/nif/`; this crate is for the pre-Skyrim era only.
//!
//! ## Why a clean-room reverse-engineering job
//!
//! IDV (the SpeedTree vendor) never released a public file format
//! spec. OpenMW explicitly skips `.spt` files with `"Ignoring
//! SpeedTree data file"` (`components/resource/scenemanager.cpp`),
//! and OpenMW's TES4 TREE record loader (`loadtree.cpp`) skips every
//! SpeedTree-specific subrecord. There is no upstream decoder to
//! lean on. The only path forward is corpus-driven reverse
//! engineering: stat-sweep every `.spt` in vanilla FO3 / FNV /
//! Oblivion BSAs, identify common headers, partition into sections,
//! validate geometry/leaf splits against the cross-referenced TREE
//! record's ICON / SNAM / CNAM / BNAM data.
//!
//! See `docs/format-notes.md` for the running observation log. The
//! recon harness (feature `recon`) is the tool that produces it.
//!
//! ## Project policy — no SDK linkage, no SDK paraphrasing
//!
//! Per the project's proprietary-dependency rule (memory:
//! `proprietary_dependencies.md`), we parse SpeedTree data but never
//! link the IDV SDK, copy SDK headers, or paraphrase SDK documentation
//! verbatim. Findings in `docs/format-notes.md` describe only black-box
//! observations from the corpus.
//!
//! ## Status
//!
//! Ships the version dispatch ([`detect_variant`]), the TLV tag walker
//! ([`parse_spt`] → [`SptScene`]), and the NIF-side import
//! ([`import_spt_scene`]). The walker partitions the `.spt` container into
//! its tagged sections; the import drives the placeholder fallback — a
//! yaw-billboard quad keyed on the TREE record's ICON — so trees stay
//! visible while a full byte-level geometry parser remains future work. The
//! `recon` harness (feature `recon`) is the corpus-analysis tool behind
//! `docs/format-notes.md` (SPT-NEW-02: this block previously claimed only the
//! version dispatch + recon harness shipped).

pub mod import;
pub mod parser;
pub mod scene;
pub mod stream;
pub mod tag;
pub mod version;

#[cfg(feature = "recon")]
pub mod recon;

pub use import::{import_spt_scene, SptImportParams};
pub use parser::{parse_spt, TAG_MAX, TAG_MIN};
pub use scene::{SptScene, SptValue, TagEntry};
pub use stream::SptStream;
pub use tag::{dispatch_tag, SptTagKind};
pub use version::{detect_variant, SpeedTreeVariant};
