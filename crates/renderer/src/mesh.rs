//! Mesh registry — maps MeshHandle IDs to GPU buffers.

use crate::vertex::{UiVertex, Vertex};
use crate::vulkan::allocator::SharedAllocator;
use crate::vulkan::buffer::{GpuBuffer, StagingPool};
use anyhow::Result;
use ash::vk;

/// A mesh stored on the GPU: vertex + index buffers and index count.
pub struct GpuMesh {
    pub vertex_buffer: GpuBuffer,
    pub index_buffer: GpuBuffer,
    pub index_count: u32,
    /// Offset into the global vertex SSBO (in vertices). Set after build_geometry_ssbo.
    pub global_vertex_offset: u32,
    /// Offset into the global index SSBO (in indices). Set after build_geometry_ssbo.
    pub global_index_offset: u32,
    /// Number of vertices in this mesh.
    pub vertex_count: u32,
}

impl GpuMesh {
    pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        self.vertex_buffer.destroy(device, allocator);
        self.index_buffer.destroy(device, allocator);
    }
}

/// Registry mapping mesh handle IDs to GPU-side geometry.
pub struct MeshRegistry {
    meshes: Vec<GpuMesh>,
    /// Accumulated vertex data for building the global geometry SSBO.
    /// Kept alive after `build_geometry_ssbo()` so late-loaded meshes
    /// can append and trigger a rebuild. See #258.
    pending_vertices: Vec<Vertex>,
    /// Accumulated index data for building the global geometry SSBO.
    pending_indices: Vec<u32>,
    /// Global geometry SSBO (vertices). Built by `build_geometry_ssbo()`.
    pub global_vertex_buffer: Option<GpuBuffer>,
    /// Global geometry SSBO (indices). Built by `build_geometry_ssbo()`.
    pub global_index_buffer: Option<GpuBuffer>,
    /// Set when `upload_scene_mesh` is called after the initial SSBO
    /// build — signals the frame loop to call `rebuild_geometry_ssbo`.
    geometry_dirty: bool,
    /// Number of vertices in the SSBO at last build. Used to detect
    /// whether a rebuild is needed vs. the current pending state.
    ssbo_vertex_count: usize,
    /// Old SSBOs awaiting deferred destruction. Each entry is a pair of
    /// (vertex, index) buffers and a countdown of frames before they can
    /// be safely destroyed (must survive MAX_FRAMES_IN_FLIGHT frames to
    /// guarantee no in-flight command buffer references them).
    deferred_destroy: Vec<(Option<GpuBuffer>, Option<GpuBuffer>, u32)>,
}

impl MeshRegistry {
    pub fn new() -> Self {
        Self {
            meshes: Vec::new(),
            pending_vertices: Vec::new(),
            pending_indices: Vec::new(),
            global_vertex_buffer: None,
            global_index_buffer: None,
            geometry_dirty: false,
            ssbo_vertex_count: 0,
            deferred_destroy: Vec::new(),
        }
    }

