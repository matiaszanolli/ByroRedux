//! Cooperative spawning for CSG-decoded static precombines.

use super::super::FrameTimeBudget;
use super::*;
use std::sync::Arc;

/// Keeps the original placement root and submesh numbering across yields.
/// Only `geometry_only_cached` entries enter here; ordinary NIFs retain the
/// full placement path for lights, particles and authored collision.
pub(in crate::cell_loader) struct PrecombinedPlacement {
    pub cached: Arc<CachedNifImport>,
    root: byroredux_core::ecs::EntityId,
    paths: Vec<mesh_instance::ResolvedMeshPaths>,
    next_mesh: usize,
    pub count: usize,
    pub timings: PlacementSpawnTimings,
    pub elapsed: Duration,
    pub groups: usize,
    pub max_group: Duration,
}

impl PrecombinedPlacement {
    pub fn new(
        world: &mut World,
        cached: Arc<CachedNifImport>,
        tex_provider: &TextureProvider,
        origin: Vec3,
        mat_provider: Option<&mut MaterialProvider>,
    ) -> Self {
        let started = Instant::now();
        let (root, _) = spawn_placement_root(
            world,
            &cached,
            origin,
            Quat::IDENTITY,
            1.0,
            None,
            None,
            None,
        );
        let paths = resolve_mesh_paths_with_pre_merge(
            world,
            &cached.meshes,
            &cached.pre_merge_materials,
            None,
            mat_provider,
            Some(tex_provider),
        );
        let elapsed = started.elapsed();
        Self {
            cached,
            root,
            paths,
            next_mesh: 0,
            count: 0,
            timings: PlacementSpawnTimings {
                cpu_upload: elapsed,
                ..Default::default()
            },
            elapsed,
            groups: 0,
            max_group: Duration::ZERO,
        }
    }

    pub fn advance(
        &mut self,
        world: &mut World,
        ctx: &mut VulkanContext,
        tex_provider: &TextureProvider,
        origin: Vec3,
        path: &str,
        budget: &mut FrameTimeBudget,
    ) -> bool {
        let pc = PlacementCtx {
            tex_provider,
            geometry_dedup: &self.cached.geometry_dedup,
            ref_pos: origin,
            ref_rot: Quat::IDENTITY,
            ref_scale: 1.0,
            base_layer: byroredux_core::ecs::RenderLayer::Architecture,
            mesh_cache_key: Some(path),
            refr_overlay: None,
            light_data: None,
            light_animation_flags: 0,
            light_shadow_flags: 0,
            light_kind: byroredux_core::ecs::LightKind::Point,
            light_direction: [0.0; 3],
            light_outer_angle: 0.0,
            light_falloff_exponent: 1.0,
            placement_root: self.root,
            collision_fallback: MissingCollisionFallback::ArchitectureTriMesh,
            spawned_nif_lights: 0,
        };
        while self.next_mesh < self.cached.meshes.len() {
            if budget.should_yield() {
                return false;
            }
            // Keep synchronous bootstrap's one upload/BLAS batch per hash.
            // Streaming batches cap both submesh count and estimated transfer
            // bytes; one oversized mesh still guarantees progress.
            let mut end = self.cached.meshes.len();
            if budget.is_limited() {
                end = self.next_mesh;
                let mut bytes = 0usize;
                while end < self.cached.meshes.len() && end - self.next_mesh < 8 {
                    let mesh = &self.cached.meshes[end];
                    let size = mesh
                        .positions
                        .len()
                        .saturating_mul(std::mem::size_of::<byroredux_renderer::Vertex>())
                        .saturating_add(mesh.indices.len().saturating_mul(4));
                    if end > self.next_mesh && bytes.saturating_add(size) > 4 * 1024 * 1024 {
                        break;
                    }
                    bytes = bytes.saturating_add(size);
                    end += 1;
                }
            }
            let started = Instant::now();
            let prepared = mesh_instance::prepare_mesh_upload_range(
                ctx,
                &pc,
                &self.cached,
                &self.paths,
                self.next_mesh..end,
            );
            let mut specs = Vec::new();
            let mut synthesized = false;
            for (index, prepared) in prepared
                .into_iter()
                .enumerate()
                .take(end)
                .skip(self.next_mesh)
            {
                if spawn_mesh_instance(
                    world,
                    ctx,
                    &pc,
                    &self.cached,
                    &self.cached.meshes[index],
                    &self.paths[index],
                    self.count,
                    prepared,
                    &mut specs,
                    &mut synthesized,
                ) {
                    self.count += 1;
                }
            }
            let blas_started = Instant::now();
            self.timings.blas_requested += specs.len() as u32;
            if !specs.is_empty() {
                self.timings.blas_built += ctx.build_blas_batched(&specs) as u32;
            }
            let blas = blas_started.elapsed();
            let elapsed = started.elapsed();
            self.timings.blas += blas;
            self.timings.cpu_upload += elapsed.saturating_sub(blas);
            self.elapsed += elapsed;
            self.groups += 1;
            self.max_group = self.max_group.max(elapsed);
            self.next_mesh = end;
            budget.complete_unit();
        }
        true
    }
}
