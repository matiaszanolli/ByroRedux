//! Opt-in, CPU-only census of admitted source geometry. This does not change
//! admission, mesh identity, buffers, or BLAS. Hash collisions are checked
//! against complete vertex/index bytes before counting a duplicate.
//!
//! Beyond the aggregate totals, the census attributes duplication: every
//! mesh is classified by upload provenance ([`MeshUploadSource`], annotated
//! by the uploading path), content-derived skinning (`bone_weights` nonzero
//! anywhere in the vertex span — the ground truth an exact-content share
//! would have to match), morph provenance (stated by the uploader; morph
//! deltas live outside the vertex bytes), and storage class (dedicated
//! per-mesh buffers vs global-SSBO-only). Duplicate groups are reported
//! largest-first with member labels so a run names the assets responsible.

use super::{MeshRegistry, MeshUploadSource, index_slice_bytes, vertex_slice_bytes};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Per-mesh classification cell: (source, skinned, morphs, dedicated).
type CategoryKey = (MeshUploadSource, bool, bool, bool);

#[derive(Debug, Default, PartialEq, Eq)]
struct ResidencyCensus {
    meshes: usize,
    source_bytes: usize,
    duplicate_meshes: usize,
    duplicate_bytes: usize,
    dedicated_vertex_bytes: u64,
    dedicated_index_bytes: u64,
    /// Per-category totals, sorted by `duplicate_bytes` descending.
    categories: Vec<CensusCategory>,
    /// Largest duplicate groups by wasted bytes, capped.
    top_groups: Vec<CensusGroup>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CensusCategory {
    source: MeshUploadSource,
    skinned: bool,
    morphs: bool,
    dedicated: bool,
    meshes: usize,
    duplicates: usize,
    duplicate_bytes: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CensusGroup {
    copies: usize,
    per_copy_bytes: usize,
    wasted_bytes: usize,
    vertex_count: u32,
    rt: bool,
    /// Classification of the group's first (representative) member; content
    /// equality already guarantees skinned/dedicated/rt match across members,
    /// only provenance can differ.
    key: CategoryKey,
    /// Distinct member labels with counts, most frequent first, capped.
    labels: Vec<(String, usize)>,
}

/// A verified member of an exact-content bucket.
struct Member<'a> {
    bytes: usize,
    vertex_count: u32,
    rt: bool,
    vertices: &'a [u8],
    indices: &'a [u8],
    key: CategoryKey,
    label: Option<&'a str>,
}

/// Cap on reported top groups / distinct labels per group — enough to name
/// the dominant assets without turning one scene's census into a dump.
const TOP_GROUPS_REPORTED: usize = 12;
const LABELS_PER_GROUP: usize = 5;

impl MeshRegistry {
    fn geometry_residency_census(&self) -> Option<ResidencyCensus> {
        // During a deferred compaction offsets still describe the old GPU
        // generation, not the compacted CPU pools. Never compare mixed layouts.
        if self.deferred_compaction.is_some() {
            return None;
        }
        let mut census = ResidencyCensus::default();
        let mut buckets: HashMap<u64, Vec<Member<'_>>> = HashMap::new();
        for (handle, mesh) in self
            .meshes
            .iter()
            .enumerate()
            .filter_map(|(i, m)| m.as_ref().map(|m| (i as u32, m)))
            .filter(|(_, m)| m.is_scene_mesh)
        {
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
            // Content-derived skinning: rigid vertices carry all-zero
            // weights by contract (the shader's rigid-path tag), skinned
            // ones bind at least one nonzero weight — including the
            // bone-0 fallback tails (#2467). This reads what a share would
            // actually have to byte-match, independent of annotation.
            let skinned = self.pending_vertices[vs..vs + mesh.vertex_count as usize]
                .iter()
                .any(|v| v.bone_weights != [0.0; 4]);
            let provenance = self.mesh_provenance.get(&handle);
            let key = (
                provenance.map_or(MeshUploadSource::Other, |p| p.source),
                skinned,
                provenance.is_some_and(|p| p.morphs),
                mesh.vertex_buffer.is_some(),
            );
            let member = Member {
                bytes,
                vertex_count: mesh.vertex_count,
                rt: mesh.rt_capable,
                vertices,
                indices,
                key,
                label: provenance.and_then(|p| p.label.as_deref()),
            };
            let mut hash = DefaultHasher::new();
            mesh.rt_capable.hash(&mut hash);
            vertices.hash(&mut hash);
            indices.hash(&mut hash);
            let bucket = buckets.entry(hash.finish()).or_default();
            // Every verified copy stays in the bucket — category totals
            // rank members (rank > 0 = duplicate) and the largest-group
            // report needs the full member list for labels and counts.
            if bucket
                .iter()
                .any(|m| m.rt == member.rt && m.vertices == vertices && m.indices == indices)
            {
                census.duplicate_meshes += 1;
                census.duplicate_bytes += bytes;
            }
            bucket.push(member);
        }
        census.categories = Self::category_totals(&buckets);
        census.top_groups = Self::largest_groups(&buckets);
        Some(census)
    }

