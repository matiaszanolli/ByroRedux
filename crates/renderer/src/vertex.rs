//! Vertex format definition.

use ash::vk;

/// Per-vertex data: position + color + normal + UV + bone skinning (indices + weights).
///
/// `#[repr(C)]` ensures the layout matches what the shader expects.
///
/// The skinning fields are zeroed for rigid (non-skinned) vertices. The
/// vertex shader detects the rigid case by `sum(bone_weights) < epsilon`
/// and falls through to the per-instance `model` matrix in the instance
/// SSBO (set 1, binding 4) in that case — so every mesh (cube, quad, UI
/// overlay, NIF rigid, NIF skinned) runs through a single pipeline without
/// a second vertex format. See issue #178.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// Per-vertex bone indices (up to 4). Used only when `bone_weights`
    /// sums to > epsilon. Indices are local to the mesh's bone palette
    /// slot (see `VulkanContext::bone_palette` and the `bone_offset`
    /// field on `GpuInstance` in the instance SSBO).
    pub bone_indices: [u32; 4],
    /// Per-vertex bone weights (up to 4). Must sum to 1.0 for skinned
    /// vertices, or 0.0 for rigid vertices (the shader's rigid-path tag).
    pub bone_weights: [f32; 4],
}

impl Vertex {
    /// Construct a rigid (non-skinned) vertex. Bone indices and weights
    /// are zeroed; the shader's `sum(weights) < epsilon` check routes
    /// the vertex through the per-instance `model` matrix (instance SSBO)
    /// instead of the bone palette.
    pub const fn new(position: [f32; 3], color: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position,
            color,
            normal,
            uv,
            bone_indices: [0, 0, 0, 0],
            bone_weights: [0.0, 0.0, 0.0, 0.0],
        }
    }

    /// Construct a skinned vertex with explicit bone bindings. `bone_weights`
    /// should sum to approximately 1.0.
    pub const fn new_skinned(
        position: [f32; 3],
        color: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
        bone_indices: [u32; 4],
        bone_weights: [f32; 4],
    ) -> Self {
        Self {
            position,
            color,
            normal,
            uv,
            bone_indices,
            bone_weights,
        }
    }

    /// How vertex data is read from the buffer (stride, rate).
    pub fn binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription {
            binding: 0,
            stride: std::mem::size_of::<Self>() as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }
    }

    /// Per-attribute layout within a vertex.
    pub fn attribute_descriptions() -> [vk::VertexInputAttributeDescription; 6] {
        // Field offsets computed via memoffset-style arithmetic. `repr(C)`
        // guarantees no padding between the POD fields we use, so raw
        // prefix-sum math matches the struct layout.
        const OFF_POSITION: u32 = 0;
        const OFF_COLOR: u32 = OFF_POSITION + 12; // after [f32; 3]
        const OFF_NORMAL: u32 = OFF_COLOR + 12;
        const OFF_UV: u32 = OFF_NORMAL + 12;
        const OFF_BONE_INDICES: u32 = OFF_UV + 8; // after [f32; 2]
        const OFF_BONE_WEIGHTS: u32 = OFF_BONE_INDICES + 16; // after [u32; 4]
        [
            // location 0: position (vec3)
            vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: OFF_POSITION,
            },
            // location 1: color (vec3)
            vk::VertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: OFF_COLOR,
            },
            // location 2: normal (vec3)
            vk::VertexInputAttributeDescription {
                location: 2,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: OFF_NORMAL,
            },
            // location 3: uv (vec2)
            vk::VertexInputAttributeDescription {
                location: 3,
                binding: 0,
                format: vk::Format::R32G32_SFLOAT,
                offset: OFF_UV,
            },
            // location 4: bone_indices (uvec4) — rigid vertices pass all zeros.
            vk::VertexInputAttributeDescription {
                location: 4,
                binding: 0,
                format: vk::Format::R32G32B32A32_UINT,
                offset: OFF_BONE_INDICES,
            },
            // location 5: bone_weights (vec4) — rigid vertices pass all zeros.
            vk::VertexInputAttributeDescription {
                location: 5,
                binding: 0,
                format: vk::Format::R32G32B32A32_SFLOAT,
                offset: OFF_BONE_WEIGHTS,
            },
        ]
    }
}

/// Lightweight UI vertex: position + UV only (20 bytes).
///
/// The UI overlay just needs position (already in NDC) and texture
/// coordinates. Using this instead of the full 76-byte `Vertex` avoids
/// feeding unused color/normal/bone attributes through the vertex input.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UiVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

impl UiVertex {
    pub const fn new(position: [f32; 3], uv: [f32; 2]) -> Self {
        Self { position, uv }
    }

    pub fn binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription {
            binding: 0,
            stride: std::mem::size_of::<Self>() as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }
    }

    pub fn attribute_descriptions() -> [vk::VertexInputAttributeDescription; 2] {
        [
            // location 0: position (vec3)
            vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: 0,
            },
            // location 1: uv (vec2)
            vk::VertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: vk::Format::R32G32_SFLOAT,
                offset: 12, // after [f32; 3]
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn vertex_size_matches_attribute_stride() {
        // Total = 12 (pos) + 12 (color) + 12 (normal) + 8 (uv) + 16 (indices) + 16 (weights) = 76
        assert_eq!(size_of::<Vertex>(), 76);
    }

    #[test]
    fn attribute_offsets_match_struct_layout() {
        assert_eq!(offset_of!(Vertex, position), 0);
        assert_eq!(offset_of!(Vertex, color), 12);
        assert_eq!(offset_of!(Vertex, normal), 24);
        assert_eq!(offset_of!(Vertex, uv), 36);
        assert_eq!(offset_of!(Vertex, bone_indices), 44);
        assert_eq!(offset_of!(Vertex, bone_weights), 60);
    }

    #[test]
    fn rigid_vertex_has_zero_weight_sum() {
        let v = Vertex::new([0.0; 3], [1.0; 3], [0.0, 1.0, 0.0], [0.0; 2]);
        let sum: f32 = v.bone_weights.iter().sum();
        assert_eq!(sum, 0.0, "rigid marker: sum-of-weights must be exactly 0");
    }

    #[test]
    fn ui_vertex_size() {
        // Total = 12 (pos) + 8 (uv) = 20
        assert_eq!(size_of::<UiVertex>(), 20);
    }

    #[test]
    fn ui_vertex_offsets_match_struct_layout() {
        assert_eq!(offset_of!(UiVertex, position), 0);
        assert_eq!(offset_of!(UiVertex, uv), 12);
    }
}
