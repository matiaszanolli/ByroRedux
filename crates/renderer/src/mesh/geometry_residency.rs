//! Opt-in, CPU-only census of admitted source geometry. This does not change
//! admission, mesh identity, buffers, or BLAS. Hash collisions are checked
//! against complete vertex/index bytes before counting a duplicate.

use super::{MeshRegistry, index_slice_bytes, vertex_slice_bytes};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Debug, Default, PartialEq, Eq)]
struct ResidencyCensus {
    meshes: usize,
    source_bytes: usize,
    duplicate_meshes: usize,
    duplicate_bytes: usize,
    dedicated_vertex_bytes: u64,
    dedicated_index_bytes: u64,
}

impl MeshRegistry {
    fn geometry_residency_census(&self) -> Option<ResidencyCensus> {
        // During a deferred compaction offsets still describe the old GPU
        // generation, not the compacted CPU pools. Never compare mixed layouts.
        if self.deferred_compaction.is_some() {
            return None;
        }
        let mut census = ResidencyCensus::default();
        let mut buckets: HashMap<u64, Vec<(bool, &[u8], &[u8])>> = HashMap::new();
        for mesh in self.meshes.iter().flatten().filter(|m| m.is_scene_mesh) {
            let vs = mesh.global_vertex_offset as usize;
            let is = mesh.global_index_offset as usize;
            let vertices = vertex_slice_bytes(
                self.pending_vertices
                    .get(vs..vs.checked_add(mesh.vertex_count as usize)?)?,
            );
            let indices = index_slice_bytes(
                self.pending_indices
                    .get(is..is.checked_add(mesh.index_count as usize)?)?,
            );
            let bytes = vertices.len() + indices.len();
            census.meshes += 1;
            census.source_bytes += bytes;
            census.dedicated_vertex_bytes += mesh.vertex_buffer.as_ref().map_or(0, |b| b.size);
            census.dedicated_index_bytes += mesh.index_buffer.as_ref().map_or(0, |b| b.size);
            let mut hash = DefaultHasher::new();
            mesh.rt_capable.hash(&mut hash);
            vertices.hash(&mut hash);
            indices.hash(&mut hash);
            let bucket = buckets.entry(hash.finish()).or_default();
            if bucket
                .iter()
                .any(|&(rt, v, i)| rt == mesh.rt_capable && v == vertices && i == indices)
            {
                census.duplicate_meshes += 1;
                census.duplicate_bytes += bytes;
            } else {
                bucket.push((mesh.rt_capable, vertices, indices));
            }
        }
        Some(census)
    }

    pub(super) fn log_geometry_residency_if_requested(&self) {
        if std::env::var("BYROREDUX_GEOMETRY_CENSUS").as_deref() != Ok("1") {
            return;
        }
        match self.geometry_residency_census() {
            Some(c) => log::warn!(
                "geometry-census: meshes={} vertices={} indices={} source_bytes={} \
                 duplicate_meshes={} duplicate_bytes={} dedicated_vertex_bytes={} \
                 dedicated_index_bytes={} admission_closed={}",
                c.meshes,
                self.pending_vertices.len(),
                self.pending_indices.len(),
                c.source_bytes,
                c.duplicate_meshes,
                c.duplicate_bytes,
                c.dedicated_vertex_bytes,
                c.dedicated_index_bytes,
                self.scene_geometry_admission_closed,
            ),
            None => log::warn!("geometry-census: unavailable during a layout transition"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertex::Vertex;

    #[test]
    fn census_counts_exact_duplicates_without_mutating_mesh_ownership() {
        let mut registry = MeshRegistry::new();
        let vertices = [Vertex::new([1.0; 3], [1.0; 3], [0.0, 1.0, 0.0], [0.0; 2]); 3];
        let first = registry
            .upload_scene_mesh_global_only(&vertices, &[0, 1, 2])
            .unwrap();
        let second = registry
            .upload_scene_mesh_global_only(&vertices, &[0, 1, 2])
            .unwrap();
        let census = registry.geometry_residency_census().unwrap();
        assert_eq!(census.meshes, 2);
        assert_eq!(census.duplicate_meshes, 1);
        assert_eq!(census.source_bytes, census.duplicate_bytes * 2);
        assert_ne!(first, second);
        assert_eq!(registry.refcount(first), Some(1));
        assert_eq!(registry.refcount(second), Some(1));
    }

    #[test]
    fn census_does_not_merge_different_skin_data_or_triangle_winding() {
        let mut registry = MeshRegistry::new();
        let mut vertices = [Vertex::new([1.0; 3], [1.0; 3], [0.0, 1.0, 0.0], [0.0; 2]); 3];
        registry
            .upload_scene_mesh_global_only(&vertices, &[0, 1, 2])
            .unwrap();
        registry
            .upload_scene_mesh_global_only(&vertices, &[0, 2, 1])
            .unwrap();
        vertices[0].bone_indices[0] = 17;
        registry
            .upload_scene_mesh_global_only(&vertices, &[0, 1, 2])
            .unwrap();
        assert_eq!(
            registry
                .geometry_residency_census()
                .unwrap()
                .duplicate_meshes,
            0
        );
    }

    #[test]
    fn census_ignores_dropped_slots_and_their_stranded_pool_bytes() {
        let mut registry = MeshRegistry::new();
        let vertices = [Vertex::new([1.0; 3], [1.0; 3], [0.0, 1.0, 0.0], [0.0; 2]); 3];
        let first = registry
            .upload_scene_mesh_global_only(&vertices, &[0, 1, 2])
            .unwrap();
        registry
            .upload_scene_mesh_global_only(&vertices, &[0, 1, 2])
            .unwrap();
        registry.drop_mesh(first);
        let census = registry.geometry_residency_census().unwrap();
        assert_eq!(census.meshes, 1);
        assert_eq!(census.duplicate_meshes, 0);
    }
}
