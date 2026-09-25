//! Exact-content sharing for immutable scene geometry. Source paths/materials
//! are not geometry identity; all vertex attributes, indices, and RT buffer
//! eligibility are. Deformation data stored outside the vertex buffer must
//! keep its own identity: the loader does not register skin/morph meshes here.

use super::{MeshRegistry, SceneMeshUpload, index_slice_bytes, mesh_cache_key, vertex_slice_bytes};
use std::hash::{DefaultHasher, Hash, Hasher};

impl SceneMeshUpload<'_> {
    /// Lookup accelerator only. Always confirm with `same_geometry` before
    /// sharing; a hash collision is not permission to substitute a mesh.
    pub fn geometry_fingerprint(&self) -> u64 {
        let mut hash = DefaultHasher::new();
        self.rt_enabled.hash(&mut hash);
        vertex_slice_bytes(self.vertices).hash(&mut hash);
        index_slice_bytes(self.indices).hash(&mut hash);
        hash.finish()
    }

    pub fn same_geometry(&self, other: &Self) -> bool {
        self.rt_enabled == other.rt_enabled
            && vertex_slice_bytes(self.vertices) == vertex_slice_bytes(other.vertices)
            && self.indices == other.indices
    }
}

impl MeshRegistry {
    /// Opt in a successfully uploaded immutable mesh. Skins/morphs carry
    /// additional identity outside these buffers and must not opt in.
    pub fn register_scene_geometry_for_sharing(&mut self, handle: u32) {
        self.register_scene_geometry_for_sharing_with_fingerprint(handle, None);
    }

    /// Register geometry when the caller already computed its fingerprint
    /// from the sanitized upload payload.
    pub fn register_scene_geometry_for_sharing_with_fingerprint(
        &mut self,
        handle: u32,
        fingerprint: Option<u64>,
    ) {
        if self.deferred_compaction.is_some() {
            return;
        }
        let Some(mesh) = self.get(handle).filter(|m| m.is_scene_mesh) else {
            return;
        };
        if mesh.vertex_buffer.is_some() != mesh.index_buffer.is_some() {
            return;
        }
        let dedicated = mesh.vertex_buffer.is_some();
        let vs = mesh.global_vertex_offset as usize;
        let is = mesh.global_index_offset as usize;
        let upload = SceneMeshUpload {
            vertices: &self.pending_vertices[vs..vs + mesh.vertex_count as usize],
            indices: &self.pending_indices[is..is + mesh.index_count as usize],
            rt_enabled: mesh.rt_capable,
            cache_key: None,
        };
        let fingerprint = fingerprint.unwrap_or_else(|| upload.geometry_fingerprint());
        let bucket = self.geometry_cache.entry((fingerprint, dedicated)).or_default();
        if !bucket.contains(&handle) {
            bucket.push(handle);
        }
    }

    /// Acquire a byte-identical, dedicated-buffer mesh and cache the new model
    /// path as another alias. A hit owns one refcount, just like a path hit.
    /// Safe after admission closes: no buffers or source ranges are appended.
    pub fn acquire_matching_scene_mesh(&mut self, upload: &SceneMeshUpload<'_>) -> Option<u32> {
        self.acquire_matching_scene_mesh_with_fingerprint(upload).0
    }

    /// Like `acquire_matching_scene_mesh`, and also returns the fingerprint
    /// used for lookup so fresh uploads can reuse it when entering the cache.
    pub fn acquire_matching_scene_mesh_with_fingerprint(
        &mut self,
        upload: &SceneMeshUpload<'_>,
    ) -> (Option<u32>, u64) {
        self.acquire_matching_geometry_with_fingerprint(upload, true)
    }

    #[cfg(test)]
    fn acquire_matching_geometry(
        &mut self,
        upload: &SceneMeshUpload<'_>,
        dedicated: bool,
    ) -> Option<u32> {
        self.acquire_matching_geometry_with_fingerprint(upload, dedicated).0
    }

    fn acquire_matching_geometry_with_fingerprint(
        &mut self,
        upload: &SceneMeshUpload<'_>,
        dedicated: bool,
    ) -> (Option<u32>, u64) {
        let indices = Self::sanitize_scene_indices(upload.vertices.len(), upload.indices);
        let sanitized = SceneMeshUpload {
            indices: &indices,
            ..*upload
        };
        let fingerprint = sanitized.geometry_fingerprint();
        // CPU pools already use the new layout while old mesh offsets remain
        // published during chunked compaction. Do not read the wrong ranges.
        // Still return the sanitized fingerprint used by a subsequent upload.
        if self.deferred_compaction.is_some() {
            return (None, fingerprint);
        }
        let Some(candidates) = self.geometry_cache.get(&(fingerprint, dedicated)) else {
            return (None, fingerprint);
        };
        let handle = candidates.iter().copied().find(|&handle| {
            let Some(mesh) = self.get(handle) else {
                return false;
            };
            if self.refcount(handle) == Some(0) {
                return false;
            }
            let vs = mesh.global_vertex_offset as usize;
            let is = mesh.global_index_offset as usize;
            let Some(vertices) = self
                .pending_vertices
                .get(vs..vs + mesh.vertex_count as usize)
            else {
                return false;
            };
            let Some(indices) = self.pending_indices.get(is..is + mesh.index_count as usize) else {
                return false;
            };
            sanitized.same_geometry(&SceneMeshUpload {
                vertices,
                indices,
                rt_enabled: mesh.rt_capable,
                cache_key: None,
            })
        });
        let acquired = handle.and_then(|handle| self.acquire_mesh_alias(handle, upload.cache_key));
        (acquired, fingerprint)
    }

