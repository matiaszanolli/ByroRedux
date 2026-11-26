//! Asset-provider tests, split by topic (#2411 / TD1-010).
//!
//! Was one 2051-LOC file whose eight topic-divider comments already marked
//! the seams; this mirrors the `crates/nif/src/import/tests/` precedent
//! (#2311). Zero logic change — every test moved verbatim.

mod archive_precedence;
mod archive_set;
mod archive_siblings;
mod bgsm_diagnostics;
mod bgsm_merge;
mod facegen_texture_fallback;
mod fo4_mswp_corpus;
mod fo4_palette_corpus;
mod material_flags;
mod loose_override;
mod material_path;
mod starfield_mat;
mod starfield_material_opacity;
mod xwma_effects;

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

/// A v1 GNRL BA2 holding `entries` (name, payload, whether the payload is declared zlib-compressed) at `path`: the
/// fixture the archive tests share. An undeclared payload is stored raw.
pub(crate) fn write_ba2(path: &std::path::Path, entries: &[(&str, &[u8], bool)]) {
    let count = entries.len() as u32;
    let data_start = 24u64 + 36 * u64::from(count);
    let payloads: u64 = entries.iter().map(|(_, payload, _)| payload.len() as u64).sum();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BTDX");
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(b"GNRL");
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&(data_start + payloads).to_le_bytes());
    let mut offset = data_start;
    for (_, payload, packed) in entries {
        bytes.extend_from_slice(&[0u8; 16]); // name/ext/dir hashes + flags
        bytes.extend_from_slice(&offset.to_le_bytes());
        bytes.extend_from_slice(&(if *packed { payload.len() as u32 } else { 0 }).to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&0xBAAD_F00Du32.to_le_bytes());
        offset += payload.len() as u64;
    }
    for (_, payload, _) in entries {
        bytes.extend_from_slice(payload);
    }
    for (name, _, _) in entries {
        bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
        bytes.extend_from_slice(name.as_bytes());
    }
    std::fs::write(path, bytes).expect("write temp BA2");
}
