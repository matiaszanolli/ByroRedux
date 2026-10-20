//! BSA/BA2-backed texture and mesh extraction.

mod actor_graph;
mod animation;
mod archetype_idle;
mod archive;
pub(crate) mod audio;
pub(crate) mod material;
mod object_graph;
mod script;
pub(crate) mod terrain_height;
mod texture;
mod texture_prefetch;

pub(crate) use actor_graph::*;
pub(crate) use animation::*;
pub(crate) use archetype_idle::*;
pub(crate) use archive::*;
pub(crate) use audio::*;
pub(crate) use material::*;
pub(crate) use object_graph::*;
pub(crate) use script::*;
pub(crate) use texture::*;
pub(crate) use texture_prefetch::{prefetch_textures, PrefetchStats};

// `normalize_mesh_path` is `pub` (used outside the crate); re-export it at
// that visibility explicitly — a `pub(crate) use` glob can't carry a `pub`
// item back out (E0364).
pub use archive::normalize_mesh_path;

#[cfg(test)]
mod tests;
