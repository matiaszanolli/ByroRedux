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
//! ## Status — Phase 1.2 (recon scaffold)
//!
//! Today this crate ships only the version dispatch and the recon
//! harness. The actual byte-level parser (Phase 1.3) lands once the
//! recon results in `docs/format-notes.md` partition ≥95 % of the FNV
//! corpus into sections. Below that threshold, the SpeedTree
//! compatibility plan ships the placeholder fallback (a yaw-billboard
//! quad keyed on the TREE record's ICON) instead.

pub mod version;

#[cfg(feature = "recon")]
pub mod recon;

pub use version::{detect_variant, SpeedTreeVariant};
