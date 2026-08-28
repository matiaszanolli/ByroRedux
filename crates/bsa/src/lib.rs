//! BSA/BA2 archive readers for Bethesda's engine lineage.
//!
//! - [`BsaArchive`] handles BSA v103 (Oblivion), v104 (Fallout 3/NV, Skyrim LE),
//!   and v105 (Skyrim SE, Fallout 4 — BSA variant).
//! - [`Ba2Archive`] handles the BTDX-family BA2 format used by Fallout 4,
//!   Fallout 76, and Starfield, covering both GNRL (general files) and
//!   DX10 (texture) variants with zlib and LZ4 block compression.
//!
//! # Usage
//!
//! `no_run`, not `ignore` (#3348): these need real archives on disk, so they
//! must not execute — but they *should* still compile, which pins the public
//! signatures against drift. Under `ignore` they were compiled only by
//! `cargo test -- --ignored`, where the bare `?` in an implicit `fn main() -> ()`
//! failed to build and made the whole crate's `--ignored` sweep exit non-zero.
//! ```no_run
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // BSA
//!     let bsa = byroredux_bsa::BsaArchive::open("Fallout - Meshes.bsa")?;
//!     let data = bsa.extract("meshes\\clutter\\food\\beerbottle01.nif")?;
//!
//!     // BA2 (Fallout 4)
//!     let ba2 = byroredux_bsa::Ba2Archive::open("Fallout4 - Meshes.ba2")?;
//!     let data = ba2.extract("meshes/interiors/desk01.nif")?;
//!     Ok(())
//! }
//! ```

mod archive;
mod ba2;
mod csg;
mod safety;

pub use archive::BsaArchive;
pub use ba2::{Ba2Archive, Ba2Variant};
pub use csg::{bscrc32, csg_name_hash, CsgArchive, CSG_CHUNK_SIZE};
pub use safety::{MAX_CHUNK_BYTES, MAX_ENTRY_COUNT};
