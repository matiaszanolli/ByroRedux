//! Mesh registry — maps MeshHandle IDs to GPU buffers.

use crate::vertex::Vertex;
use crate::vulkan::allocator::SharedAllocator;
use crate::vulkan::buffer::GpuBuffer;
use anyhow::Result;
use ash::vk;

/// A mesh stored on the GPU: vertex + index buffers and index count.
pub struct GpuMesh {
    pub vertex_buffer: GpuBuffer,
    pub index_buffer: GpuBuffer,
    pub index_count: u32,
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
}

impl MeshRegistry {
    pub fn new() -> Self {
        Self {
            meshes: Vec::new(),
        }
    }

    /// Upload a mesh to the GPU and return its handle ID.
    ///
    /// Uses a staging buffer to place geometry in DEVICE_LOCAL memory.
    pub fn upload(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        vertices: &[Vertex],
        indices: &[u32],
        rt_enabled: bool,
    ) -> Result<u32> {
        let vertex_buffer =
            GpuBuffer::create_vertex_buffer(device, allocator, queue, command_pool, vertices, rt_enabled)?;
        let index_buffer =
            GpuBuffer::create_index_buffer(device, allocator, queue, command_pool, indices, rt_enabled)?;
        let index_count = indices.len() as u32;

        let id = self.meshes.len() as u32;
        self.meshes.push(GpuMesh {
            vertex_buffer,
            index_buffer,
            index_count,
        });

        Ok(id)
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
    }
}

/// Colored cube geometry: 24 vertices (4 per face), 36 indices, with UVs and normals.
pub fn cube_vertices() -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        // Front face (red-ish), normal = +Z
        Vertex::new([-0.5, -0.5,  0.5], [1.0, 0.3, 0.3], [0.0, 0.0, 1.0], [0.0, 1.0]),
        Vertex::new([ 0.5, -0.5,  0.5], [1.0, 0.3, 0.3], [0.0, 0.0, 1.0], [1.0, 1.0]),
        Vertex::new([ 0.5,  0.5,  0.5], [1.0, 0.5, 0.5], [0.0, 0.0, 1.0], [1.0, 0.0]),
        Vertex::new([-0.5,  0.5,  0.5], [1.0, 0.5, 0.5], [0.0, 0.0, 1.0], [0.0, 0.0]),
        // Back face (blue-ish), normal = -Z
        Vertex::new([-0.5, -0.5, -0.5], [0.3, 0.3, 1.0], [0.0, 0.0, -1.0], [1.0, 1.0]),
        Vertex::new([ 0.5, -0.5, -0.5], [0.3, 0.3, 1.0], [0.0, 0.0, -1.0], [0.0, 1.0]),
        Vertex::new([ 0.5,  0.5, -0.5], [0.5, 0.5, 1.0], [0.0, 0.0, -1.0], [0.0, 0.0]),
        Vertex::new([-0.5,  0.5, -0.5], [0.5, 0.5, 1.0], [0.0, 0.0, -1.0], [1.0, 0.0]),
        // Top face (green-ish), normal = +Y
        Vertex::new([-0.5,  0.5, -0.5], [0.3, 1.0, 0.3], [0.0, 1.0, 0.0], [0.0, 1.0]),
        Vertex::new([ 0.5,  0.5, -0.5], [0.3, 1.0, 0.3], [0.0, 1.0, 0.0], [1.0, 1.0]),
        Vertex::new([ 0.5,  0.5,  0.5], [0.5, 1.0, 0.5], [0.0, 1.0, 0.0], [1.0, 0.0]),
        Vertex::new([-0.5,  0.5,  0.5], [0.5, 1.0, 0.5], [0.0, 1.0, 0.0], [0.0, 0.0]),
        // Bottom face (yellow-ish), normal = -Y
        Vertex::new([-0.5, -0.5, -0.5], [1.0, 1.0, 0.3], [0.0, -1.0, 0.0], [0.0, 0.0]),
        Vertex::new([ 0.5, -0.5, -0.5], [1.0, 1.0, 0.3], [0.0, -1.0, 0.0], [1.0, 0.0]),
        Vertex::new([ 0.5, -0.5,  0.5], [1.0, 1.0, 0.5], [0.0, -1.0, 0.0], [1.0, 1.0]),
        Vertex::new([-0.5, -0.5,  0.5], [1.0, 1.0, 0.5], [0.0, -1.0, 0.0], [0.0, 1.0]),
        // Right face (cyan-ish), normal = +X
        Vertex::new([ 0.5, -0.5, -0.5], [0.3, 1.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0]),
        Vertex::new([ 0.5,  0.5, -0.5], [0.3, 1.0, 1.0], [1.0, 0.0, 0.0], [0.0, 0.0]),
        Vertex::new([ 0.5,  0.5,  0.5], [0.5, 1.0, 1.0], [1.0, 0.0, 0.0], [1.0, 0.0]),
        Vertex::new([ 0.5, -0.5,  0.5], [0.5, 1.0, 1.0], [1.0, 0.0, 0.0], [1.0, 1.0]),
        // Left face (magenta-ish), normal = -X
        Vertex::new([-0.5, -0.5, -0.5], [1.0, 0.3, 1.0], [-1.0, 0.0, 0.0], [1.0, 1.0]),
        Vertex::new([-0.5,  0.5, -0.5], [1.0, 0.3, 1.0], [-1.0, 0.0, 0.0], [1.0, 0.0]),
        Vertex::new([-0.5,  0.5,  0.5], [1.0, 0.5, 1.0], [-1.0, 0.0, 0.0], [0.0, 0.0]),
        Vertex::new([-0.5, -0.5,  0.5], [1.0, 0.5, 1.0], [-1.0, 0.0, 0.0], [0.0, 1.0]),
    ];

    let indices = vec![
        0,  1,  2,  2,  3,  0,  // front
        4,  6,  5,  6,  4,  7,  // back
        8,  9,  10, 10, 11, 8,  // top
        12, 14, 13, 14, 12, 15, // bottom
        16, 17, 18, 18, 19, 16, // right
        20, 22, 21, 22, 20, 23, // left
    ];

    (vertices, indices)
}

