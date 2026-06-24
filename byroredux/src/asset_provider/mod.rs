//! BSA/BA2-backed texture and mesh extraction.

mod archive;
mod texture;
mod script;
mod material;

pub(crate) use archive::*;
pub(crate) use texture::*;
pub(crate) use script::*;
pub(crate) use material::*;

// `normalize_mesh_path` is `pub` (used outside the crate); re-export it at
// that visibility explicitly — a `pub(crate) use` glob can't carry a `pub`
// item back out (E0364).
pub use archive::normalize_mesh_path;

#[cfg(test)]
mod tests;
