//! BSA/BA2-backed texture and mesh extraction.

mod archive;
mod material;
mod script;
mod texture;

pub(crate) use archive::*;
pub(crate) use material::*;
pub(crate) use script::*;
pub(crate) use texture::*;

// `normalize_mesh_path` is `pub` (used outside the crate); re-export it at
// that visibility explicitly — a `pub(crate) use` glob can't carry a `pub`
// item back out (E0364).
pub use archive::normalize_mesh_path;

#[cfg(test)]
mod tests;
