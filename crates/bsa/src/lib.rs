//! BSA/BA2 archive readers for Bethesda's engine lineage.
//!
//! - [`BsaArchive`] handles BSA v103 (Oblivion), v104 (Fallout 3/NV, Skyrim LE),
//!   and v105 (Skyrim SE, Fallout 4 — BSA variant).
//! - [`Ba2Archive`] handles the BTDX-family BA2 format used by Fallout 4,
//!   Fallout 76, and Starfield, covering both GNRL (general files) and
//!   DX10 (texture) variants with zlib and LZ4 block compression.
//!
//! # Usage
//! ```ignore
//! // BSA
//! let bsa = byroredux_bsa::BsaArchive::open("Fallout - Meshes.bsa")?;
//! let data = bsa.extract("meshes\\clutter\\food\\beerbottle01.nif")?;
//!
//! // BA2 (Fallout 4)
//! let ba2 = byroredux_bsa::Ba2Archive::open("Fallout4 - Meshes.ba2")?;
//! let data = ba2.extract("meshes/interiors/desk01.nif")?;
//! ```

mod archive;
mod ba2;

pub use archive::BsaArchive;
pub use ba2::{Ba2Archive, Ba2Variant};
