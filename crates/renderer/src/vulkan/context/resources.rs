//! VulkanContext resource management methods (BLAS, UI quad, extent, memory).

use super::VulkanContext;
use anyhow::Result;

impl VulkanContext {
    /// Build a BLAS for a mesh (RT only). Call after uploading a mesh.
    pub fn build_blas_for_mesh(&mut self, mesh_handle: u32, vertex_count: u32, index_count: u32) {
        let Some(ref mut accel) = self.accel_manager else {
            return;
        };
        let Some(mesh) = self.mesh_registry.get(mesh_handle) else {
            return;
        };
        let allocator = self.allocator.as_ref().expect("allocator missing");
        if let Err(e) = accel.build_blas(
            &self.device,
            allocator,
            &self.graphics_queue,
            self.transfer_pool,
            mesh_handle,
            mesh,
            vertex_count,
            index_count,
        ) {
            log::warn!("BLAS build failed for mesh {}: {e}", mesh_handle);
        }
    }

    /// Build BLAS for multiple meshes in a single GPU submission.
    ///
    /// Call this after uploading all meshes during scene/cell load to
    /// avoid the per-mesh fence stall of `build_blas_for_mesh`. Returns
    /// the number of BLAS successfully built.
    pub fn build_blas_batched(&mut self, mesh_specs: &[(u32, u32, u32)]) -> usize {
        let Some(ref mut accel) = self.accel_manager else {
            return 0;
        };
        let allocator = self.allocator.as_ref().expect("allocator missing");

        // Gather mesh references for the batch — skip any missing handles.
        let meshes: Vec<(u32, &crate::mesh::GpuMesh, u32, u32)> = mesh_specs
            .iter()
            .filter_map(|&(handle, vc, ic)| {
                self.mesh_registry.get(handle).map(|m| (handle, m, vc, ic))
            })
            .collect();

        match accel.build_blas_batched(
            &self.device,
            allocator,
            &self.graphics_queue,
            self.transfer_pool,
            &meshes,
        ) {
            Ok(count) => count,
            Err(e) => {
                log::warn!("Batched BLAS build failed: {e}");
                0
            }
        }
    }

    /// Register the fullscreen quad mesh for UI overlay rendering.
    /// Call this once after creating the context.
    pub fn register_ui_quad(&mut self) -> Result<()> {
        let (vertices, indices) = crate::mesh::fullscreen_quad_ui_vertices();
        let allocator = self.allocator.as_ref().expect("allocator missing");
        let handle = self.mesh_registry.upload(
            &self.device,
            allocator,
            &self.graphics_queue,
            self.transfer_pool,
            &vertices,
            &indices,
            false, // UI quad doesn't need RT
            None,
        )?;
        self.ui_quad_handle = Some(handle);
        log::info!("UI fullscreen quad registered (mesh handle {})", handle);
        Ok(())
    }

    /// Get the current swapchain extent (viewport dimensions).
    pub fn swapchain_extent(&self) -> (u32, u32) {
        (
            self.swapchain_state.extent.width,
            self.swapchain_state.extent.height,
        )
    }

    /// Log current GPU memory allocation statistics.
    pub fn log_memory_usage(&self) {
        if let Some(ref alloc) = self.allocator {
            super::super::allocator::log_memory_usage(alloc);
        }
    }
}