/// A single colored triangle in the XY plane at Z=0, with UVs and normals.
pub fn triangle_vertices(color: [f32; 3]) -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        Vertex::new([ 0.0,  0.5, 0.0], color, [0.0, 0.0, 1.0], [0.5, 0.0]),
        Vertex::new([-0.5, -0.5, 0.0], color, [0.0, 0.0, 1.0], [0.0, 1.0]),
        Vertex::new([ 0.5, -0.5, 0.0], color, [0.0, 0.0, 1.0], [1.0, 1.0]),
    ];
    let indices = vec![0, 1, 2];
    (vertices, indices)
}

/// A textured quad in the XY plane at Z=0, with normals.
pub fn quad_vertices() -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        Vertex::new([-0.5, -0.5, 0.0], [1.0, 1.0, 1.0], [0.0, 0.0, 1.0], [0.0, 1.0]),
        Vertex::new([ 0.5, -0.5, 0.0], [1.0, 1.0, 1.0], [0.0, 0.0, 1.0], [1.0, 1.0]),
        Vertex::new([ 0.5,  0.5, 0.0], [1.0, 1.0, 1.0], [0.0, 0.0, 1.0], [1.0, 0.0]),
        Vertex::new([-0.5,  0.5, 0.0], [1.0, 1.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
    ];
    let indices = vec![0, 1, 2, 2, 3, 0];
    (vertices, indices)
}

/// Fullscreen quad in NDC (clip space [-1,1]), for UI overlay compositing.
/// No transforms needed — vertices pass through directly to clip space.
pub fn fullscreen_quad_vertices() -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        Vertex::new([-1.0, -1.0, 0.0], [1.0, 1.0, 1.0], [0.0, 0.0, 1.0], [0.0, 1.0]),
        Vertex::new([ 1.0, -1.0, 0.0], [1.0, 1.0, 1.0], [0.0, 0.0, 1.0], [1.0, 1.0]),
        Vertex::new([ 1.0,  1.0, 0.0], [1.0, 1.0, 1.0], [0.0, 0.0, 1.0], [1.0, 0.0]),
        Vertex::new([-1.0,  1.0, 0.0], [1.0, 1.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
    ];
    let indices = vec![0, 1, 2, 2, 3, 0];
    (vertices, indices)
}
