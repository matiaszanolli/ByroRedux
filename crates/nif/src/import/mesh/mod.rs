//! Geometry extraction from NIF blocks.
//!
//! Split out of the 2 212-LOC monolith into per-shape-flavor +
//! cross-cutting-concern submodules:
//!
//! - [`tangent`]      — tangent-space extraction + Mikkelsen-style synthesis
//! - [`ni_tri_shape`] — classic `NiTriShape` (Oblivion / FO3 / FNV / Skyrim LE)
//! - [`bs_tri_shape`] — packed-half `BSTriShape` (Skyrim SE / FO4 / FO76)
//! - [`bs_geometry`]  — Starfield `BSGeometry` external + internal branches
//! - [`sse_recon`]    — Skyrim-SE skinned-geometry reconstruction (#559)
//! - [`decode`]       — half-float / byte-normal / LE readers
//! - [`material_path`] — `material_path_from_name` (`.bgsm` / `.bgem` capture)
//! - [`skin`]         — skinning extraction (#151) + bone-pose flattening
//!
//! Cross-sibling helpers are re-exported at this module's namespace
//! with `pub(crate)` visibility so each sibling can keep its
//! `use super::*;` glob without per-file boilerplate. The original
//! `pub(super)` API surface (`extract_mesh`, `extract_bs_tri_shape`,
//! `GeomData<'a>`, `material_path_from_name`, the skin extractors)
//! is preserved one-for-one through these re-exports.

mod bs_geometry;
mod bs_tri_shape;
mod decode;
mod material_path;
mod ni_tri_shape;
mod skin;
mod sse_recon;
mod tangent;

pub(crate) use bs_geometry::*;
pub(crate) use bs_tri_shape::*;
pub(crate) use decode::*;
pub(crate) use material_path::*;
pub(crate) use ni_tri_shape::*;
pub(crate) use skin::*;
pub(crate) use sse_recon::*;
pub(crate) use tangent::*;

#[cfg(test)]
mod bs_geometry_tangent_tests;
#[cfg(test)]
mod bs_tri_shape_partition_remap_tests;
#[cfg(test)]
mod bs_tri_shape_shader_flag_tests;
#[cfg(test)]
mod material_path_capture_tests;
#[cfg(test)]
mod shader_type_fields_tests;
#[cfg(test)]
mod skin_tests;
#[cfg(test)]
mod sse_skin_geometry_reconstruction_tests;
#[cfg(test)]
mod tangent_convention_tests;