    /// Per-member accounting: every mesh lands in its own category's
    /// `meshes`, every *duplicate* member (all but the first of its
    /// content bucket) lands in its own category's duplicate totals — the
    /// uploads that exact-content sharing could still reclaim.
    fn category_totals(buckets: &HashMap<u64, Vec<Member<'_>>>) -> Vec<CensusCategory> {
        let mut totals: HashMap<CategoryKey, CensusCategory> = HashMap::new();
        for bucket in buckets.values() {
            for (rank, member) in bucket.iter().enumerate() {
                let entry = totals.entry(member.key).or_insert_with(|| CensusCategory {
                    source: member.key.0,
                    skinned: member.key.1,
                    morphs: member.key.2,
                    dedicated: member.key.3,
                    meshes: 0,
                    duplicates: 0,
                    duplicate_bytes: 0,
                });
                entry.meshes += 1;
                if rank > 0 {
                    entry.duplicates += 1;
                    entry.duplicate_bytes += member.bytes;
                }
            }
        }
        let mut rows: Vec<CensusCategory> = totals.into_values().collect();
        rows.sort_by(|a, b| {
            b.duplicate_bytes
                .cmp(&a.duplicate_bytes)
                .then_with(|| b.meshes.cmp(&a.meshes))
        });
        rows
    }

    fn largest_groups(buckets: &HashMap<u64, Vec<Member<'_>>>) -> Vec<CensusGroup> {
        let mut groups: Vec<CensusGroup> = buckets
            .values()
            .filter(|bucket| bucket.len() > 1)
            .map(|bucket| {
                let representative = &bucket[0];
                let mut labels: HashMap<&str, usize> = HashMap::new();
                for member in bucket {
                    if let Some(label) = member.label {
                        *labels.entry(label).or_default() += 1;
                    }
                }
                let mut labels: Vec<(String, usize)> = labels
                    .into_iter()
                    .map(|(label, count)| (label.to_owned(), count))
                    .collect();
                labels.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                labels.truncate(LABELS_PER_GROUP);
                CensusGroup {
                    copies: bucket.len(),
                    per_copy_bytes: representative.bytes,
                    wasted_bytes: representative.bytes * (bucket.len() - 1),
                    vertex_count: representative.vertex_count,
                    rt: representative.rt,
                    key: representative.key,
                    labels,
                }
            })
            .collect();
        groups.sort_by(|a, b| {
            b.wasted_bytes
                .cmp(&a.wasted_bytes)
                .then_with(|| b.copies.cmp(&a.copies))
        });
        groups.truncate(TOP_GROUPS_REPORTED);
        groups
    }

