//! BSA archive reader for Bethesda Softworks Archives.
//!
//! Supports BSA version 104 (Oblivion, Fallout 3, Fallout New Vegas).
//!
//! # Usage
//! ```ignore
//! let archive = byroredux_bsa::BsaArchive::open("Fallout - Meshes.bsa")?;
//! let data = archive.extract("meshes\\clutter\\food\\beerbottle01.nif")?;
//! ```

mod archive;

pub use archive::BsaArchive;
