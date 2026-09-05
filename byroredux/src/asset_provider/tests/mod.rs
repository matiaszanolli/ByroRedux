//! Asset-provider tests, split by topic (#2411 / TD1-010).
//!
//! Was one 2051-LOC file whose eight topic-divider comments already marked
//! the seams; this mirrors the `crates/nif/src/import/tests/` precedent
//! (#2311). Zero logic change — every test moved verbatim.

mod archive_precedence;
mod archive_siblings;
mod bgsm_merge;
mod facegen_texture_fallback;
mod material_flags;
mod material_path;
mod starfield_mat;

use byroredux_nif::import::ImportedMesh;

pub(super) fn imported_mesh_with_material_path(
    pool: &mut byroredux_core::string::StringPool,
    path: &str,
) -> ImportedMesh {
    // The merge helper only touches material-flow fields. Start from the
    // shared geometry/material defaults so this fixture cannot drift as the
    // import contract grows.
    let mut mesh = ImportedMesh::from_geometry(
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    mesh.material.material_path = Some(pool.intern(path));
    mesh.material.alpha_threshold = 0.0;
    mesh.material.specular_strength = 1.0;
    mesh.material.glossiness = 80.0;
    mesh.material.env_map_scale = 1.0;
    mesh
}