    /// Tick the deferred-destroy list. Call once per frame. Destroys old
    /// SSBOs whose countdown has reached zero (safe because all in-flight
    /// command buffers referencing them have completed).
    pub fn tick_deferred_destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        self.deferred_destroy.retain_mut(|(vb, ib, countdown)| {
            if *countdown == 0 {
                if let Some(mut b) = vb.take() {
                    b.destroy(device, allocator);
                }
                if let Some(mut b) = ib.take() {
                    b.destroy(device, allocator);
                }
                false // remove from list
            } else {
                *countdown -= 1;
                true // keep
            }
        });
    }

    /// Upload a mesh to the GPU and return its handle ID.
    ///
    /// Uses a staging buffer to place geometry in DEVICE_LOCAL memory.
    /// The vertex type is generic (`Vertex` for scene meshes, `UiVertex`
    /// for UI overlays) — the GPU buffer is format-agnostic.
    pub fn upload<V: Copy>(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        vertices: &[V],
        indices: &[u32],
        rt_enabled: bool,
        mut staging_pool: Option<&mut StagingPool>,
    ) -> Result<u32> {
        let vertex_buffer = GpuBuffer::create_vertex_buffer(
            device,
            allocator,
            queue,
            command_pool,
            vertices,
            rt_enabled,
            staging_pool.as_deref_mut(),
        )?;
        let index_buffer = GpuBuffer::create_index_buffer(
            device,
            allocator,
            queue,
            command_pool,
            indices,
            rt_enabled,
            staging_pool,
        )?;
        let index_count = indices.len() as u32;

        let id = self.meshes.len() as u32;
        self.meshes.push(GpuMesh {
            vertex_buffer,
            index_buffer,
            index_count,
            global_vertex_offset: 0,
            global_index_offset: 0,
            vertex_count: vertices.len() as u32,
        });

        Ok(id)
    }

    /// Upload a scene mesh (Vertex type) and track its geometry for the
    /// global SSBO used by RT reflection ray UV lookups.
    pub fn upload_scene_mesh(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        vertices: &[Vertex],
        indices: &[u32],
        rt_enabled: bool,
        staging_pool: Option<&mut StagingPool>,
    ) -> Result<u32> {
        // Record offsets before appending.
        let v_offset = self.pending_vertices.len() as u32;
        let i_offset = self.pending_indices.len() as u32;

        // Accumulate for global SSBO.
        self.pending_vertices.extend_from_slice(vertices);
        self.pending_indices.extend_from_slice(indices);

        // Upload to per-mesh buffers.
        let id = self.upload(device, allocator, queue, command_pool, vertices, indices, rt_enabled, staging_pool)?;

        // Store offsets.
        let mesh = &mut self.meshes[id as usize];
        mesh.global_vertex_offset = v_offset;
        mesh.global_index_offset = i_offset;

        // If the SSBO has already been built, mark dirty so the frame
        // loop knows to call rebuild_geometry_ssbo. See #258.
        if self.global_vertex_buffer.is_some()
            && self.pending_vertices.len() > self.ssbo_vertex_count
        {
            self.geometry_dirty = true;
        }

        Ok(id)
    }

    /// Build the global geometry SSBO from accumulated vertex/index data.
    /// Call once after all scene meshes are loaded.
    ///
    /// When `staging_pool` is `Some`, the staging buffer is reused from the
    /// pool instead of allocating a fresh one. This avoids a large
    /// fire-and-forget staging allocation on cell loads. See #242.
    pub fn build_geometry_ssbo(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        staging_pool: Option<&mut StagingPool>,
    ) -> Result<()> {
        if self.pending_vertices.is_empty() {
            return Ok(());
        }

        let vertex_size = (std::mem::size_of::<Vertex>() * self.pending_vertices.len()) as vk::DeviceSize;
        let index_size = (std::mem::size_of::<u32>() * self.pending_indices.len()) as vk::DeviceSize;

        // Create as STORAGE_BUFFER so the fragment shader can read vertex data
        // for RT reflection UV lookups via barycentrics.
        // Re-borrow the staging pool for two consecutive uploads.
        let mut pool = staging_pool;
        self.global_vertex_buffer = Some(GpuBuffer::create_device_local_buffer(
            device, allocator, queue, command_pool,
            vertex_size,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            &self.pending_vertices,
            pool.as_deref_mut(),
        )?);
        self.global_index_buffer = Some(GpuBuffer::create_device_local_buffer(
            device, allocator, queue, command_pool,
            index_size,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            &self.pending_indices,
            pool.as_deref_mut(),
        )?);

        log::info!(
            "Global geometry SSBO: {} vertices ({:.1} KB), {} indices ({:.1} KB)",
            self.pending_vertices.len(),
            vertex_size as f64 / 1024.0,
            self.pending_indices.len(),
            index_size as f64 / 1024.0,
        );

        // Track the built size so we can detect when new data arrives.
        // pending data is kept alive for potential rebuilds (#258).
        self.ssbo_vertex_count = self.pending_vertices.len();
        self.geometry_dirty = false;

        Ok(())
    }

    /// Rebuild the global geometry SSBO after new meshes have been loaded.
    /// Destroys the old SSBO and creates a new one from all accumulated
    /// vertex/index data. Only call when `is_geometry_dirty()` returns true.
    ///
    /// This is the simple "full rebuild" path — acceptable for infrequent
    /// cell transitions. A future streaming optimization could append
    /// in-place with buffer resize. See #258.
    pub fn rebuild_geometry_ssbo(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        staging_pool: Option<&mut StagingPool>,
    ) -> Result<()> {
        // Defer destruction of old SSBOs instead of stalling with
        // device_wait_idle. The old buffers survive for MAX_FRAMES_IN_FLIGHT
        // frames, guaranteeing no in-flight command buffer references them
        // when they're finally destroyed. The descriptor set bindings 8/9
        // will be updated to the new SSBO in the same frame this is called.
        let old_vb = self.global_vertex_buffer.take();
        let old_ib = self.global_index_buffer.take();
        if old_vb.is_some() || old_ib.is_some() {
            // Countdown of 2 frames (MAX_FRAMES_IN_FLIGHT) ensures safety.
            self.deferred_destroy.push((old_vb, old_ib, 2));
        }

        log::info!(
            "Rebuilding geometry SSBO: {} → {} vertices",
            self.ssbo_vertex_count,
            self.pending_vertices.len(),
        );

        // Rebuild from all accumulated data.
        self.build_geometry_ssbo(device, allocator, queue, command_pool, staging_pool)
    }

    /// Returns true when new meshes have been loaded since the last SSBO
    /// build. The frame loop should call `rebuild_geometry_ssbo` to update.
    pub fn is_geometry_dirty(&self) -> bool {
        self.geometry_dirty
    }

    pub fn get(&self, id: u32) -> Option<&GpuMesh> {
        self.meshes.get(id as usize)
    }

    pub fn len(&self) -> usize {
        self.meshes.len()
    }

    pub fn destroy_all(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for mesh in &mut self.meshes {
            mesh.destroy(device, allocator);
        }
        self.meshes.clear();
        if let Some(ref mut vb) = self.global_vertex_buffer {
            vb.destroy(device, allocator);
        }
        if let Some(ref mut ib) = self.global_index_buffer {
            ib.destroy(device, allocator);
        }
        self.global_vertex_buffer = None;
        self.global_index_buffer = None;
        // Drain deferred-destroy list.
        for (mut vb, mut ib, _) in self.deferred_destroy.drain(..) {
            if let Some(ref mut b) = vb {
                b.destroy(device, allocator);
            }
            if let Some(ref mut b) = ib {
                b.destroy(device, allocator);
            }
        }
    }
}