    /// Acquire an already-validated representative for a same-batch duplicate.
    /// Every alias has its own lifetime; dropping one never frees its siblings.
    pub fn acquire_mesh_alias(&mut self, handle: u32, key: Option<(&str, u32)>) -> Option<u32> {
        self.get(handle)?;
        let refs = self.mesh_ref_counts.get_mut(handle as usize)?;
        if *refs == 0 {
            return None;
        }
        *refs = refs.checked_add(1)?;
        if let Some((path, sub)) = key {
            self.mesh_cache.insert(mesh_cache_key(path, sub), handle);
        }
        Some(handle)
    }

    pub(super) fn prune_shared_geometry(&mut self, mut freed: impl FnMut(u32) -> bool) {
        self.geometry_cache.retain(|_, bucket| {
            bucket.retain(|&handle| !freed(handle));
            !bucket.is_empty()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertex::Vertex;

    fn triangle() -> [Vertex; 3] {
        [Vertex::new([1.0; 3], [1.0; 3], [0.0, 1.0, 0.0], [0.0; 2]); 3]
    }

    #[test]
    fn different_paths_share_exact_bytes_and_release_only_the_last_holder() {
        let mut registry = MeshRegistry::new();
        let vertices = triangle();
        let handle = registry
            .upload_scene_mesh_global_only(&vertices, &[0, 1, 2])
            .unwrap();
        registry.register_scene_geometry_for_sharing(handle);
        let upload = SceneMeshUpload {
            vertices: &vertices,
            indices: &[0, 1, 2],
            rt_enabled: false,
            cache_key: Some(("second.nif", 7)),
        };
        // Exercise the same cache with device-free global-only storage. The
        // production dedicated-buffer lookup must not substitute that class.
        assert_eq!(registry.acquire_matching_scene_mesh(&upload), None);
        assert_eq!(
            registry.acquire_matching_geometry(&upload, false),
            Some(handle)
        );
        assert_eq!(registry.acquire_cached("second.nif", 7), Some(handle));
        assert_eq!(registry.pending_vertices.len(), 3);
        assert!(!registry.drop_mesh(handle));
        assert!(!registry.drop_mesh(handle));
        assert!(registry.drop_mesh(handle));
        assert!(registry.geometry_cache.is_empty());
        assert_eq!(registry.acquire_cached("second.nif", 7), None);
        assert_eq!(registry.acquire_matching_geometry(&upload, false), None);
    }

    #[test]
    fn exact_content_hits_work_after_admission_closes_and_across_compaction() {
        let mut registry = MeshRegistry::new();
        let vertices = triangle();
        let dead = registry
            .upload_scene_mesh_global_only(&vertices, &[0, 2, 1])
            .unwrap();
        let handle = registry
            .upload_scene_mesh_global_only(&vertices, &[0, 1, 2])
            .unwrap();
        registry.register_scene_geometry_for_sharing(handle);
        registry.drop_mesh(dead);
        registry.compact_pending_geometry();
        registry.scene_geometry_admission_closed = true;
        let upload = SceneMeshUpload {
            vertices: &vertices,
            indices: &[0, 1, 2],
            rt_enabled: false,
            cache_key: None,
        };
        assert_eq!(
            registry.acquire_matching_geometry(&upload, false),
            Some(handle)
        );
        assert_eq!(registry.pending_vertices.len(), 3);
    }

    #[test]
    fn hash_collisions_do_not_share_different_vertex_attributes_or_rt_eligibility() {
        let mut registry = MeshRegistry::new();
        let vertices = triangle();
        let handle = registry
            .upload_scene_mesh_global_only(&vertices, &[0, 1, 2])
            .unwrap();
        registry.register_scene_geometry_for_sharing(handle);
        let mut changed = vertices;
        changed[0].uv[0] = 0.25;
        let mut upload = SceneMeshUpload {
            vertices: &changed,
            indices: &[0, 1, 2],
            rt_enabled: false,
            cache_key: None,
        };
        // Force the wrong representative into the lookup bucket.
        registry
            .geometry_cache
            .insert((upload.geometry_fingerprint(), false), vec![handle]);
        assert_eq!(registry.acquire_matching_geometry(&upload, false), None);
        upload.vertices = &vertices;
        upload.rt_enabled = true;
        registry
            .geometry_cache
            .insert((upload.geometry_fingerprint(), false), vec![handle]);
        assert_eq!(registry.acquire_matching_geometry(&upload, false), None);
        assert_eq!(registry.refcount(handle), Some(1));
    }

    #[test]
    fn lookup_fingerprint_can_register_fresh_geometry_after_index_sanitization() {
        let mut registry = MeshRegistry::new();
        let vertices = triangle();
        let upload = SceneMeshUpload {
            vertices: &vertices,
            indices: &[0, 1, 99],
            rt_enabled: false,
            cache_key: None,
        };
        let (missing, fingerprint) = registry.acquire_matching_geometry_with_fingerprint(&upload, false);
        assert_eq!(missing, None);
        // GPU uploads clamp indices to the last vertex. Registration must use
        // that identity, including when lookup returned no representative.
        let handle = registry.upload_scene_mesh_global_only(&vertices, upload.indices).unwrap();
        registry.register_scene_geometry_for_sharing_with_fingerprint(handle, Some(fingerprint));
        let sanitized = SceneMeshUpload { indices: &[0, 1, 2], ..upload };
        let (hit, reused_fingerprint) = registry.acquire_matching_geometry_with_fingerprint(&sanitized, false);
        assert_eq!(hit, Some(handle));
        assert_eq!(reused_fingerprint, fingerprint);
        assert_eq!(registry.acquire_matching_geometry(&upload, false), Some(handle));
    }
}