    pub(super) fn log_geometry_residency_if_requested(&self) {
        if std::env::var("BYROREDUX_GEOMETRY_CENSUS").as_deref() != Ok("1") {
            return;
        }
        match self.geometry_residency_census() {
            Some(c) => {
                log::warn!(
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
                );
                for category in &c.categories {
                    log::warn!(
                        "geometry-census-category: source={} skinned={} morphs={} dedicated={} \
                         meshes={} duplicates={} dup_bytes={}",
                        category.source.as_str(),
                        category.skinned as u8,
                        category.morphs as u8,
                        category.dedicated as u8,
                        category.meshes,
                        category.duplicates,
                        category.duplicate_bytes,
                    );
                }
                for group in &c.top_groups {
                    let labels = if group.labels.is_empty() {
                        "-".to_owned()
                    } else {
                        group
                            .labels
                            .iter()
                            .map(|(label, count)| format!("{label}(x{count})"))
                            .collect::<Vec<_>>()
                            .join(",")
                    };
                    log::warn!(
                        "geometry-census-top: copies={} per_copy_bytes={} wasted_bytes={} \
                         verts={} rt={} source={} skinned={} morphs={} dedicated={} labels={}",
                        group.copies,
                        group.per_copy_bytes,
                        group.wasted_bytes,
                        group.vertex_count,
                        group.rt as u8,
                        group.key.0.as_str(),
                        group.key.1 as u8,
                        group.key.2 as u8,
                        group.key.3 as u8,
                        labels,
                    );
                }
            }
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

    fn skinned_triangle() -> [Vertex; 3] {
        let mut vertices = [Vertex::new([1.0; 3], [1.0; 3], [0.0, 1.0, 0.0], [0.0; 2]); 3];
        vertices[0].bone_weights = [1.0, 0.0, 0.0, 0.0];
        vertices[0].bone_indices = [3, 0, 0, 0];
        vertices
    }

    #[test]
    fn census_attributes_duplicates_to_upload_source_and_skin_class() {
        let mut registry = MeshRegistry::new();
        let rigid = [Vertex::new([1.0; 3], [1.0; 3], [0.0, 1.0, 0.0], [0.0; 2]); 3];
        let skinned = skinned_triangle();

        let rigid_a = registry
            .upload_scene_mesh_global_only(&rigid, &[0, 1, 2])
            .unwrap();
        let rigid_b = registry
            .upload_scene_mesh_global_only(&rigid, &[0, 1, 2])
            .unwrap();
        registry.note_mesh_provenance(
            rigid_a,
            MeshUploadSource::CellLoader,
            false,
            Some("props\\chair.nif#0"),
        );
        registry.note_mesh_provenance(rigid_b, MeshUploadSource::CellLoader, false, None);

        let skin_a = registry
            .upload_scene_mesh_global_only(&skinned, &[0, 1, 2])
            .unwrap();
        let skin_b = registry
            .upload_scene_mesh_global_only(&skinned, &[0, 1, 2])
            .unwrap();
        let skin_c = registry
            .upload_scene_mesh_global_only(&skinned, &[0, 1, 2])
            .unwrap();
        registry.note_mesh_provenance(
            skin_a,
            MeshUploadSource::NifLoader,
            false,
            Some("Naked_M:0"),
        );
        // morphs=true lands skin_b in its own category even though its
        // bytes match — morph provenance is uploader-stated, and its
        // duplicate is attributed there.
        registry.note_mesh_provenance(skin_b, MeshUploadSource::NifLoader, true, None);
        // Third copy unannotated: classified Other by provenance but
        // skinned by content.
        let _ = skin_c;

        let census = registry.geometry_residency_census().unwrap();
        assert_eq!(census.meshes, 5);
        assert_eq!(census.duplicate_meshes, 3);

        let rigid_row = census
            .categories
            .iter()
            .find(|c| c.source == MeshUploadSource::CellLoader)
            .expect("cell-loader category present");
        assert!(!rigid_row.skinned);
        assert_eq!(rigid_row.meshes, 2);
        assert_eq!(rigid_row.duplicates, 1);

        let morph_row = census
            .categories
            .iter()
            .find(|c| c.source == MeshUploadSource::NifLoader && c.morphs)
            .expect("morph-bearing skinned category");
        assert!(morph_row.skinned);
        assert_eq!(morph_row.meshes, 1);
        assert_eq!(morph_row.duplicates, 1);

        let unannotated = census
            .categories
            .iter()
            .find(|c| c.source == MeshUploadSource::Other && c.skinned)
            .expect("unannotated skinned copy classified Other");
        assert_eq!(unannotated.meshes, 1);
        assert_eq!(unannotated.duplicates, 1);

        // Skinned group (3 copies) outranks the rigid pair by wasted bytes.
        let top = &census.top_groups[0];
        assert_eq!(top.copies, 3);
        assert_eq!(top.wasted_bytes, top.per_copy_bytes * 2);
        assert!(top.key.1, "content-derived skinned flag on the group");
        let labels: Vec<_> = top.labels.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(labels, ["Naked_M:0"]);
    }

    #[test]
    fn provenance_on_dead_handles_is_ignored() {
        let mut registry = MeshRegistry::new();
        let vertices = [Vertex::new([1.0; 3], [1.0; 3], [0.0, 1.0, 0.0], [0.0; 2]); 3];
        let handle = registry
            .upload_scene_mesh_global_only(&vertices, &[0, 1, 2])
            .unwrap();
        registry.drop_mesh(handle);
        registry.note_mesh_provenance(handle, MeshUploadSource::NifLoader, false, Some("dead"));
        registry.note_mesh_provenance(9000, MeshUploadSource::NifLoader, false, Some("ghost"));
        assert!(registry.mesh_provenance.is_empty());
    }

    /// #4802 — provenance is census-only and the census reads live handles
    /// only, so the entry must leave with the last holder. It used to stay
    /// forever: every mesh ever uploaded kept an owned label in host RAM.
    #[test]
    fn provenance_is_pruned_when_the_last_holder_releases() {
        let mut registry = MeshRegistry::new();
        let vertices = [Vertex::new([1.0; 3], [1.0; 3], [0.0, 1.0, 0.0], [0.0; 2]); 3];
        let shared = registry
            .upload_scene_mesh_global_only(&vertices, &[0, 1, 2])
            .unwrap();
        let solo = registry
            .upload_scene_mesh_global_only(&vertices, &[0, 1, 2])
            .unwrap();
        registry.note_mesh_provenance(
            shared,
            MeshUploadSource::CellLoader,
            false,
            Some("props\\chair.nif#0"),
        );
        registry.note_mesh_provenance(solo, MeshUploadSource::CellLoader, false, None);
        // A second placement holds `shared`, the way `acquire_cached` would.
        registry.mesh_ref_counts[shared as usize] = 2;

        registry.drop_mesh(shared);
        assert!(
            registry.mesh_provenance.contains_key(&shared),
            "one holder is still live — its provenance must survive a partial release"
        );

        registry.drop_mesh(shared);
        registry.drop_meshes(&[solo]);
        assert!(
            registry.mesh_provenance.is_empty(),
            "provenance for freed handles leaked: {:?}",
            registry.mesh_provenance.keys().collect::<Vec<_>>()
        );
    }
}