/// Colored cube geometry: 24 vertices (4 per face), 36 indices, with UVs and normals.
pub fn cube_vertices() -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        // Front face (red-ish), normal = +Z
        Vertex::new(
            [-0.5, -0.5, 0.5],
            [1.0, 0.3, 0.3],
            [0.0, 0.0, 1.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, -0.5, 0.5],
            [1.0, 0.3, 0.3],
            [0.0, 0.0, 1.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, 0.5],
            [1.0, 0.5, 0.5],
            [0.0, 0.0, 1.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, 0.5],
            [1.0, 0.5, 0.5],
            [0.0, 0.0, 1.0],
            [0.0, 0.0],
        ),
        // Back face (blue-ish), normal = -Z
        Vertex::new(
            [-0.5, -0.5, -0.5],
            [0.3, 0.3, 1.0],
            [0.0, 0.0, -1.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [0.5, -0.5, -0.5],
            [0.3, 0.3, 1.0],
            [0.0, 0.0, -1.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, -0.5],
            [0.5, 0.5, 1.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, -0.5],
            [0.5, 0.5, 1.0],
            [0.0, 0.0, -1.0],
            [1.0, 0.0],
        ),
        // Top face (green-ish), normal = +Y
        Vertex::new(
            [-0.5, 0.5, -0.5],
            [0.3, 1.0, 0.3],
            [0.0, 1.0, 0.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, -0.5],
            [0.3, 1.0, 0.3],
            [0.0, 1.0, 0.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, 0.5],
            [0.5, 1.0, 0.5],
            [0.0, 1.0, 0.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, 0.5],
            [0.5, 1.0, 0.5],
            [0.0, 1.0, 0.0],
            [0.0, 0.0],
        ),
        // Bottom face (yellow-ish), normal = -Y
        Vertex::new(
            [-0.5, -0.5, -0.5],
            [1.0, 1.0, 0.3],
            [0.0, -1.0, 0.0],
            [0.0, 0.0],
        ),
        Vertex::new(
            [0.5, -0.5, -0.5],
            [1.0, 1.0, 0.3],
            [0.0, -1.0, 0.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [0.5, -0.5, 0.5],
            [1.0, 1.0, 0.5],
            [0.0, -1.0, 0.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [-0.5, -0.5, 0.5],
            [1.0, 1.0, 0.5],
            [0.0, -1.0, 0.0],
            [0.0, 1.0],
        ),
        // Right face (cyan-ish), normal = +X
        Vertex::new(
            [0.5, -0.5, -0.5],
            [0.3, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, -0.5],
            [0.3, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        Vertex::new(
            [0.5, 0.5, 0.5],
            [0.5, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [0.5, -0.5, 0.5],
            [0.5, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0],
        ),
        // Left face (magenta-ish), normal = -X
        Vertex::new(
            [-0.5, -0.5, -0.5],
            [1.0, 0.3, 1.0],
            [-1.0, 0.0, 0.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [-0.5, 0.5, -0.5],
            [1.0, 0.3, 1.0],
            [-1.0, 0.0, 0.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, 0.5],
            [1.0, 0.5, 1.0],
            [-1.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        Vertex::new(
            [-0.5, -0.5, 0.5],
            [1.0, 0.5, 1.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0],
        ),
    ];

    let indices = vec![
        0, 1, 2, 2, 3, 0, // front
        4, 6, 5, 6, 4, 7, // back
        8, 9, 10, 10, 11, 8, // top
        12, 14, 13, 14, 12, 15, // bottom
        16, 17, 18, 18, 19, 16, // right
        20, 22, 21, 22, 20, 23, // left
    ];

    (vertices, indices)
}

/// A single colored triangle in the XY plane at Z=0, with UVs and normals.
pub fn triangle_vertices(color: [f32; 3]) -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        Vertex::new([0.0, 0.5, 0.0], color, [0.0, 0.0, 1.0], [0.5, 0.0]),
        Vertex::new([-0.5, -0.5, 0.0], color, [0.0, 0.0, 1.0], [0.0, 1.0]),
        Vertex::new([0.5, -0.5, 0.0], color, [0.0, 0.0, 1.0], [1.0, 1.0]),
    ];
    let indices = vec![0, 1, 2];
    (vertices, indices)
}

/// A textured quad in the XY plane at Z=0, with normals.
pub fn quad_vertices() -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        Vertex::new(
            [-0.5, -0.5, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, -0.5, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0],
        ),
    ];
    let indices = vec![0, 1, 2, 2, 3, 0];
    (vertices, indices)
}

/// Fullscreen quad in NDC (clip space [-1,1]), for UI overlay compositing.
/// No transforms needed — vertices pass through directly to clip space.
pub fn fullscreen_quad_vertices() -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        Vertex::new(
            [-1.0, -1.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [-1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0],
        ),
    ];
    let indices = vec![0, 1, 2, 2, 3, 0];
    (vertices, indices)
}

/// Lightweight fullscreen quad for UI overlay — position + UV only (20 B/vertex).
pub fn fullscreen_quad_ui_vertices() -> (Vec<UiVertex>, Vec<u32>) {
    let vertices = vec![
        UiVertex::new([-1.0, -1.0, 0.0], [0.0, 1.0]),
        UiVertex::new([1.0, -1.0, 0.0], [1.0, 1.0]),
        UiVertex::new([1.0, 1.0, 0.0], [1.0, 0.0]),
        UiVertex::new([-1.0, 1.0, 0.0], [0.0, 0.0]),
    ];
    let indices = vec![0, 1, 2, 2, 3, 0];
    (vertices, indices)
}
